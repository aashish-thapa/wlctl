use anyhow::Result;
use std::sync::Arc;
use zbus::zvariant::OwnedObjectPath;

use crate::nm::{Mode, NMClient};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Row, Table, TableState},
};

use crate::{
    app::{AdapterView, FocusedBlock},
    config::Config,
    mode::{ap::AccessPoint, station::Station},
};

#[derive(Clone)]
pub struct Device {
    client: Arc<NMClient>,
    pub device_path: String,
    pub name: String,
    pub address: String,
    pub mode: Mode,
    pub is_powered: bool,
    pub station: Option<Station>,
    pub ap: Option<AccessPoint>,
}

impl Device {
    pub async fn new(client: Arc<NMClient>, device_path: OwnedObjectPath) -> Result<Self> {
        let device_path_str = device_path.as_str().to_string();

        let name = client.get_device_interface(&device_path_str).await?;
        let address = client.get_device_hw_address(&device_path_str).await?;
        let is_powered = client.is_wireless_enabled().await?;

        // Default to Station mode - NetworkManager doesn't have explicit mode switching
        // The mode is determined by the active connection type
        let mode = Mode::Station;

        let (station, ap) = if is_powered {
            match mode {
                Mode::Station => {
                    if let Ok(station) = Station::new(client.clone(), device_path_str.clone()).await
                    {
                        (Some(station), None)
                    } else {
                        (None, None)
                    }
                }
                Mode::Ap => {
                    if let Ok(ap) = AccessPoint::new(client.clone(), device_path_str.clone()).await
                    {
                        (None, Some(ap))
                    } else {
                        (None, None)
                    }
                }
            }
        } else {
            (None, None)
        };

        Ok(Self {
            client,
            device_path: device_path_str,
            name,
            address,
            mode,
            is_powered,
            station,
            ap,
        })
    }

    pub async fn set_mode(&mut self, mode: Mode) -> Result<()> {
        // In NetworkManager, we don't switch modes explicitly
        // Instead, we activate different connection types
        // For AP mode, we'll create a hotspot connection
        // For station mode, we connect to infrastructure networks
        self.mode = mode;

        // Reinitialize station or AP based on mode
        match mode {
            Mode::Station => {
                self.ap = None;
                if self.is_powered {
                    self.station = Station::new(self.client.clone(), self.device_path.clone())
                        .await
                        .ok();
                }
            }
            Mode::Ap => {
                self.station = None;
                if self.is_powered {
                    self.ap = AccessPoint::new(self.client.clone(), self.device_path.clone())
                        .await
                        .ok();
                }
            }
        }

        Ok(())
    }

    pub async fn power_off(&self) -> Result<()> {
        self.client.set_wireless_enabled(false).await?;
        Ok(())
    }

    pub async fn power_on(&self) -> Result<()> {
        self.client.set_wireless_enabled(true).await?;
        Ok(())
    }

    pub async fn refresh(&mut self) -> Result<()> {
        self.is_powered = self.client.is_wireless_enabled().await?;

        if self.is_powered {
            match self.mode {
                Mode::Station => {
                    if let Some(station) = &mut self.station {
                        station.refresh().await?;
                    } else {
                        self.station = Station::new(self.client.clone(), self.device_path.clone())
                            .await
                            .ok();
                    }
                }
                Mode::Ap => {
                    if let Some(ap) = &mut self.ap {
                        ap.refresh().await?;
                    } else {
                        self.ap = AccessPoint::new(self.client.clone(), self.device_path.clone())
                            .await
                            .ok();
                    }
                }
            }
        }
        Ok(())
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        focused_block: FocusedBlock,
        config: Arc<Config>,
        view: &AdapterView,
    ) {
        let (device_block, help_block) = {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(AdapterView::BLOCK_HEIGHT),
                    Constraint::Length(1),
                ])
                .margin(1)
                .split(frame.area());
            (chunks[1], chunks[2])
        };

        let is_powered = self.is_powered;
        let rows = view.build_rows(
            |adapter, marker| active_device_row(&adapter.name, is_powered, marker),
            |adapter| inactive_device_row(&adapter.name),
        );
        let widths = [
            Constraint::Length(16),
            Constraint::Length(8),
            Constraint::Length(6),
        ];

        let device_table = Table::new(rows, widths)
            .header({
                Row::new(vec![
                    Line::from("Name").yellow().centered(),
                    Line::from("Powered").yellow().centered(),
                    Line::from("Active").yellow().centered(),
                ])
                .style(Style::new().bold())
                .bottom_margin(1)
            })
            .block(
                Block::default()
                    .title(" Device ")
                    .title_style(Style::default().bold())
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green))
                    .border_type(BorderType::Thick)
                    .padding(Padding::horizontal(1)),
            )
            .column_spacing(1)
            .flex(Flex::SpaceAround)
            .row_highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White));

        let mut device_state =
            TableState::default().with_selected(view.table_selection(focused_block));
        frame.render_stateful_widget(device_table, device_block, &mut device_state);

        let help_message = Self::help_line(focused_block, &config, view.is_multi());
        frame.render_widget(help_message.centered().blue(), help_block);
    }

    fn help_line<'a>(focused_block: FocusedBlock, config: &Config, multi: bool) -> Line<'a> {
        match focused_block {
            FocusedBlock::Device => {
                let mut spans = vec![
                    Span::from(config.device.infos.to_string()).bold(),
                    Span::from(" Infos"),
                    Span::from(" | "),
                    Span::from(config.device.toggle_power.to_string()).bold(),
                    Span::from(" Toggle Power"),
                    Span::from(" | "),
                    Span::from(config.device.doctor.to_string()).bold(),
                    Span::from(" Doctor"),
                ];
                if multi {
                    spans.extend(adapter_nav_spans());
                }
                Line::from(spans)
            }
            FocusedBlock::AdapterInfos => {
                Line::from(vec![Span::from("󱊷 ").bold(), Span::from(" Discard")])
            }
            _ => Line::from(""),
        }
    }
}

/// Help-line fragment shown when adapter navigation is active. Shared by the
/// Device / Station / AP help rows so the keybinding hint stays in sync.
pub fn adapter_nav_spans<'a>() -> Vec<Span<'a>> {
    vec![
        Span::from(" | "),
        Span::from("j/k").bold(),
        Span::from(" Move"),
        Span::from(" | "),
        Span::from("⏎").bold(),
        Span::from(" Activate"),
    ]
}

fn active_device_row<'a>(name: &str, is_powered: bool, marker: &str) -> Row<'a> {
    Row::new(vec![
        Line::from(name.to_string()).centered(),
        Line::from(if is_powered { "On" } else { "Off" }).centered(),
        Line::from(marker.to_string()).centered(),
    ])
}

fn inactive_device_row<'a>(name: &str) -> Row<'a> {
    Row::new(vec![
        Line::from(name.to_string()).centered(),
        Line::from("-").centered(),
        Line::from("").centered(),
    ])
}
