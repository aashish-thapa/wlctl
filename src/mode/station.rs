use anyhow::Result;
pub mod auth;
pub mod known_network;
pub mod network;
pub mod share;
pub mod speed_test;

use std::sync::Arc;

use crate::nm::{DiagnosticInfo, NMClient, StationState};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Row, Table, TableState},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::FocusedBlock,
    config::Config,
    device::Device,
    event::Event,
    mode::station::{known_network::KnownNetwork, share::Share, speed_test::SpeedTest},
    notification::{Notification, NotificationLevel},
};

use network::Network;

/// Hidden network representation for NetworkManager
#[derive(Debug, Clone)]
pub struct HiddenNetwork {
    pub address: String,
    pub network_type: String,
    pub signal_strength: i16,
}

#[derive(Clone)]
pub struct Station {
    pub client: Arc<NMClient>,
    pub device_path: String,
    pub state: StationState,
    pub is_scanning: bool,
    pub connected_network: Option<Network>,
    pub is_ethernet_connected: bool,
    pub new_networks: Vec<(Network, i16)>,
    pub new_hidden_networks: Vec<HiddenNetwork>,
    pub known_networks: Vec<(Network, i16)>,
    pub unavailable_known_networks: Vec<KnownNetwork>,
    pub known_networks_state: TableState,
    pub new_networks_state: TableState,
    pub diagnostic: Option<DiagnosticInfo>,
    pub show_unavailable_known_networks: bool,
    pub show_hidden_networks: bool,
    pub share: Option<Share>,
    pub speed_test: Option<SpeedTest>,
}

