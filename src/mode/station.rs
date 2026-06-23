use anyhow::Result;
pub mod auth;
pub mod known_network;
pub mod network;
pub mod share;
pub mod speed_test;

use std::sync::Arc;

use crate::nm::{
    AccessPointInfo, ConnectionInfo, Connectivity, DiagnosticInfo, LinkKind, NMClient, PrimaryLink,
    StationState,
};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Flex, Layout},
    style::{Color, Style, Stylize},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Padding, Paragraph, Row, Table, TableState},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::{AdapterView, FocusedBlock},
    config::Config,
    device::{Device, adapter_nav_spans},
    event::Event,
    mode::station::{known_network::KnownNetwork, share::Share, speed_test::SpeedTest},
    notification::{Notification, NotificationLevel},
};

use network::Network;

/// Row for an adapter that is *not* the active Station. Runtime columns are
/// dashed out because state/scanning/frequency/security only exist on the
/// active adapter; the name and mode are enough for the user to pick it.
fn inactive_station_row<'a>(name: &str) -> Row<'a> {
    Row::new(vec![
        Line::from(name.to_string()).centered(),
        Line::from("station").centered(),
        Line::from("-").centered(),
        Line::from("-").centered(),
        Line::from("-").centered(),
        Line::from("-").centered(),
        Line::from("-").centered(),
        Line::from("").centered(),
    ])
}

/// Caption for the Known Networks box naming the link that currently carries
/// internet traffic, plus a hint when that link isn't actually online (e.g.
/// stuck behind a captive portal). `None` when the route is something not shown
/// in this list (e.g. a VPN, which the top-right badge already reports).
fn internet_caption(primary: Option<&PrimaryLink>, connectivity: Connectivity) -> Option<String> {
    let primary = primary?;
    let label = match primary.kind {
        LinkKind::Wifi => format!("WiFi · {}", primary.id),
        LinkKind::Ethernet => "Ethernet".to_string(),
        LinkKind::Other => return None,
    };
    let status = match connectivity {
        Connectivity::Portal => " · captive portal",
        Connectivity::Limited => " · no internet",
        Connectivity::None => " · offline",
        Connectivity::Full | Connectivity::Unknown => "",
    };
    Some(format!(" 󰖟 Internet: {label}{status} "))
}

/// Result of resolving the selected known networks table index,
/// accounting for the ethernet row offset.
pub enum KnownNetworkSelection {
    /// The ethernet row is selected (no-op for most actions)
    Ethernet,
    /// A known (visible) network at the given data index
    Network(usize),
    /// An unavailable (saved but not visible) network at the given index
    Unavailable(usize),
}

/// Network-status context passed into [`Station::render`]: which link owns the
/// default route and the current connectivity. Bundled so the render signature
/// stays small.
#[derive(Clone, Copy)]
pub struct StationStatus<'a> {
    pub primary: Option<&'a PrimaryLink>,
    pub connectivity: Connectivity,
}

/// Result of resolving the selected New Networks table row to real data,
/// accounting for the SSID filter and the trailing hidden-networks rows.
pub enum NewNetworkSelection {
    /// A visible scanned network at the given `new_networks` index
    Visible(usize),
    /// A hidden network at the given `new_hidden_networks` index
    Hidden(usize),
}

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
    /// Primary IPv4 of the active adapter, cached for the Device-box caption so
    /// the LAN address (handy for SSH) is visible without leaving the TUI.
    pub ipv4: Option<String>,
    /// Live SSID filter for the New Networks list. Empty = inactive.
    pub filter_query: String,
    /// Whether the New Networks filter is currently being typed into.
    pub filter_input: bool,
}

