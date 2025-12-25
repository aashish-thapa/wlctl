use anyhow::Result;
use chrono::{DateTime, FixedOffset, TimeZone};
use std::sync::Arc;

use crate::nm::{ConnectionInfo, NMClient, SecurityType};

use tokio::sync::mpsc::UnboundedSender;

use crate::{
    event::Event,
    notification::{Notification, NotificationLevel},
};

#[derive(Debug, Clone)]
pub struct KnownNetwork {
    pub client: Arc<NMClient>,
    pub connection_path: String,
    pub name: String,
    pub network_type: SecurityType,
    pub is_autoconnect: bool,
    pub is_hidden: bool,
    pub last_connected: Option<DateTime<FixedOffset>>,
}

impl KnownNetwork {
    pub fn from_connection_info(client: Arc<NMClient>, info: ConnectionInfo) -> Self {
        // Convert unix timestamp to DateTime
        let last_connected = if info.timestamp > 0 {
            FixedOffset::east_opt(0)
                .and_then(|offset| offset.timestamp_opt(info.timestamp as i64, 0).single())
        } else {
            None
        };

        Self {
            client,
            connection_path: info.path,
            name: info.ssid,
            network_type: info.security,
            is_autoconnect: info.autoconnect,
            is_hidden: info.hidden,
            last_connected,
        }
    }

    pub async fn forget(&self, sender: UnboundedSender<Event>) -> Result<()> {
        match self.client.delete_connection(&self.connection_path).await {
            Ok(()) => {
                let _ = Notification::send(
                    format!("The Network {} is removed", self.name),
                    NotificationLevel::Info,
                    &sender,
                );
            }
            Err(e) => {
                let _ =
                    Notification::send(e.to_string(), NotificationLevel::Error, &sender.clone());
            }
        }

        Ok(())
    }

    pub async fn toggle_autoconnect(&mut self, sender: UnboundedSender<Event>) -> Result<()> {
        let new_autoconnect = !self.is_autoconnect;

        match self
            .client
            .set_connection_autoconnect(&self.connection_path, new_autoconnect)
            .await
        {
            Ok(()) => {
                self.is_autoconnect = new_autoconnect;
                let msg = if new_autoconnect {
                    format!("Enable Autoconnect for: {}", self.name)
                } else {
                    format!("Disable Autoconnect for: {}", self.name)
                };
                Notification::send(msg, NotificationLevel::Info, &sender.clone())?;
            }
            Err(e) => {
                Notification::send(e.to_string(), NotificationLevel::Error, &sender.clone())?;
            }
        }
        Ok(())
    }
}
