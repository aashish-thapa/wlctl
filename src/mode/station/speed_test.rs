use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear},
};
use std::process::Stdio;
use tokio::process::Command;
use tokio::sync::mpsc::UnboundedSender;

use crate::event::Event;
use crate::notification::{Notification, NotificationLevel};

#[derive(Debug, Clone, Default)]
pub struct SpeedTest {
    pub is_running: bool,
    pub download: Option<String>,
    pub upload: Option<String>,
    pub ping: Option<String>,
    pub error: Option<String>,
}

impl SpeedTest {
    pub fn new() -> Self {
        Self {
            is_running: true,
            download: None,
            upload: None,
            ping: None,
            error: None,
        }
    }

    pub async fn run(sender: UnboundedSender<Event>) -> SpeedTest {
        let mut result = SpeedTest::new();

        // Check if speedtest-cli is available
        let check = Command::new("which").arg("speedtest-cli").output().await;

        if check.is_err() || !check.unwrap().status.success() {
            result.error =
                Some("speedtest-cli not found. Install: pip install speedtest-cli".to_string());
            result.is_running = false;
            let _ = Notification::send(
                "speedtest-cli not installed".to_string(),
                NotificationLevel::Error,
                &sender,
            );
            return result;
        }

        // Run speedtest-cli with simple output
        let output = Command::new("speedtest-cli")
            .arg("--simple")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .await;

        match output {
            Ok(output) => {
                if output.status.success() {
                    let stdout = String::from_utf8_lossy(&output.stdout);
                    // Parse output format:
                    // Ping: 12.345 ms
                    // Download: 94.23 Mbit/s
                    // Upload: 45.67 Mbit/s
                    for line in stdout.lines() {
                        if line.starts_with("Ping:") {
                            result.ping = Some(line.replace("Ping:", "").trim().to_string());
                        } else if line.starts_with("Download:") {
                            result.download =
                                Some(line.replace("Download:", "").trim().to_string());
                        } else if line.starts_with("Upload:") {
                            result.upload = Some(line.replace("Upload:", "").trim().to_string());
                        }
                    }
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    result.error = Some(format!("Speed test failed: {}", stderr.trim()));
                }
            }
            Err(e) => {
                result.error = Some(format!("Failed to run speed test: {}", e));
            }
        }

        result.is_running = false;
        result
    }

    pub fn render(&self, frame: &mut Frame) {
        let block_height = if self.is_running { 7 } else { 11 };
        let block_width = 45;

        let block = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(block_height),
                Constraint::Fill(1),
            ])
            .flex(Flex::SpaceBetween)
            .split(frame.area())[1];

        let block = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(block_width),
                Constraint::Fill(1),
            ])
            .flex(Flex::SpaceBetween)
            .split(block)[1];

        frame.render_widget(Clear, block);

        let border_color = if self.error.is_some() {
            Color::Red
        } else if self.is_running {
            Color::Yellow
        } else {
            Color::Green
        };

        frame.render_widget(
            Block::new()
                .borders(Borders::all())
                .border_type(BorderType::Thick)
                .border_style(Style::new().fg(border_color)),
            block,
        );

        let content_block = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Fill(1)])
            .margin(2)
            .split(block)[0];

        let content = if self.is_running {
            Text::from(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Running speed test...",
                    Style::default().fg(Color::Yellow).bold(),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "  Please wait, this may take ~30s",
                    Style::default().fg(Color::Gray),
                )),
            ])
        } else if let Some(error) = &self.error {
            Text::from(vec![
                Line::from(""),
                Line::from(Span::styled(
                    "  Error",
                    Style::default().fg(Color::Red).bold(),
                )),
                Line::from(""),
                Line::from(format!("  {}", error)),
            ])
        } else {
            Text::from(vec![
                Span::styled(" Speed Test Results ", Style::default().bold()).into_centered_line(),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ↓ Download:  ", Style::default().fg(Color::Cyan).bold()),
                    Span::from(self.download.clone().unwrap_or("-".to_string())),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled(
                        "  ↑ Upload:    ",
                        Style::default().fg(Color::Magenta).bold(),
                    ),
                    Span::from(self.upload.clone().unwrap_or("-".to_string())),
                ]),
                Line::from(""),
                Line::from(vec![
                    Span::styled("  ◷ Ping:      ", Style::default().fg(Color::Yellow).bold()),
                    Span::from(self.ping.clone().unwrap_or("-".to_string())),
                ]),
            ])
        };

        frame.render_widget(content, content_block);

        // Help text at bottom
        if !self.is_running {
            let help_block = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Fill(1), Constraint::Length(1)])
                .margin(1)
                .split(block)[1];

            let help = Line::from(vec![
                Span::styled("Press ", Style::default().fg(Color::DarkGray)),
                Span::styled("Esc", Style::default().fg(Color::White).bold()),
                Span::styled(" to close", Style::default().fg(Color::DarkGray)),
            ])
            .centered();

            frame.render_widget(help, help_block);
        }
    }
}
