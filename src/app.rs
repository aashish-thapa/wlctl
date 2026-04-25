use anyhow::{Result, anyhow};
use ratatui::widgets::Row;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;
use zbus::zvariant::OwnedObjectPath;

use crate::nm::{Mode, NMClient};

use crate::{
    adapter::Adapter,
    agent::AuthAgent,
    config::Config,
    device::Device,
    doctor::DoctorModal,
    event::Event,
    mode::station::auth::Auth,
    mode::station::network::Network,
    notification::{Notification, NotificationLevel},
    portal::{self, PortalWatcher},
    reset::Reset,
};

/// Marker glyph rendered in the Active column for the currently active adapter.
const ACTIVE_MARKER: &str = "●";

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FocusedBlock {
    Device,
    AccessPoint,
    KnownNetworks,
    NewNetworks,
    PskAuthKey,
    WpaEntrepriseAuth,
    AdapterInfos,
    AccessPointInput,
    AccessPointConnectedDevices,
    RequestKeyPasshphrase,
    RequestPassword,
    RequestUsernameAndPassword,
    ShareNetwork,
    SpeedTest,
    HiddenSsidInput,
    Doctor,
}

/// A lightweight handle to a WiFi adapter known to NetworkManager.
///
/// Holding the path + human name avoids re-fetching the interface name every
/// render and lets the Device block list adapters without instantiating a full
/// `Device` per row.
#[derive(Debug, Clone)]
pub struct AdapterSummary {
    pub path: OwnedObjectPath,
    pub name: String,
}

impl AdapterSummary {
    async fn fetch(client: &NMClient, path: OwnedObjectPath) -> Result<Self> {
        let name = client.get_device_interface(path.as_str()).await?;
        Ok(Self { path, name })
    }

    async fn fetch_all(client: &NMClient) -> Result<Vec<Self>> {
        let paths = client.get_wifi_devices().await?;
        let mut out = Vec::with_capacity(paths.len());
        for p in paths {
            out.push(Self::fetch(client, p).await?);
        }
        Ok(out)
    }
}

/// Snapshot of adapter state passed to render paths. Bundles the list plus
/// both cursors so Device / Station / AP views can render a consistent
/// multi-row adapter list without each re-computing selection logic.
pub struct AdapterView<'a> {
    pub adapters: &'a [AdapterSummary],
    pub active_index: usize,
    pub selection_index: usize,
}

impl<'a> AdapterView<'a> {
    /// Number of adapter rows visible in the Device block before the table
    /// starts scrolling. Keeping this fixed means the layout footprint does
    /// not shift when adapters are plugged in or out.
    pub const VISIBLE_ROWS: u16 = 2;

    /// Non-data rows the Device block reserves: top border + bottom border
    /// + header row + header bottom margin.
    const CHROME_ROWS: u16 = 4;

    /// Total height callers should use for the Device block. Constant so
    /// `TableState` can scroll the viewport for 3+ adapters without the
    /// surrounding layout shifting.
    pub const BLOCK_HEIGHT: u16 = Self::VISIBLE_ROWS + Self::CHROME_ROWS;

    pub fn count(&self) -> usize {
        self.adapters.len()
    }

    pub fn is_multi(&self) -> bool {
        self.adapters.len() > 1
    }

    /// Index the Table's `TableState` should highlight. Selection cursor is
    /// only meaningful while the Device block has focus and more than one
    /// adapter exists; otherwise snap to the active row.
    pub fn table_selection(&self, focused: FocusedBlock) -> usize {
        if focused == FocusedBlock::Device && self.is_multi() {
            self.selection_index
        } else {
            self.active_index
        }
    }

    pub fn is_active(&self, index: usize) -> bool {
        index == self.active_index
    }