impl Station {
    pub async fn new(client: Arc<NMClient>, device_path: String) -> Result<Self> {
        let device_state = client.get_device_state(&device_path).await?;
        let state = StationState::from(device_state);

        let is_ethernet_connected = client
            .has_active_ethernet_connection()
            .await
            .unwrap_or(false);

        let connected_ssid = client.get_connected_ssid(&device_path).await?;

        // Request a fresh scan so we get up-to-date AP data.
        // This is non-blocking; NM populates APs asynchronously and
        // the periodic refresh() tick will pick them up.
        let _ = client.request_scan(&device_path).await;

        let visible_networks = client.get_visible_networks(&device_path).await?;
        let saved_connections = client.get_wifi_connections().await?;

        let (new_networks, known_networks, connected_network) = Self::categorize_networks(
            &client,
            &device_path,
            &visible_networks,
            &saved_connections,
            &connected_ssid,
        );

        let unavailable_known_networks =
            Self::find_unavailable_networks(&client, &known_networks, saved_connections);

        let diagnostic =
            Self::fetch_diagnostic(&client, &device_path, connected_network.is_some()).await?;

        let ipv4 = Self::fetch_device_ipv4(&client, &device_path).await;

        Ok(Self {
            client,
            device_path,
            state,
            is_scanning: false,
            connected_network,
            is_ethernet_connected,
            new_networks_state: Self::table_state_for(&new_networks),
            new_networks,
            new_hidden_networks: Vec::new(),
            known_networks_state: Self::table_state_for(&known_networks),
            known_networks,
            unavailable_known_networks,
            diagnostic,
            show_unavailable_known_networks: false,
            show_hidden_networks: false,
            share: None,
            speed_test: None,
            ipv4,
            filter_query: String::new(),
            filter_input: false,
        })
    }

    /// Reads the active adapter's primary IPv4, if any. Best-effort: a missing
    /// or unreadable address just yields `None`.
    async fn fetch_device_ipv4(client: &Arc<NMClient>, device_path: &str) -> Option<String> {
        client
            .get_ip4_info(device_path)
            .await
            .ok()
            .flatten()
            .and_then(|ip| ip.addresses.into_iter().next().map(|(addr, _)| addr))
    }

    pub async fn refresh(&mut self) -> Result<()> {
        let device_state = self.client.get_device_state(&self.device_path).await?;
        self.state = StationState::from(device_state);

        self.is_ethernet_connected = self
            .client
            .has_active_ethernet_connection()
            .await
            .unwrap_or(false);

        let connected_ssid = self.client.get_connected_ssid(&self.device_path).await?;
        let visible_networks = self.client.get_visible_networks(&self.device_path).await?;
        let saved_connections = self.client.get_wifi_connections().await?;

        let (new_networks, known_networks, connected_network) = Self::categorize_networks(
            &self.client,
            &self.device_path,
            &visible_networks,
            &saved_connections,
            &connected_ssid,
        );

        self.update_network_list(
            &new_networks,
            |s| &mut s.new_networks,
            |s| &mut s.new_networks_state,
        );
        self.update_known_network_list(&known_networks);

        self.unavailable_known_networks =
            Self::find_unavailable_networks(&self.client, &self.known_networks, saved_connections);

        self.connected_network = connected_network;
        self.diagnostic = Self::fetch_diagnostic(
            &self.client,
            &self.device_path,
            self.connected_network.is_some(),
        )
        .await?;

        self.ipv4 = Self::fetch_device_ipv4(&self.client, &self.device_path).await;

        Ok(())
    }

