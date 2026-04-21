use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

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

pub struct App {
    pub running: bool,
    pub focused_block: FocusedBlock,
    pub notifications: Vec<Notification>,
    pub client: Arc<NMClient>,
    pub adapter: Adapter,
    pub device: Device,
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

        let mut device = Device::new(client.clone()).await?;

        let adapter =
            match Adapter::new(client.clone(), device.device_path.clone(), config.clone()).await {
                Ok(v) => v,
                Err(e) => {
                    return Err(anyhow!("Can not access the NetworkManager service: {}", e));
                }
            };

        // Set the initial mode
        device.set_mode(mode).await?;

        let agent = AuthAgent::new(sender);
        // Note: NetworkManager handles authentication differently than iwd
        // Secrets are managed via NetworkManager's SecretAgent interface
        // For now, we'll handle password prompts through the existing agent mechanism

        let focused_block = if device.is_powered {
            match device.mode {
                Mode::Station => FocusedBlock::KnownNetworks,
                Mode::Ap => FocusedBlock::AccessPoint,
            }
        } else {
            FocusedBlock::Device
        };

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
            config,
            auth: Auth::default(),
            network_name_requiring_auth: None,
            network_pending_auth: None,
            doctor: None,
            doctor_run_id: 0,
            portal_watcher: PortalWatcher::new(),
        })
    }

    pub async fn reset(mode: Mode) -> Result<()> {
        let client = {
            match NMClient::new().await {
                Ok(client) => Arc::new(client),
                Err(e) => return Err(anyhow!("Can not access the NetworkManager service: {}", e)),
            }
        };

        let mut device = match Device::new(client.clone()).await {
            Ok(v) => v,
            Err(e) => return Err(anyhow!("Can not access the NetworkManager service: {}", e)),
        };

        device.set_mode(mode).await?;
        Ok(())
    }

    pub async fn tick(&mut self) -> Result<()> {
        self.notifications.retain(|n| n.ttl > 0);
        self.notifications.iter_mut().for_each(|n| n.ttl -= 1);

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