impl Station {
    pub async fn new(client: Arc<NMClient>, device_path: String) -> Result<Self> {
        let device_state = client.get_device_state(&device_path).await?;
        let state = StationState::from(device_state);

        // Check if Ethernet is connected
        let is_ethernet_connected = client
            .has_active_ethernet_connection()
            .await
            .unwrap_or(false);

        // Get current connected network
        let connected_ssid = client.get_connected_ssid(&device_path).await?;

        // Get all visible access points
        let visible_networks = client.get_visible_networks(&device_path).await?;

        // Get all saved WiFi connections
        let saved_connections = client.get_wifi_connections().await?;

        // Build networks list
        let mut new_networks: Vec<(Network, i16)> = Vec::new();
        let mut known_networks: Vec<(Network, i16)> = Vec::new();
        let mut connected_network: Option<Network> = None;

        for ap_info in visible_networks {
            let is_connected = Some(&ap_info.ssid) == connected_ssid.as_ref();
            let signal = ap_info.strength as i16 * 100; // Convert 0-100 to match iwd format

            // Check if this network has a saved connection
            let known_network = saved_connections
                .iter()
                .find(|conn| conn.ssid == ap_info.ssid)
                .map(|conn| KnownNetwork::from_connection_info(client.clone(), conn.clone()));

            let network = Network::from_access_point(
                client.clone(),
                device_path.clone(),
                ap_info,
                known_network.clone(),
                is_connected,
            );

            if is_connected {
                connected_network = Some(network.clone());
            }

            if known_network.is_some() {
                known_networks.push((network, signal));
            } else {
                new_networks.push((network, signal));
            }
        }

        // Get unavailable known networks (saved but not visible)
        let visible_ssids: Vec<&str> = known_networks
            .iter()
            .map(|(n, _)| n.name.as_str())
            .collect();

        let unavailable_known_networks: Vec<KnownNetwork> = saved_connections
            .into_iter()
            .filter(|conn| !visible_ssids.contains(&conn.ssid.as_str()))
            .map(|conn| KnownNetwork::from_connection_info(client.clone(), conn))
            .collect();

        let mut new_networks_state = TableState::default();
        if new_networks.is_empty() {
            new_networks_state.select(None);
        } else {
            new_networks_state.select(Some(0));
        }

        let mut known_networks_state = TableState::default();
        if known_networks.is_empty() {
            known_networks_state.select(None);
        } else {
            known_networks_state.select(Some(0));
        }

        // Get diagnostic info if connected
        let diagnostic = if connected_network.is_some() {
            // Try to get active AP info for diagnostics
            if let Some(ap_path) = client.get_active_access_point(&device_path).await? {
                if let Ok(ap_info) = client.get_access_point_info(ap_path.as_str()).await {
                    Some(DiagnosticInfo {
                        frequency: Some(ap_info.frequency),
                        signal_strength: Some(ap_info.strength as i32),
                        security: Some(ap_info.security.to_string()),
                        ..Default::default()
                    })
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        };

        Ok(Self {
            client,
            device_path,
            state,
            is_scanning: false,
            connected_network,
            is_ethernet_connected,
            new_networks,
            new_hidden_networks: Vec::new(), // NetworkManager doesn't list hidden networks separately
            known_networks,
            unavailable_known_networks,
            known_networks_state,
            new_networks_state,
            diagnostic,
            show_unavailable_known_networks: false,
            show_hidden_networks: false,
            share: None,
            speed_test: None,
        })
    }

    pub async fn connect_hidden_network(
        &self,
        _ssid: String,
        _password: Option<&str>,
    ) -> Result<()> {
        // For hidden networks, we need to create a connection with the hidden flag
        // This is handled by add_and_activate_connection with special settings
        // For now, we'll return an error - full hidden network support needs more work
        Err(anyhow::anyhow!(
            "Hidden network connection not yet implemented for NetworkManager"
        ))
    }

    #[allow(clippy::collapsible_if)]
    pub async fn refresh(&mut self) -> Result<()> {
        let device_state = self.client.get_device_state(&self.device_path).await?;
        self.state = StationState::from(device_state);

        // Check if Ethernet is connected
        self.is_ethernet_connected = self
            .client
            .has_active_ethernet_connection()
            .await
            .unwrap_or(false);

        // Get current connected network
        let connected_ssid = self.client.get_connected_ssid(&self.device_path).await?;

        // Get all visible access points
        let visible_networks = self.client.get_visible_networks(&self.device_path).await?;

        // Get all saved WiFi connections
        let saved_connections = self.client.get_wifi_connections().await?;

        // Build networks list
        let mut new_networks: Vec<(Network, i16)> = Vec::new();
        let mut known_networks: Vec<(Network, i16)> = Vec::new();
        let mut connected_network: Option<Network> = None;

        for ap_info in visible_networks {
            let is_connected = Some(&ap_info.ssid) == connected_ssid.as_ref();
            let signal = ap_info.strength as i16 * 100;

            let known_network = saved_connections
                .iter()
                .find(|conn| conn.ssid == ap_info.ssid)
                .map(|conn| KnownNetwork::from_connection_info(self.client.clone(), conn.clone()));

            let network = Network::from_access_point(
                self.client.clone(),
                self.device_path.clone(),
                ap_info,
                known_network.clone(),
                is_connected,
            );

            if is_connected {
                connected_network = Some(network.clone());
            }

            if known_network.is_some() {
                known_networks.push((network, signal));
            } else {
                new_networks.push((network, signal));
            }
        }

        // Update network lists, preserving selection if possible
        if self.new_networks.len() == new_networks.len() {
            // Just update signal strengths
            self.new_networks.iter_mut().for_each(|(net, signal)| {
                if let Some((_, new_signal)) = new_networks.iter().find(|(n, _)| n.name == net.name)
                {
                    *signal = *new_signal;
                }
            });
        } else {
            let mut new_networks_state = TableState::default();
            if new_networks.is_empty() {
                new_networks_state.select(None);
            } else {
                new_networks_state.select(Some(0));
            }
            self.new_networks_state = new_networks_state;
            self.new_networks = new_networks;
        }

        if self.known_networks.len() == known_networks.len() {
            // Just update signal strengths and autoconnect status
            self.known_networks.iter_mut().for_each(|(net, signal)| {
                if let Some((refreshed_net, new_signal)) =
                    known_networks.iter().find(|(n, _)| n.name == net.name)
                {
                    if let Some(known) = &mut net.known_network {
                        if let Some(refreshed_known) = &refreshed_net.known_network {
                            known.is_autoconnect = refreshed_known.is_autoconnect;
                        }
                    }
                    *signal = *new_signal;
                }
            });
        } else {
            let mut known_networks_state = TableState::default();
            if known_networks.is_empty() {
                known_networks_state.select(None);
            } else {
                known_networks_state.select(Some(0));
            }
            self.known_networks_state = known_networks_state;
            self.known_networks = known_networks;
        }

        // Update unavailable known networks
        let visible_ssids: Vec<&str> = self
            .known_networks
            .iter()
            .map(|(n, _)| n.name.as_str())
            .collect();

        self.unavailable_known_networks = saved_connections
            .into_iter()
            .filter(|conn| !visible_ssids.contains(&conn.ssid.as_str()))
            .map(|conn| KnownNetwork::from_connection_info(self.client.clone(), conn))
            .collect();

        self.connected_network = connected_network;

        // Update diagnostic info
        if self.connected_network.is_some() {
            if let Some(ap_path) = self
                .client
                .get_active_access_point(&self.device_path)
                .await?
            {
                if let Ok(ap_info) = self.client.get_access_point_info(ap_path.as_str()).await {
                    self.diagnostic = Some(DiagnosticInfo {
                        frequency: Some(ap_info.frequency),
                        signal_strength: Some(ap_info.strength as i32),
                        security: Some(ap_info.security.to_string()),
                        ..Default::default()
                    });
                }
            }
        } else {
            self.diagnostic = None;
        }

        Ok(())
    }

    pub async fn scan(&mut self, sender: UnboundedSender<Event>) -> Result<()> {
        match self.client.request_scan(&self.device_path).await {
            Ok(()) => {
                self.is_scanning = true;
                Notification::send(
                    "Start Scanning".to_string(),
                    NotificationLevel::Info,
                    &sender,
                )?;
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("Scanning not allowed") {
                    Notification::send(
                        "Scanning in progress".to_string(),
                        NotificationLevel::Info,
                        &sender,
                    )?;
                } else {
                    Notification::send(msg, NotificationLevel::Error, &sender)?;
                }
            }
        }
        Ok(())
    }

    pub async fn disconnect(&self, sender: UnboundedSender<Event>) -> Result<()> {
        match self.client.disconnect_device(&self.device_path).await {
            Ok(()) => {
                if let Some(network) = &self.connected_network {
                    Notification::send(
                        format!("Disconnected from {}", network.name),
                        NotificationLevel::Info,
                        &sender,
                    )?;
                } else {
                    Notification::send(
                        "Disconnected".to_string(),
                        NotificationLevel::Info,
                        &sender,
                    )?;
                }
            }
            Err(e) => {
                Notification::send(e.to_string(), NotificationLevel::Error, &sender)?;
            }
        }
        Ok(())
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        focused_block: FocusedBlock,
        device: &Device,
        config: Arc<Config>,
    ) {
        let (known_networks_block, new_networks_block, device_block, help_block) = {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(5),
                    Constraint::Min(5),
                    Constraint::Length(5),
                    Constraint::Length(2),
                ])
                .margin(1)
                .split(frame.area());
            (chunks[0], chunks[1], chunks[2], chunks[3])
        };

        //
        // Device
        //
        let row = Row::new(vec![
            Line::from(device.name.clone()).centered(),
            Line::from("station").centered(),
            {
                if device.is_powered {
                    Line::from("On").centered()
                } else {
                    Line::from("Off").centered()
                }
            },
            Line::from(self.state.to_string()).centered(),
            Line::from(if self.is_scanning { "Yes" } else { "No" }).centered(),
            Line::from({
                if let Some(diagnostic) = &self.diagnostic {
                    if let Some(freq) = diagnostic.frequency {
                        format!("{:.2} GHz", freq as f32 / 1000.)
                    } else {
                        "-".to_string()
                    }
                } else {
                    "-".to_string()
                }
            })
            .centered(),
            Line::from({
                if let Some(diagnostic) = &self.diagnostic {
                    diagnostic.security.clone().unwrap_or("-".to_string())
                } else {
                    "-".to_string()
                }
            })
            .centered(),
        ]);

        let widths = [
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(15),
        ];

        let device_table = Table::new(vec![row], widths)
            .header({
                if focused_block == FocusedBlock::Device {
                    Row::new(vec![
                        Line::from("Name").yellow().centered(),
                        Line::from("Mode").yellow().centered(),
                        Line::from("Powered").yellow().centered(),
                        Line::from("State").yellow().centered(),
                        Line::from("Scanning").yellow().centered(),
                        Line::from("Frequency").yellow().centered(),
                        Line::from("Security").yellow().centered(),
                    ])
                    .style(Style::new().bold())
                    .bottom_margin(1)
                } else {
                    Row::new(vec![
                        Line::from("Name").centered(),
                        Line::from("Mode").centered(),
                        Line::from("Powered").centered(),
                        Line::from("State").centered(),
                        Line::from("Scanning").centered(),
                        Line::from("Frequency").centered(),
                        Line::from("Security").centered(),
                    ])
                    .bottom_margin(1)
                }
            })
            .block(
                Block::default()
                    .title(" Device ")
                    .title_style({
                        if focused_block == FocusedBlock::Device {
                            Style::default().bold()
                        } else {
                            Style::default()
                        }
                    })
                    .borders(Borders::ALL)
                    .border_style({
                        if focused_block == FocusedBlock::Device {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default()
                        }
                    })
                    .border_type({
                        if focused_block == FocusedBlock::Device {
                            BorderType::Thick
                        } else {
                            BorderType::default()
                        }
                    })
                    .padding(Padding::horizontal(1)),
            )
            .column_spacing(1)
            .flex(Flex::SpaceAround)
            .row_highlight_style(if focused_block == FocusedBlock::Device {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            });

        let mut device_state = TableState::default().with_selected(0);
        frame.render_stateful_widget(device_table, device_block, &mut device_state);

        //
        // Known networks
        //
        let is_ethernet = self.is_ethernet_connected;
        let mut rows: Vec<Row> = self
            .known_networks
            .iter()
            .map(|(net, signal)| {
                let known = net.known_network.as_ref().unwrap();
                let signal_percent = (*signal / 100).clamp(0, 100);
                let signal_str = format!("{}%", signal_percent);

                // Don't show WiFi connected icon when Ethernet is the primary connection
                if !is_ethernet
                    && let Some(connected_net) = &self.connected_network
                        && connected_net.name == net.name {
                            let row = vec![
                                Line::from("󰖩 ").centered(),
                                Line::from(known.name.clone()).centered(),
                                Line::from(known.network_type.to_string()).centered(),
                                Line::from(if known.is_hidden { "Yes" } else { "No" }).centered(),
                                Line::from(if known.is_autoconnect { "Yes" } else { "No" })
                                    .centered(),
                                Line::from(signal_str).centered(),
                            ];

                            return Row::new(row);
                        }

                let row = vec![
                    Line::from("").centered(),
                    Line::from(known.name.clone()).centered(),
                    Line::from(known.network_type.to_string()).centered(),
                    Line::from(if known.is_hidden { "Yes" } else { "No" }).centered(),
                    Line::from(if known.is_autoconnect { "Yes" } else { "No" }).centered(),
                    Line::from(signal_str).centered(),
                ];

                Row::new(row)
            })
            .collect();

        // Add Ethernet entry at the top when connected
        if self.is_ethernet_connected {
            let ethernet_row = Row::new(vec![
                Line::from("󰈀 ").centered(),
                Line::from("Ethernet").centered(),
                Line::from("-").centered(),
                Line::from("-").centered(),
                Line::from("-").centered(),
                Line::from("-").centered(),
            ]);
            rows.insert(0, ethernet_row);
        }

        if self.show_unavailable_known_networks {
            self.unavailable_known_networks.iter().for_each(|net| {
                let row = Row::new(vec![
                    Line::from(""),
                    Line::from(net.name.clone()).centered(),
                    Line::from(net.network_type.to_string()).centered(),
                    Line::from(""),
                    Line::from(""),
                    Line::from(""),
                ])
                .fg(Color::DarkGray);

                rows.push(row);
            });
        }

        let widths = [
            Constraint::Length(2),
            Constraint::Length(25),
            Constraint::Length(8),
            Constraint::Length(6),
            Constraint::Length(12),
            Constraint::Length(6),
        ];

        let known_networks_table = Table::new(rows, widths)
            .header({
                if focused_block == FocusedBlock::KnownNetworks {
                    Row::new(vec![
                        Line::from(""),
                        Line::from("Name").yellow().centered(),
                        Line::from("Security").yellow().centered(),
                        Line::from("Hidden").yellow().centered(),
                        Line::from("Auto Connect").yellow().centered(),
                        Line::from("Signal").yellow().centered(),
                    ])
                    .style(Style::new().bold())
                    .bottom_margin(1)
                } else {
                    Row::new(vec![
                        Line::from(""),
                        Line::from("Name").centered(),
                        Line::from("Security").centered(),
                        Line::from("Hidden").centered(),
                        Line::from("Auto Connect").centered(),
                        Line::from("Signal").centered(),
                    ])
                    .bottom_margin(1)
                }
            })
            .block(
                Block::default()
                    .title(" Known Networks ")
                    .title_style({
                        if focused_block == FocusedBlock::KnownNetworks {
                            Style::default().bold()
                        } else {
                            Style::default()
                        }
                    })
                    .borders(Borders::ALL)
                    .border_style({
                        if focused_block == FocusedBlock::KnownNetworks {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default()
                        }
                    })
                    .border_type({
                        if focused_block == FocusedBlock::KnownNetworks {
                            BorderType::Thick
                        } else {
                            BorderType::default()
                        }
                    })
                    .padding(Padding::horizontal(1)),
            )
            .column_spacing(1)
            .flex(Flex::SpaceAround)
            .row_highlight_style(if focused_block == FocusedBlock::KnownNetworks {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            });

        frame.render_stateful_widget(
            known_networks_table,
            known_networks_block,
            &mut self.known_networks_state,
        );

        //
        // New networks
        //
        let mut rows: Vec<Row> = self
            .new_networks
            .iter()
            .map(|(net, signal)| {
                let signal_percent = (*signal / 100).clamp(0, 100);
                Row::new(vec![
                    Line::from(net.name.clone()).centered(),
                    Line::from(net.network_type.to_string()).centered(),
                    Line::from({
                        match signal_percent {
                            n if n >= 75 => format!("{signal_percent:3}% 󰤨"),
                            n if (50..75).contains(&n) => format!("{signal_percent:3}% 󰤥"),
                            n if (25..50).contains(&n) => format!("{signal_percent:3}% 󰤢"),
                            _ => format!("{signal_percent:3}% 󰤟"),
                        }
                    })
                    .centered(),
                ])
            })
            .collect();

        if self.show_hidden_networks {
            self.new_hidden_networks.iter().for_each(|net| {
                let signal_percent = (net.signal_strength / 100).clamp(0, 100);
                rows.push(
                    Row::new(vec![
                        Line::from(net.address.clone()).centered(),
                        Line::from(net.network_type.clone()).centered(),
                        Line::from({
                            match signal_percent {
                                n if n >= 75 => format!("{signal_percent:3}% 󰤨"),
                                n if (50..75).contains(&n) => format!("{signal_percent:3}% 󰤥"),
                                n if (25..50).contains(&n) => format!("{signal_percent:3}% 󰤢"),
                                _ => format!("{signal_percent:3}% 󰤟"),
                            }
                        })
                        .centered(),
                    ])
                    .dark_gray(),
                )
            })
        };

        let widths = [
            Constraint::Length(25),
            Constraint::Length(15),
            Constraint::Length(8),
        ];

        let new_networks_table = Table::new(rows, widths)
            .header({
                if focused_block == FocusedBlock::NewNetworks {
                    Row::new(vec![
                        Line::from("Name").yellow().centered(),
                        Line::from("Security").yellow().centered(),
                        Line::from("Signal").yellow().centered(),
                    ])
                    .style(Style::new().bold())
                    .bottom_margin(1)
                } else {
                    Row::new(vec![
                        Line::from("Name").centered(),
                        Line::from("Security").centered(),
                        Line::from("Signal").centered(),
                    ])
                    .bottom_margin(1)
                }
            })
            .block(
                Block::default()
                    .title(" New Networks ")
                    .title_style({
                        if focused_block == FocusedBlock::NewNetworks {
                            Style::default().bold()
                        } else {
                            Style::default()
                        }
                    })
                    .borders(Borders::ALL)
                    .border_style({
                        if focused_block == FocusedBlock::NewNetworks {
                            Style::default().fg(Color::Green)
                        } else {
                            Style::default()
                        }
                    })
                    .border_type({
                        if focused_block == FocusedBlock::NewNetworks {
                            BorderType::Thick
                        } else {
                            BorderType::default()
                        }
                    })
                    .padding(Padding::horizontal(1)),
            )
            .column_spacing(1)
            .flex(Flex::SpaceAround)
            .row_highlight_style(if focused_block == FocusedBlock::NewNetworks {
                Style::default().bg(Color::DarkGray).fg(Color::White)
            } else {
                Style::default()
            });

        frame.render_stateful_widget(
            new_networks_table,
            new_networks_block,
            &mut self.new_networks_state,
        );

        let help_message = match focused_block {
            FocusedBlock::Device => vec![Line::from(vec![
                Span::from(config.station.start_scanning.to_string()).bold(),
                Span::from(" Scan"),
                Span::from(" | "),
                Span::from(config.device.infos.to_string()).bold(),
                Span::from(" Infos"),
                Span::from(" | "),
                Span::from(config.device.toggle_power.to_string()).bold(),
                Span::from(" Toggle Power"),
                Span::from(" | "),
                Span::from("ctrl+r").bold(),
                Span::from(" Switch Mode"),
                Span::from(" | "),
                Span::from("⇄").bold(),
                Span::from(" Nav"),
            ])],
            FocusedBlock::KnownNetworks => {
                if frame.area().width <= 130 {
                    vec![
                        Line::from(vec![
                            Span::from("󱁐  or ↵ ").bold(),
                            Span::from(" Dis/connect"),
                            Span::from(" | "),
                            Span::from(config.station.known_network.show_all.to_string()).bold(),
                            Span::from(" Show All"),
                            Span::from(" | "),
                            Span::from(config.station.known_network.remove.to_string()).bold(),
                            Span::from(" Remove"),
                            Span::from(" | "),
                            Span::from(config.station.known_network.share.to_string()).bold(),
                            Span::from(" Share"),
                            Span::from(" | "),
                            Span::from(config.station.start_scanning.to_string()).bold(),
                            Span::from(" Scan"),
                        ]),
                        Line::from(vec![
                            Span::from("k,").bold(),
                            Span::from("  Up"),
                            Span::from(" | "),
                            Span::from("j,").bold(),
                            Span::from("  Down"),
                            Span::from(" | "),
                            Span::from("⇄").bold(),
                            Span::from(" Nav"),
                            Span::from(" | "),
                            Span::from("ctrl+r").bold(),
                            Span::from(" Switch Mode"),
                            Span::from(" | "),
                            Span::from(config.station.known_network.toggle_autoconnect.to_string())
                                .bold(),
                            Span::from(" Autoconnect"),
                            Span::from(" | "),
                            Span::from(config.station.known_network.speed_test.to_string()).bold(),
                            Span::from(" Speed"),
                        ]),
                    ]
                } else {
                    vec![Line::from(vec![
                        Span::from("k,").bold(),
                        Span::from("  Up"),
                        Span::from(" | "),
                        Span::from("j,").bold(),
                        Span::from("  Down"),
                        Span::from(" | "),
                        Span::from("󱁐  or ↵ ").bold(),
                        Span::from(" Dis/connect"),
                        Span::from(" | "),
                        Span::from(config.station.known_network.show_all.to_string()).bold(),
                        Span::from(" Show All"),
                        Span::from(" | "),
                        Span::from(config.station.known_network.remove.to_string()).bold(),
                        Span::from(" Remove"),
                        Span::from(" | "),
                        Span::from(config.station.known_network.toggle_autoconnect.to_string())
                            .bold(),
                        Span::from(" Autoconnect"),
                        Span::from(" | "),
                        Span::from(config.station.start_scanning.to_string()).bold(),
                        Span::from(" Scan"),
                        Span::from(" | "),
                        Span::from(config.station.known_network.share.to_string()).bold(),
                        Span::from(" Share"),
                        Span::from(" | "),
                        Span::from(config.station.known_network.speed_test.to_string()).bold(),
                        Span::from(" Speed"),
                        Span::from(" | "),
                        Span::from("ctrl+r").bold(),
                        Span::from(" Switch Mode"),
                        Span::from(" | "),
                        Span::from("⇄").bold(),
                        Span::from(" Nav"),
                    ])]
                }
            }
            FocusedBlock::NewNetworks => {
                if frame.area().width < 80 {
                    vec![
                        Line::from(vec![
                            Span::from("󱁐  or ↵ ").bold(),
                            Span::from(" Connect"),
                            Span::from(" | "),
                            Span::from(config.station.start_scanning.to_string()).bold(),
                            Span::from(" Scan"),
                        ]),
                        Line::from(vec![
                            Span::from("k,").bold(),
                            Span::from("  Up"),
                            Span::from(" | "),
                            Span::from("j,").bold(),
                            Span::from("  Down"),
                            Span::from(" | "),
                            Span::from("ctrl+r").bold(),
                            Span::from(" Switch Mode"),
                            Span::from(" | "),
                            Span::from("⇄").bold(),
                            Span::from(" Nav"),
                        ]),
                    ]
                } else {
                    vec![Line::from(vec![
                        Span::from("k,").bold(),
                        Span::from("  Up"),
                        Span::from(" | "),
                        Span::from("j,").bold(),
                        Span::from("  Down"),
                        Span::from(" | "),
                        Span::from("󱁐  or ↵ ").bold(),
                        Span::from(" Connect"),
                        Span::from(" | "),
                        Span::from(config.station.new_network.show_all.to_string()).bold(),
                        Span::from(" Show All"),
                        Span::from(" | "),
                        Span::from(config.station.start_scanning.to_string()).bold(),
                        Span::from(" Scan"),
                        Span::from(" | "),
                        Span::from("ctrl+r").bold(),
                        Span::from(" Switch Mode"),
                        Span::from(" | "),
                        Span::from("⇄").bold(),
                        Span::from(" Nav"),
                    ])]
                }
            }
            FocusedBlock::AdapterInfos => {
                vec![Line::from(vec![
                    Span::from("󱊷 ").bold(),
                    Span::from(" Discard"),
                ])]
            }
            FocusedBlock::PskAuthKey => vec![Line::from(vec![
                Span::from(" ↵ ").bold(),
                Span::from(" Apply"),
                Span::from(" | "),
                Span::from("⇄").bold(),
                Span::from(" Hide/Show password"),
                Span::from(" | "),
                Span::from("󱊷 ").bold(),
                Span::from(" Discard"),
            ])],
            FocusedBlock::WpaEntrepriseAuth => vec![Line::from(vec![
                Span::from(" ↵ ").bold(),
                Span::from(" Apply"),
                Span::from(" | "),
                Span::from("h,l,←,→").bold(),
                Span::from(" Switch EAP/Method"),
                Span::from(" | "),
                Span::from("󱊷 ").bold(),
                Span::from(" Discard"),
                Span::from(" | "),
                Span::from("⇄").bold(),
                Span::from(" Nav"),
            ])],
            _ => vec![Line::from(vec![
                Span::from("󱊷 ").bold(),
                Span::from(" Discard"),
            ])],
        };

        let help_message = Paragraph::new(help_message).centered().blue();

        frame.render_widget(help_message, help_block);

        // Share
        if let Some(share) = &self.share {
            share.render(frame);
        }
    }
}
