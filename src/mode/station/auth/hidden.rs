use crate::nm::SecurityType;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Padding, Paragraph},
};
use tui_input::Input;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HiddenField {
    Ssid,
    Security,
    Password,
}

#[derive(Debug)]
pub struct HiddenSsidDialog {
    pub ssid: Input,
    pub password: Input,
    pub security: SecurityType,
    pub focused_field: HiddenField,
    pub show_password: bool,
}

impl Default for HiddenSsidDialog {
    fn default() -> Self {
        Self {
            ssid: Input::default(),
            password: Input::default(),
            security: SecurityType::WPA2,
            focused_field: HiddenField::Ssid,
            show_password: true,
        }
    }
}

impl HiddenSsidDialog {
    pub fn reset(&mut self) {
        self.ssid.reset();
        self.password.reset();
        self.security = SecurityType::WPA2;
        self.focused_field = HiddenField::Ssid;
        self.show_password = true;
    }

    pub fn cycle_security_next(&mut self) {
        self.security = match self.security {
            SecurityType::Open => SecurityType::WPA2,
            SecurityType::WPA2 => SecurityType::WPA3,
            SecurityType::WPA3 => SecurityType::Open,
            _ => SecurityType::WPA2,
        };
    }

    pub fn cycle_security_prev(&mut self) {
        self.security = match self.security {
            SecurityType::Open => SecurityType::WPA3,
            SecurityType::WPA2 => SecurityType::Open,
            SecurityType::WPA3 => SecurityType::WPA2,
            _ => SecurityType::WPA2,
        };
    }

    pub fn next_field(&mut self) {
        self.focused_field = match self.focused_field {
            HiddenField::Ssid => HiddenField::Security,
            HiddenField::Security => {
                if self.security == SecurityType::Open {
                    HiddenField::Ssid
                } else {
                    HiddenField::Password
                }
            }
            HiddenField::Password => HiddenField::Ssid,
        };
    }

    pub fn prev_field(&mut self) {
        self.focused_field = match self.focused_field {
            HiddenField::Ssid => {
                if self.security == SecurityType::Open {
                    HiddenField::Security
                } else {
                    HiddenField::Password
                }
            }
            HiddenField::Security => HiddenField::Ssid,
            HiddenField::Password => HiddenField::Security,
        };
    }

    pub fn requires_password(&self) -> bool {
        self.security != SecurityType::Open
    }