    /// Categorize visible APs into known networks, new networks, and the connected network.
    #[allow(clippy::type_complexity)]
    fn categorize_networks(
        client: &Arc<NMClient>,
        device_path: &str,
        visible_networks: &[AccessPointInfo],
        saved_connections: &[ConnectionInfo],
        connected_ssid: &Option<String>,
    ) -> (Vec<(Network, i16)>, Vec<(Network, i16)>, Option<Network>) {
        let mut new_networks: Vec<(Network, i16)> = Vec::new();
        let mut known_networks: Vec<(Network, i16)> = Vec::new();
        let mut connected_network: Option<Network> = None;

        for ap_info in visible_networks {
            let is_connected = Some(&ap_info.ssid) == connected_ssid.as_ref();
            let signal = ap_info.strength as i16 * 100;

            let known_network = saved_connections
                .iter()
                .find(|conn| conn.ssid == ap_info.ssid)
                .map(|conn| KnownNetwork::from_connection_info(client.clone(), conn.clone()));

            let network = Network::from_access_point(
                client.clone(),
                device_path.to_string(),
                ap_info.clone(),
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

        (new_networks, known_networks, connected_network)
    }

    /// Find saved connections that are not currently visible.
    fn find_unavailable_networks(
        client: &Arc<NMClient>,
        known_networks: &[(Network, i16)],
        saved_connections: Vec<ConnectionInfo>,
    ) -> Vec<KnownNetwork> {
        let visible_ssids: Vec<&str> = known_networks
            .iter()
            .map(|(n, _)| n.name.as_str())
            .collect();

        saved_connections
            .into_iter()
            .filter(|conn| !visible_ssids.contains(&conn.ssid.as_str()))
            .map(|conn| KnownNetwork::from_connection_info(client.clone(), conn))
            .collect()
    }

    /// Fetch diagnostic info for the active access point.
    async fn fetch_diagnostic(
        client: &NMClient,
        device_path: &str,
        is_connected: bool,
    ) -> Result<Option<DiagnosticInfo>> {
        if !is_connected {
            return Ok(None);
        }
        if let Some(ap_path) = client.get_active_access_point(device_path).await?
            && let Ok(ap_info) = client.get_access_point_info(ap_path.as_str()).await
        {
            return Ok(Some(DiagnosticInfo {
                frequency: Some(ap_info.frequency),
                signal_strength: Some(ap_info.strength as i32),
                security: Some(ap_info.security.to_string()),
                ..Default::default()
            }));
        }
        Ok(None)
    }

    /// Create a TableState with the first item selected if the list is non-empty.
    fn table_state_for<T>(items: &[T]) -> TableState {
        let mut state = TableState::default();
        state.select(if items.is_empty() { None } else { Some(0) });
        state
    }

    /// Update a network list, preserving selection when the same set of networks is present.
    fn update_network_list(
        &mut self,
        fresh: &[(Network, i16)],
        get_list: fn(&mut Self) -> &mut Vec<(Network, i16)>,
        get_state: fn(&mut Self) -> &mut TableState,
    ) {
        let current = get_list(self);
        let same_set = current.len() == fresh.len()
            && current
                .iter()
                .all(|(net, _)| fresh.iter().any(|(n, _)| n.name == net.name));

        if same_set {
            current.iter_mut().for_each(|(net, signal)| {
                if let Some((_, new_signal)) = fresh.iter().find(|(n, _)| n.name == net.name) {
                    *signal = *new_signal;
                }
            });
        } else {
            let state = get_state(self);
            *state = Self::table_state_for(fresh);
            *get_list(self) = fresh.to_vec();
        }
    }

    /// Update known network list, also syncing autoconnect status.
    fn update_known_network_list(&mut self, fresh: &[(Network, i16)]) {
        let same_set = self.known_networks.len() == fresh.len()
            && self
                .known_networks
                .iter()
                .all(|(net, _)| fresh.iter().any(|(n, _)| n.name == net.name));

        if same_set {
            self.known_networks.iter_mut().for_each(|(net, signal)| {
                if let Some((refreshed_net, new_signal)) =
                    fresh.iter().find(|(n, _)| n.name == net.name)
                {
                    if let Some(known) = &mut net.known_network
                        && let Some(refreshed_known) = &refreshed_net.known_network
                    {
                        known.is_autoconnect = refreshed_known.is_autoconnect;
                    }
                    *signal = *new_signal;
                }
            });
        } else {
            self.known_networks_state = Self::table_state_for(fresh);
            self.known_networks = fresh.to_vec();
        }
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

    /// Resolve the currently selected known networks table index to a typed selection,
    /// accounting for the ethernet row offset and unavailable networks.
    pub fn resolve_known_selection(&self) -> Option<KnownNetworkSelection> {
        let selected = self.known_networks_state.selected()?;
        let ethernet_offset = usize::from(self.is_ethernet_connected);

        if selected < ethernet_offset {
            return Some(KnownNetworkSelection::Ethernet);
        }

        let data_index = selected - ethernet_offset;
        if data_index < self.known_networks.len() {
            Some(KnownNetworkSelection::Network(data_index))
        } else {
            let unavail_index = data_index - self.known_networks.len();
            if unavail_index < self.unavailable_known_networks.len() {
                Some(KnownNetworkSelection::Unavailable(unavail_index))
            } else {
                None
            }
        }
    }

    /// Total number of rows in the known networks table (ethernet + known + unavailable).
    pub fn known_networks_total_rows(&self) -> usize {
        let ethernet_offset = usize::from(self.is_ethernet_connected);
        let unavail = if self.show_unavailable_known_networks {
            self.unavailable_known_networks.len()
        } else {
            0
        };
        ethernet_offset + self.known_networks.len() + unavail
    }

    /// True when an SSID filter is narrowing the New Networks list.
    pub fn new_filter_active(&self) -> bool {
        !self.filter_query.trim().is_empty()
    }

    /// Case-insensitive substring match used by the New Networks filter.
    fn new_filter_matches(&self, name: &str) -> bool {
        let query = self.filter_query.trim().to_lowercase();
        query.is_empty() || name.to_lowercase().contains(&query)
    }

    /// Indices into `new_networks` currently visible under the filter, in order.
    pub fn visible_new_indices(&self) -> Vec<usize> {
        self.new_networks
            .iter()
            .enumerate()
            .filter(|(_, (net, _))| self.new_filter_matches(&net.name))
            .map(|(i, _)| i)
            .collect()
    }

    /// Whether hidden-SSID rows are shown. They carry no SSID to match, so an
    /// active filter suppresses them.
    fn hidden_rows_visible(&self) -> bool {
        self.show_hidden_networks && !self.new_filter_active()
    }

    /// Total rows the New Networks table renders, for cursor clamping.
    pub fn new_networks_total_rows(&self) -> usize {
        let hidden = if self.hidden_rows_visible() {
            self.new_hidden_networks.len()
        } else {
            0
        };
        self.visible_new_indices().len() + hidden
    }

    /// Resolves the highlighted New Networks row to real data, mapping through
    /// the filter so the cursor and the action target never diverge.
    pub fn resolve_new_selection(&self) -> Option<NewNetworkSelection> {
        let selected = self.new_networks_state.selected()?;
        let visible = self.visible_new_indices();

        if selected < visible.len() {
            return Some(NewNetworkSelection::Visible(visible[selected]));
        }
        if !self.hidden_rows_visible() {
            return None;
        }
        let hidden_index = selected - visible.len();
        (hidden_index < self.new_hidden_networks.len())
            .then_some(NewNetworkSelection::Hidden(hidden_index))
    }

    /// Snaps the New Networks cursor to a valid row after the filter changes.
    fn reset_new_selection(&mut self) {
        if self.new_networks_total_rows() == 0 {
            self.new_networks_state.select(None);
        } else {
            self.new_networks_state.select(Some(0));
        }
    }

    /// Begins typing an SSID filter on the New Networks list.
    pub fn start_new_filter(&mut self) {
        self.filter_input = true;
    }

    /// Appends a character to the active filter and re-anchors the cursor.
    pub fn push_new_filter(&mut self, c: char) {
        self.filter_query.push(c);
        self.reset_new_selection();
    }

    /// Deletes the last filter character and re-anchors the cursor.
    pub fn pop_new_filter(&mut self) {
        self.filter_query.pop();
        self.reset_new_selection();
    }

    /// Stops editing but keeps the filter applied.
    pub fn commit_new_filter(&mut self) {
        self.filter_input = false;
    }

    /// Clears the filter entirely and exits editing.
    pub fn clear_new_filter(&mut self) {
        self.filter_query.clear();
        self.filter_input = false;
        self.reset_new_selection();
    }

    /// Border caption for the New Networks box while a filter is being typed or
    /// is applied; `None` otherwise. Includes the live match count.
    fn new_filter_caption(&self) -> Option<String> {
        if !self.filter_input && !self.new_filter_active() {
            return None;
        }
        let matches = self.visible_new_indices().len();
        if self.filter_input {
            Some(format!(" / {}▏ ({matches}) ", self.filter_query))
        } else {
            Some(format!(" filter: {} ({matches}) ", self.filter_query))
        }
    }

    /// Row for the adapter currently active on this Station. Carries the full
    /// set of runtime columns (state/scanning/frequency/security) plus an
    /// optional active marker for the multi-adapter case.
    fn active_device_row<'a>(&self, name: &str, device: &Device, marker: &str) -> Row<'a> {
        let frequency = self
            .diagnostic
            .as_ref()
            .and_then(|d| d.frequency)
            .map(|freq| format!("{:.2} GHz", freq as f32 / 1000.))
            .unwrap_or_else(|| "-".to_string());
        let security = self
            .diagnostic
            .as_ref()
            .and_then(|d| d.security.clone())
            .unwrap_or_else(|| "-".to_string());

        Row::new(vec![
            Line::from(name.to_string()).centered(),
            Line::from("station").centered(),
            Line::from(if device.is_powered { "On" } else { "Off" }).centered(),
            Line::from(self.state.to_string()).centered(),
            Line::from(if self.is_scanning { "Yes" } else { "No" }).centered(),
            Line::from(frequency).centered(),
            Line::from(security).centered(),
            Line::from(marker.to_string()).centered(),
        ])
    }

    pub fn render(
        &mut self,
        frame: &mut Frame,
        focused_block: FocusedBlock,
        device: &Device,
        config: Arc<Config>,
        view: &AdapterView,
        status: StationStatus,
    ) {
        let StationStatus {
            primary,
            connectivity,
        } = status;
        // Which link kind owns the default route, used to highlight its row.
        let primary_kind = primary.map(|p| p.kind);
        let (known_networks_block, new_networks_block, device_block, help_block) = {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Min(5),
                    Constraint::Min(5),
                    Constraint::Length(AdapterView::BLOCK_HEIGHT),
                    Constraint::Length(2),
                ])
                .margin(1)
                .split(frame.area());
            (chunks[0], chunks[1], chunks[2], chunks[3])
        };

        //
        // Device — one row per adapter. Inactive rows carry only identity so
        // the user can see and select them; station-specific columns are only
        // populated for the active adapter.
        //
        let rows = view.build_rows(
            |adapter, marker| self.active_device_row(&adapter.name, device, marker),
            |adapter| inactive_station_row(&adapter.name),
        );

        let widths = [
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Length(10),
            Constraint::Length(12),
            Constraint::Length(10),
            Constraint::Length(10),
            Constraint::Length(15),
            Constraint::Length(6),
        ];

        let device_table = Table::new(rows, widths)
            .header({
                let labels = [
                    "Name",
                    "Mode",
                    "Powered",
                    "State",
                    "Scanning",
                    "Frequency",
                    "Security",
                    "Active",
                ];
                if focused_block == FocusedBlock::Device {
                    Row::new(
                        labels
                            .iter()
                            .map(|l| Line::from(*l).yellow().centered())
                            .collect::<Vec<_>>(),
                    )
                    .style(Style::new().bold())
                    .bottom_margin(1)
                } else {
                    Row::new(
                        labels
                            .iter()
                            .map(|l| Line::from(*l).centered())
                            .collect::<Vec<_>>(),
                    )
                    .bottom_margin(1)
                }
            })
            .block(
                {
                    let block = Block::default().title(" Device ");
                    // Surface the adapter's LAN IPv4 (handy for SSH) without
                    // leaving the TUI.
                    match self.ipv4.as_deref() {
                        Some(ip) => block
                            .title_bottom(Line::from(format!(" 󰩟 {} · {ip} ", device.name)).cyan()),
                        None => block,
                    }
                }
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

        let mut device_state =
            TableState::default().with_selected(view.table_selection(focused_block));
        frame.render_stateful_widget(device_table, device_block, &mut device_state);

        //
        // Known networks
        //
        let mut rows: Vec<Row> = self
            .known_networks
            .iter()
            .map(|(net, signal)| {
                let known = net.known_network.as_ref().unwrap();
                let signal_percent = (*signal / 100).clamp(0, 100);
                let signal_str = format!("{}%", signal_percent);

                // WiFi can stay associated alongside an active Ethernet link, so
                // mark the connected network regardless of the wired state.
                if let Some(connected_net) = &self.connected_network
                    && connected_net.name == net.name
                {
                    let row = Row::new(vec![
                        Line::from("󰖩 ").centered(),
                        Line::from(known.name.clone()).centered(),
                        Line::from(known.network_type.to_string()).centered(),
                        Line::from(if known.is_hidden { "Yes" } else { "No" }).centered(),
                        Line::from(if known.is_autoconnect { "Yes" } else { "No" }).centered(),
                        Line::from(signal_str).centered(),
                    ]);

                    // Highlight the link NM is actually routing internet over.
                    return if primary_kind == Some(LinkKind::Wifi) {
                        row.green().bold()
                    } else {
                        row
                    };
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
            // Highlight Ethernet when it's the link carrying internet traffic.
            let ethernet_row = if primary_kind == Some(LinkKind::Ethernet) {
                ethernet_row.green().bold()
            } else {
                ethernet_row
            };
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
            // 32 cells fits the max-length 32-byte SSID without clipping the
            // centered name on both ends.
            Constraint::Length(32),
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
                {
                    let block = Block::default().title(" Known Networks ");
                    // Spell out which link is the live internet path so the green
                    // highlight isn't the only cue. Turn the caption amber when
                    // the link is up but not actually online (e.g. portal).
                    match internet_caption(primary, connectivity) {
                        Some(caption) => {
                            let line = Line::from(caption).bold();
                            let line = if connectivity == Connectivity::Full
                                || connectivity == Connectivity::Unknown
                            {
                                line.green()
                            } else {
                                line.yellow()
                            };
                            block.title_bottom(line)
                        }
                        None => block,
                    }
                }
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
            .visible_new_indices()
            .into_iter()
            .map(|idx| {
                let (net, signal) = &self.new_networks[idx];
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

        if self.hidden_rows_visible() {
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
            // 32 cells fits the max-length 32-byte SSID without clipping the
            // centered name on both ends.
            Constraint::Length(32),
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
                {
                    let block = Block::default().title(" New Networks ");
                    // Show the live filter on the border while typing or applied.
                    match self.new_filter_caption() {
                        Some(caption) => block.title_bottom(Line::from(caption).yellow().bold()),
                        None => block,
                    }
                }
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

        let mut help_message = match focused_block {
            FocusedBlock::Device => {
                let mut spans = vec![
                    Span::from(config.station.start_scanning.to_string()).bold(),
                    Span::from(" Scan"),
                    Span::from(" | "),
                    Span::from(config.device.infos.to_string()).bold(),
                    Span::from(" Infos"),
                    Span::from(" | "),
                    Span::from(config.device.toggle_power.to_string()).bold(),
                    Span::from(" Toggle Power"),
                    Span::from(" | "),
                    Span::from(config.device.doctor.to_string()).bold(),
                    Span::from(" Doctor"),
                    Span::from(" | "),
                    Span::from("ctrl+r").bold(),
                    Span::from(" Switch Mode"),
                    Span::from(" | "),
                    Span::from("⇄").bold(),
                    Span::from(" Nav"),
                ];
                if view.is_multi() {
                    spans.extend(adapter_nav_spans());
                }
                vec![Line::from(spans)]
            }
            FocusedBlock::KnownNetworks => {
                let single_line = Line::from(vec![
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
                    Span::from(config.station.known_network.prefer.to_string()).bold(),
                    Span::from(" Internet"),
                    Span::from(" | "),
                    Span::from(config.station.known_network.portal.to_string()).bold(),
                    Span::from(" Portal"),
                    Span::from(" | "),
                    Span::from("ctrl+r").bold(),
                    Span::from(" Switch Mode"),
                    Span::from(" | "),
                    Span::from("⇄").bold(),
                    Span::from(" Nav"),
                ]);
                // The VPN hint is appended to the last line below, so account
                // for it when deciding if the one-line layout fits.
                let vpn_suffix_width =
                    Line::from(crate::device::vpn_hint_spans(config.vpn)).width() as u16;
                let one_line_fits = single_line.width() as u16 + vpn_suffix_width
                    <= help_block.width;

                if one_line_fits {
                    vec![single_line]
                } else {
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
                            Span::from(" | "),
                            Span::from(config.station.known_network.prefer.to_string()).bold(),
                            Span::from(" Internet"),
                            Span::from(" | "),
                            Span::from(config.station.known_network.portal.to_string()).bold(),
                            Span::from(" Portal"),
                        ]),
                    ]
                }
            }
            FocusedBlock::NewNetworks => {
                let single_line = Line::from(vec![
                    Span::from("k,").bold(),
                    Span::from("  Up"),
                    Span::from(" | "),
                    Span::from("j,").bold(),
                    Span::from("  Down"),
                    Span::from(" | "),
                    Span::from("󱁐  or ↵ ").bold(),
                    Span::from(" Connect"),
                    Span::from(" | "),
                    Span::from(config.station.new_network.connect_hidden.to_string()).bold(),
                    Span::from(" Hidden"),
                    Span::from(" | "),
                    Span::from(config.station.new_network.show_all.to_string()).bold(),
                    Span::from(" Show All"),
                    Span::from(" | "),
                    Span::from(config.station.new_network.filter.to_string()).bold(),
                    Span::from(" Filter"),
                    Span::from(" | "),
                    Span::from(config.station.start_scanning.to_string()).bold(),
                    Span::from(" Scan"),
                    Span::from(" | "),
                    Span::from("ctrl+r").bold(),
                    Span::from(" Switch Mode"),
                    Span::from(" | "),
                    Span::from("⇄").bold(),
                    Span::from(" Nav"),
                ]);
                let vpn_suffix_width =
                    Line::from(crate::device::vpn_hint_spans(config.vpn)).width() as u16;
                let one_line_fits = single_line.width() as u16 + vpn_suffix_width
                    <= help_block.width;

                if one_line_fits {
                    vec![single_line]
                } else {
                    vec![
                        Line::from(vec![
                            Span::from("󱁐  or ↵ ").bold(),
                            Span::from(" Connect"),
                            Span::from(" | "),
                            Span::from(config.station.new_network.connect_hidden.to_string())
                                .bold(),
                            Span::from(" Hidden"),
                            Span::from(" | "),
                            Span::from(config.station.new_network.filter.to_string()).bold(),
                            Span::from(" Filter"),
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

        // Advertise the global VPN shortcut from every list view by appending
        // it to the last help row.
        if matches!(
            focused_block,
            FocusedBlock::Device | FocusedBlock::KnownNetworks | FocusedBlock::NewNetworks
        ) && let Some(last) = help_message.last_mut()
        {
            last.spans.extend(crate::device::vpn_hint_spans(config.vpn));
        }

        let help_message = Paragraph::new(help_message).centered().blue();

        frame.render_widget(help_message, help_block);

        // Share
        if let Some(share) = &self.share {
            share.render(frame);
        }

        // Speed test
        if let Some(speed_test) = &self.speed_test {
            speed_test.render(frame);
        }
    }
}
