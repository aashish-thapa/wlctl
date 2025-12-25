use anyhow::Result;
use std::sync::Arc;

use crate::nm::NMClient;
use ratatui::{
    Frame,
    layout::{Alignment, Constraint, Direction, Flex, Layout},
    style::{Color, Style, Stylize},
    widgets::{Block, BorderType, Borders, Cell, Clear, Padding, Row, Table},
};

use crate::config::Config;

#[derive(Debug, Clone)]
pub struct Adapter {
    client: Arc<NMClient>,
    device_path: String,
    pub is_powered: bool,
    pub name: String,
    pub driver: Option<String>,
    pub vendor: Option<String>,
    pub supported_modes: Vec<String>,
    pub config: Arc<Config>,
}

impl Adapter {
    pub async fn new(
        client: Arc<NMClient>,
        device_path: String,
        config: Arc<Config>,
    ) -> Result<Self> {
        let is_powered = client.is_wireless_enabled().await?;
        let name = client.get_device_interface(&device_path).await?;

        // NetworkManager doesn't expose driver/vendor info directly via D-Bus
        // These would need to be read from sysfs or udev
        // For now, we'll leave them as None
        let driver = None;
        let vendor = None;

        // NetworkManager supports both station and AP modes on most hardware
        let supported_modes = vec!["station".to_string(), "ap".to_string()];

        Ok(Self {
            client,
            device_path,
            is_powered,
            name,
            driver,
            vendor,
            supported_modes,
            config,
        })
    }

    pub async fn refresh(&mut self) -> Result<()> {
        self.is_powered = self.client.is_wireless_enabled().await?;
        Ok(())
    }

    pub fn render(&self, frame: &mut Frame, device_addr: String) {
        let popup_layout = Layout::default()
            .direction(ratatui::layout::Direction::Vertical)
            .constraints([
                Constraint::Fill(1),
                Constraint::Length(9),
                Constraint::Fill(1),
            ])
            .flex(Flex::Start)
            .split(frame.area());

        let area = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Fill(1),
                Constraint::Min(80),
                Constraint::Fill(1),
            ])
            .split(popup_layout[1])[1];

        let mut rows = vec![
            Row::new(vec![
                Cell::from("name").style(Style::default().bold().yellow()),
                Cell::from(self.name.clone()),
            ]),
            Row::new(vec![
                Cell::from("address").style(Style::default().bold().yellow()),
                Cell::from(device_addr),
            ]),
            Row::new(vec![
                Cell::from("Supported modes").style(Style::default().bold().yellow()),
                Cell::from(self.supported_modes.clone().join(" ")),
            ]),
        ];

        if let Some(driver) = &self.driver {
            rows.push(Row::new(vec![
                Cell::from("driver").style(Style::default().bold().yellow()),
                Cell::from(driver.clone()),
            ]));
        }

        if let Some(vendor) = &self.vendor {
            rows.push(Row::new(vec![
                Cell::from("vendor").style(Style::default().bold().yellow()),
                Cell::from(vendor.clone()),
            ]));
        }

        let widths = [Constraint::Length(20), Constraint::Fill(1)];

        let device_infos_table = Table::new(rows, widths)
            .block(
                Block::default()
                    .title(" Adapter Infos ")
                    .title_style(Style::default().bold())
                    .title_alignment(Alignment::Center)
                    .padding(Padding::uniform(1))
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(Color::Green))
                    .border_type(BorderType::Thick),
            )
            .column_spacing(3)
            .row_highlight_style(Style::default().bg(Color::DarkGray).fg(Color::White));

        frame.render_widget(Clear, area);
        frame.render_widget(device_infos_table, area);
    }
}