    pub fn render(&self, frame: &mut Frame) {
        let has_password = self.requires_password();
        let popup_height: u16 = if has_password { 16 } else { 12 };

        let popup_layout = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(popup_height),
                Constraint::Fill(1),
            ])
            .flex(ratatui::layout::Flex::SpaceBetween)
            .split(frame.area());

        let area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(60),
                Constraint::Fill(1),
            ])
            .flex(ratatui::layout::Flex::SpaceBetween)
            .split(popup_layout[1])[1];

        frame.render_widget(Clear, area);

        frame.render_widget(
            Block::new()
                .borders(Borders::ALL)
                .border_type(BorderType::Thick)
                .title(" Connect to Hidden Network ")
                .title_style(Style::default().bold().fg(Color::White))
                .style(Style::default())
                .border_style(Style::default().fg(Color::Green))
                .padding(Padding::new(2, 2, 1, 0)),
            area,
        );

        // Inner area (inside border + padding)
        let inner = Layout::default()
            .direction(Direction::Vertical)
            .constraints(if has_password {
                vec![
                    Constraint::Length(1), // SSID label
                    Constraint::Length(1), // SSID input
                    Constraint::Length(1), // spacer
                    Constraint::Length(1), // Security label
                    Constraint::Length(1), // Security selector
                    Constraint::Length(1), // spacer
                    Constraint::Length(1), // Password label
                    Constraint::Length(1), // Password input
                    Constraint::Length(1), // spacer
                    Constraint::Length(1), // show password toggle
                    Constraint::Length(1), // spacer
                    Constraint::Length(1), // hints
                ]
            } else {
                vec![
                    Constraint::Length(1), // SSID label
                    Constraint::Length(1), // SSID input
                    Constraint::Length(1), // spacer
                    Constraint::Length(1), // Security label
                    Constraint::Length(1), // Security selector
                    Constraint::Length(1), // spacer
                    Constraint::Length(1), // spacer
                    Constraint::Length(1), // hints
                ]
            })
            .split(Block::new().padding(Padding::new(2, 2, 1, 0)).inner(area));

        // SSID label
        let ssid_label = Paragraph::new(Line::from(vec![
            Span::raw("SSID").bold(),
            if self.focused_field == HiddenField::Ssid {
                Span::raw(" *").fg(Color::Green)
            } else {
                Span::raw("")
            },
        ]));
        frame.render_widget(ssid_label, inner[0]);

        // SSID input
        let ssid_str = self.ssid.value().to_string();
        let ssid_style = if self.focused_field == HiddenField::Ssid {
            Style::default().fg(Color::White).bg(Color::DarkGray)
        } else {
            Style::default().fg(Color::Gray).bg(Color::DarkGray)
        };
        let ssid_input = Paragraph::new(
            if ssid_str.is_empty() && self.focused_field != HiddenField::Ssid {
                Line::from(Span::raw("Network name").dim())
            } else {
                Line::from(ssid_str.clone())
            },
        )
        .style(ssid_style);
        frame.render_widget(ssid_input, inner[1]);

        // Security label
        let security_label = Paragraph::new(Line::from(vec![
            Span::raw("Security").bold(),
            if self.focused_field == HiddenField::Security {
                Span::raw(" *").fg(Color::Green)
            } else {
                Span::raw("")
            },
        ]));
        frame.render_widget(security_label, inner[3]);

        // Security selector
        let security_options = [SecurityType::Open, SecurityType::WPA2, SecurityType::WPA3];
        let security_spans: Vec<Span> = security_options
            .iter()
            .enumerate()
            .flat_map(|(i, sec)| {
                let label = match sec {
                    SecurityType::Open => "Open",
                    SecurityType::WPA2 => "WPA2",
                    SecurityType::WPA3 => "WPA3",
                    _ => "",
                };
                let styled = if *sec == self.security {
                    Span::raw(format!(" {} ", label))
                        .bold()
                        .fg(Color::Black)
                        .bg(Color::Green)
                } else {
                    Span::raw(format!(" {} ", label)).dim()
                };
                if i > 0 {
                    vec![Span::raw("  "), styled]
                } else {
                    vec![styled]
                }
            })
            .collect();

        let security_line = if self.focused_field == HiddenField::Security {
            let mut spans = security_spans;
            spans.push(Span::raw("  ←/→ to change").dim());
            spans
        } else {
            security_spans
        };

        frame.render_widget(Paragraph::new(Line::from(security_line)), inner[4]);

        if has_password {
            // Password label
            let password_label = Paragraph::new(Line::from(vec![
                Span::raw("Password").bold(),
                if self.focused_field == HiddenField::Password {
                    Span::raw(" *").fg(Color::Green)
                } else {
                    Span::raw("")
                },
            ]));
            frame.render_widget(password_label, inner[6]);

            // Password input
            let password_str = if self.show_password {
                self.password.value().to_string()
            } else {
                "*".repeat(self.password.value().len())
            };
            let password_style = if self.focused_field == HiddenField::Password {
                Style::default().fg(Color::White).bg(Color::DarkGray)
            } else {
                Style::default().fg(Color::Gray).bg(Color::DarkGray)
            };
            let password_input = Paragraph::new(
                if password_str.is_empty() && self.focused_field != HiddenField::Password {
                    Line::from(Span::raw("Enter password").dim())
                } else {
                    Line::from(password_str.clone())
                },
            )
            .style(password_style);
            frame.render_widget(password_input, inner[7]);

            // Show password toggle
            let toggle = Paragraph::new(Line::from(vec![
                if self.show_password {
                    Span::raw("󰈈 Visible")
                } else {
                    Span::raw("󰈉 Hidden")
                },
                Span::raw("  (ctrl+h to toggle)").dim(),
            ]));
            frame.render_widget(toggle, inner[9]);

            // Hints
            let hints = Paragraph::new(
                Line::from(vec![
                    Span::raw("Tab").bold(),
                    Span::raw(" Next  "),
                    Span::raw("Enter").bold(),
                    Span::raw(" Connect  "),
                    Span::raw("Esc").bold(),
                    Span::raw(" Cancel"),
                ])
                .centered(),
            )
            .dim();
            frame.render_widget(hints, inner[11]);
        } else {
            // Hints (no password)
            let hints = Paragraph::new(
                Line::from(vec![
                    Span::raw("Tab").bold(),
                    Span::raw(" Next  "),
                    Span::raw("Enter").bold(),
                    Span::raw(" Connect  "),
                    Span::raw("Esc").bold(),
                    Span::raw(" Cancel"),
                ])
                .centered(),
            )
            .dim();
            frame.render_widget(hints, inner[7]);
        }

        // Set cursor on active input field
        match self.focused_field {
            HiddenField::Ssid => {
                let cursor_x = inner[1].x + self.ssid.visual_cursor().min(ssid_str.len()) as u16;
                frame.set_cursor_position((cursor_x, inner[1].y));
            }
            HiddenField::Password if has_password => {
                let pwd_len = self.password.value().len();
                let cursor_x = inner[7].x + self.password.visual_cursor().min(pwd_len) as u16;
                frame.set_cursor_position((cursor_x, inner[7].y));
            }
            _ => {}
        }
    }
}
