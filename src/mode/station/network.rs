use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

use crate::nm::{AccessPointInfo, NMClient, SecurityType};

use crate::{
    event::Event,
    mode::station::known_network::KnownNetwork,
    notification::{Notification, NotificationLevel},
};

#[derive(Debug, Clone)]
pub struct Network {
    pub client: Arc<NMClient>,
    pub device_path: String,
    pub ap_path: String,
    pub name: String,
    pub network_type: SecurityType,
    pub is_connected: bool,
    pub known_network: Option<KnownNetwork>,
    pub signal_strength: u8,
}

impl Network {
    pub fn from_access_point(
        client: Arc<NMClient>,
        device_path: String,
        ap_info: AccessPointInfo,
        known_network: Option<KnownNetwork>,
        is_connected: bool,
    ) -> Self {
        Self {
            client,
            device_path,
            ap_path: ap_info.path,
            name: ap_info.ssid,
            network_type: ap_info.security,
            is_connected,
            known_network,
            signal_strength: ap_info.strength,
        }
    }

    pub async fn connect(
        &self,
        sender: UnboundedSender<Event>,
        password: Option<&str>,
    ) -> Result<()> {
        // Check if we have a saved connection for this network
        if let Some(known) = &self.known_network {
            // Use existing connection profile
            match self
                .client
                .activate_connection(&known.connection_path, &self.device_path)
                .await
            {
                Ok(_) => {
                    Notification::send(
                        format!("Connecting to {}", self.name),
                        NotificationLevel::Info,
                        &sender,
                    )?;
                }
                Err(e) => {
                    Notification::send(
                        format!("Failed to connect: {}", e),
                        NotificationLevel::Error,
                        &sender,
                    )?;
                }
            }
        } else {
            // Create new connection
            match self
                .client
                .add_and_activate_connection(&self.device_path, &self.ap_path, password)
                .await
            {
                Ok(_) => {
                    Notification::send(
                        format!("Connecting to {}", self.name),
                        NotificationLevel::Info,
                        &sender,
                    )?;
                }
                Err(e) => {
                    // Check if password is required
                    if self.network_type.requires_password() && password.is_none() {
                        // Password required - this will be handled by the auth flow
                        return Err(anyhow::anyhow!("Password required"));
                    }
                    Notification::send(
                        format!("Failed to connect: {}", e),
                        NotificationLevel::Error,
                        &sender,
                    )?;
                }
            }
        }
        Ok(())
    }

    pub fn requires_password(&self) -> bool {
        self.known_network.is_none() && self.network_type.requires_password()
    }

    pub fn is_enterprise(&self) -> bool {
        self.network_type.is_enterprise()
    }
}
