use anyhow::{Result, anyhow};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

use crate::nm::{Mode, NMClient};

use crate::{
    adapter::Adapter, agent::AuthAgent, config::Config, device::Device, event::Event,
    mode::station::auth::Auth, mode::station::network::Network, notification::Notification,
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

        Ok(())
    }

    pub fn quit(&mut self) {
        self.running = false;
    }
}