    /// Builds one Row per adapter. `active` receives the marker string and
    /// `inactive` does not — this keeps the "●" glyph in one place and lets
    /// each render path supply its own mode-specific column set.
    pub fn build_rows<'r, F, G>(&self, active: F, inactive: G) -> Vec<Row<'r>>
    where
        F: Fn(&AdapterSummary, &str) -> Row<'r>,
        G: Fn(&AdapterSummary) -> Row<'r>,
    {
        if !self.is_multi() {
            let only = &self.adapters[self.active_index];
            return vec![active(only, ACTIVE_MARKER)];
        }

        self.adapters
            .iter()
            .enumerate()
            .map(|(idx, a)| {
                if self.is_active(idx) {
                    active(a, ACTIVE_MARKER)
                } else {
                    inactive(a)
                }
            })
            .collect()
    }
}

pub struct App {
    pub running: bool,
    pub focused_block: FocusedBlock,
    pub notifications: Vec<Notification>,
    pub client: Arc<NMClient>,
    pub adapter: Adapter,
    pub device: Device,
    pub adapters: Vec<AdapterSummary>,
    pub active_index: usize,
    pub adapter_selection_index: usize,
    pub agent: AuthAgent,
    pub reset: Reset,
    pub config: Arc<Config>,
    pub auth: Auth,
    pub network_name_requiring_auth: Option<String>,
    pub network_pending_auth: Option<Network>,
    pub doctor: Option<DoctorModal>,
    /// Monotonic run id. Each `start_doctor` bumps this; the event handler only
    /// applies results that carry the current id. Dismissing also bumps it, so
    /// in-flight results never resurrect a closed modal.
    pub doctor_run_id: u64,
    /// Captive portal transition tracker. Idle field that only acts during
    /// `tick()` when the current SSID transitions into a portal state.
    pub portal_watcher: PortalWatcher,
}

impl App {
    pub async fn new(
        sender: UnboundedSender<Event>,
        config: Arc<Config>,
        mode: Mode,
    ) -> Result<Self> {
        let client = {
            match NMClient::new().await {
                Ok(client) => Arc::new(client),
                Err(e) => {
                    return Err(anyhow!(
                        "Can not access the NetworkManager service.
Error: {}",
                        e
                    ));
                }
            }
        };

        let adapters = AdapterSummary::fetch_all(&client).await?;
        if adapters.is_empty() {
            return Err(anyhow!("No WiFi device found"));
        }

        let active_index = 0;
        let mut device = Device::new(client.clone(), adapters[active_index].path.clone()).await?;

        let adapter =
            match Adapter::new(client.clone(), device.device_path.clone(), config.clone()).await {
                Ok(v) => v,
                Err(e) => {
                    return Err(anyhow!("Can not access the NetworkManager service: {}", e));
                }
            };

        device.set_mode(mode).await?;

        let agent = AuthAgent::new(sender);

        let focused_block = Self::default_focus_for(&device);

        let reset = Reset::new(mode);

        Ok(Self {
            running: true,
            focused_block,
            notifications: Vec::new(),
            client,
            adapter,
            agent,
            reset,
            device,
            adapters,
            active_index,
            adapter_selection_index: active_index,
            config,
            auth: Auth::default(),
            network_name_requiring_auth: None,
            network_pending_auth: None,
            doctor: None,
            doctor_run_id: 0,
            portal_watcher: PortalWatcher::new(),
        })
    }

    pub fn adapter_count(&self) -> usize {
        self.adapters.len()
    }

    /// Moves the visual adapter selection without touching the active adapter.
    /// Wraps around and is a no-op with <=1 adapter so callers don't need to guard.
    pub fn move_adapter_selection(&mut self, delta: isize) {
        let len = self.adapters.len();
        if len <= 1 {
            return;
        }
        self.adapter_selection_index =
            (self.adapter_selection_index as isize + delta).rem_euclid(len as isize) as usize;
    }

    /// Activates the adapter currently highlighted by the selection cursor.
    /// No-op when the selection already points at the active adapter.
    pub async fn activate_selected_adapter(&mut self) -> Result<()> {
        if self.adapter_selection_index == self.active_index {
            return Ok(());
        }

        let target = self
            .adapters
            .get(self.adapter_selection_index)
            .ok_or_else(|| anyhow!("Selected adapter out of range"))?
            .path
            .clone();
        let previous_mode = self.device.mode;

        self.activate_device(target, previous_mode).await?;
        self.active_index = self.adapter_selection_index;
        Ok(())
    }

