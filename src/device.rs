use anyhow::Context;
use anyhow::Result;
use std::sync::Arc;

use crate::nm::{Mode, NMClient};

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Row, Table, TableState},
};

use crate::{
    app::FocusedBlock,
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
    pub async fn new(client: Arc<NMClient>) -> Result<Self> {
        let device_path = client
            .get_wifi_device()
            .await
            .context("No WiFi device found")?;
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

    pub fn render(&mut self, frame: &mut Frame, focused_block: FocusedBlock, config: Arc<Config>) {
        let (device_block, help_block) = {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Fill(1),
                    Constraint::Length(5),
                    Constraint::Length(1),
                ])
                .margin(1)
                .split(frame.area());
            (chunks[1], chunks[2])
        };

        //
        // Device
        //
        let row = Row::new(vec![Line::from(self.name.clone()).centered(), {
            if self.is_powered {
                Line::from("On").centered()
            } else {
                Line::from("Off").centered()
            }
        }]);

        let widths = [Constraint::Length(10), Constraint::Length(8)];

        let device_table = Table::new(vec![row], widths)
            .header({
                Row::new(vec![
                    Line::from("Name").yellow().centered(),
                    Line::from("Powered").yellow().centered(),
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

        let mut device_state = TableState::default().with_selected(0);
        frame.render_stateful_widget(device_table, device_block, &mut device_state);

        let help_message = match focused_block {
            FocusedBlock::Device => Line::from(vec![
                Span::from(config.device.infos.to_string()).bold(),
                Span::from(" Infos"),
                Span::from(" | "),
                Span::from(config.device.toggle_power.to_string()).bold(),
                Span::from(" Toggle Power"),
            ]),
            FocusedBlock::AdapterInfos => {
                Line::from(vec![Span::from("󱊷 ").bold(), Span::from(" Discard")])
            }
            _ => Line::from(""),
        };

        let help_message = help_message.centered().blue();

        frame.render_widget(help_message, help_block);
    }
}
