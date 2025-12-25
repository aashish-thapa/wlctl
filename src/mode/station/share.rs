use anyhow::Result;
use qrcode::QrCode;
use std::{cmp, fs};
use tui_qrcode::{Colors, QrCodeWidget};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout, Rect},
    style::{Color, Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, BorderType, Borders, Clear},
};

#[derive(Clone)]
pub struct Share {
    pub qr_code: QrCode,
    pub network_name: String,
    pub passphrase: String,
}

impl Share {
    pub fn new(network_name: String) -> Result<Self> {
        // NetworkManager stores connections in /etc/NetworkManager/system-connections/
        // Files are named <connection-id>.nmconnection
        let nm_path = format!(
            "/etc/NetworkManager/system-connections/{}.nmconnection",
            network_name
        );

        let content = fs::read_to_string(&nm_path).or_else(|_| {
            // Try alternative naming conventions
            // NetworkManager may encode special characters
            let escaped_name = network_name.replace(' ', "_");
            fs::read_to_string(format!(
                "/etc/NetworkManager/system-connections/{}.nmconnection",
                escaped_name
            ))
        })?;

        // Parse the NetworkManager connection file (INI-like format)
        // Look for psk= in [wifi-security] section
        let mut in_wifi_security = false;
        let mut passphrase = None;

        for line in content.lines() {
            let line = line.trim();
            if line == "[wifi-security]" {
                in_wifi_security = true;
                continue;
            }
            if line.starts_with('[') {
                in_wifi_security = false;
                continue;
            }
            if in_wifi_security && line.starts_with("psk=") {
                passphrase = Some(line.trim_start_matches("psk=").to_string());
                break;
            }
        }

        let passphrase = passphrase
            .ok_or_else(|| anyhow::anyhow!("No password found for network {}", network_name))?;

        let message = format!("WIFI:T:WPA;S:{network_name};P:{passphrase};;");
        let qr_code = QrCode::new(message)?;

        Ok(Self {
            qr_code,
            network_name,
            passphrase,
        })
    }

    pub fn render(&self, frame: &mut Frame) {
        let widget = QrCodeWidget::new(self.qr_code.clone()).colors(Colors::Inverted);
        let sim_area = Rect::new(0, 0, 50, 50);
        let size = widget.size(sim_area);

        let block_width = cmp::max(size.width as usize, self.passphrase.len() + 12) + 6;

        let block = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(size.height + 12),
                Constraint::Fill(1),
            ])
            .flex(Flex::SpaceBetween)
            .split(frame.area())[1];

        let block = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(block_width as u16),
                Constraint::Fill(1),
            ])
            .flex(Flex::SpaceBetween)
            .split(block)[1];

        let (title_block, mut qr_block, passphrase_block) = {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3),
                    Constraint::Fill(1),
                    Constraint::Length(3),
                ])
                .margin(3)
                .flex(Flex::SpaceBetween)
                .split(block);

            (chunks[0], chunks[1], chunks[2])
        };

        frame.render_widget(Clear, block);
        frame.render_widget(
            Block::new()
                .borders(Borders::all())
                .border_type(BorderType::Thick)
                .border_style(Style::new().green()),
            block,
        );
        frame.render_widget(
            Text::from(self.network_name.clone()).centered().bold(),
            title_block,
        );

        if (size.width as usize) < block_width {
            qr_block = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(size.width),
                    Constraint::Fill(1),
                ])
                .flex(Flex::SpaceBetween)
                .split(qr_block)[1];
        }

        frame.render_widget(widget, qr_block);

        let passphrase = Text::from(vec![
            Line::from(""),
            Line::from(vec![
                Span::from("Passphrase: "),
                Span::from(&self.passphrase).bold().bg(Color::DarkGray),
            ])
            .centered(),
        ]);
        frame.render_widget(passphrase, passphrase_block);
    }
}