    fn default_focus_for(device: &Device) -> FocusedBlock {
        if device.is_powered {
            match device.mode {
                Mode::Station => FocusedBlock::KnownNetworks,
                Mode::Ap => FocusedBlock::AccessPoint,
            }
        } else {
            FocusedBlock::Device
        }
    }

    // Builds a Device + Adapter for the given path and only commits them to `self`
    // after every await has succeeded. Callers update `active_index`/`adapters`
    // after this returns Ok, so `self` stays consistent on failure.
    async fn activate_device(&mut self, path: OwnedObjectPath, preserve_mode: Mode) -> Result<()> {
        let mut new_device = Device::new(self.client.clone(), path).await?;
        new_device.set_mode(preserve_mode).await?;
        let new_adapter = Adapter::new(
            self.client.clone(),
            new_device.device_path.clone(),
            self.config.clone(),
        )
        .await?;

        self.device = new_device;
        self.adapter = new_adapter;
        self.focused_block = Self::default_focus_for(&self.device);

        Ok(())
    }

    pub async fn reset(mode: Mode) -> Result<()> {
        let client = {
            match NMClient::new().await {
                Ok(client) => Arc::new(client),
                Err(e) => return Err(anyhow!("Can not access the NetworkManager service: {}", e)),
            }
        };

        let device_paths = client.get_wifi_devices().await?;
        let path = device_paths
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("No WiFi device found"))?;

        let mut device = match Device::new(client.clone(), path).await {
            Ok(v) => v,
            Err(e) => return Err(anyhow!("Can not access the NetworkManager service: {}", e)),
        };

        device.set_mode(mode).await?;
        Ok(())
    }

    pub async fn tick(&mut self) -> Result<()> {
        self.notifications.retain(|n| n.ttl > 0);
        self.notifications.iter_mut().for_each(|n| n.ttl -= 1);

        // Refresh the adapter list; if the active path disappeared, fall back to
        // the first remaining device. `adapters` is only committed after any
        // fallible activation so `self` stays consistent on error.
        let current = AdapterSummary::fetch_all(&self.client).await?;
        let paths_changed = current.len() != self.adapters.len()
            || current
                .iter()
                .zip(&self.adapters)
                .any(|(a, b)| a.path != b.path);

        if paths_changed {
            // Remember which path the user was browsing so we can keep the
            // cursor on it across reorders / insertions when it still exists.
            let prev_selection_path = self
                .adapters
                .get(self.adapter_selection_index)
                .map(|a| a.path.clone());

            let active_path = self.device.device_path.clone();
            match current.iter().position(|a| a.path.as_str() == active_path) {
                Some(idx) => {
                    self.active_index = idx;
                    self.adapters = current;
                }
                None => {
                    if current.is_empty() {
                        return Err(anyhow!("No WiFi device found"));
                    }
                    let previous_mode = self.device.mode;
                    let fallback = current[0].path.clone();
                    self.activate_device(fallback, previous_mode).await?;
                    self.active_index = 0;
                    self.adapters = current;
                }
            }

            self.adapter_selection_index = prev_selection_path
                .and_then(|p| self.adapters.iter().position(|a| a.path == p))
                .unwrap_or(self.active_index);
        }

        self.device.refresh().await?;
        self.adapter.refresh().await?;

        self.check_captive_portal().await;

        Ok(())
    }

    async fn check_captive_portal(&mut self) {
        if !self.config.captive_portal.auto_open {
            return;
        }

        let detected = match self
            .portal_watcher
            .poll(&self.client, &self.device.device_path)
            .await
        {
            Ok(Some(d)) => d,
            Ok(None) => return,
            Err(_) => return,
        };

        self.notifications.push(Notification {
            message: format!(
                "Captive portal detected on '{}'. Opening browser...",
                detected.ssid
            ),
            level: NotificationLevel::Info,
            ttl: 5,
        });

        if let Err(e) = portal::launch_browser(&detected.url).await {
            self.notifications.push(Notification {
                message: format!("Could not launch browser: {}", e),
                level: NotificationLevel::Error,
                ttl: 5,
            });
        }
    }

    pub fn quit(&mut self) {
        self.running = false;
    }
}
