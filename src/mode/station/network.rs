use anyhow::Result;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

use crate::nm::{
    AccessPointInfo, ActivationFailureReason, ActivationOutcome, NMClient, SecurityType,
};

use crate::{
    event::Event,
    mode::station::known_network::KnownNetwork,
    notification::{Notification, NotificationLevel},
};

/// Drive a freshly-started WiFi activation to its terminal state and surface
/// the result to the user.
///
/// Call sites are responsible for *starting* the activation (via
/// `add_and_activate_connection`, `activate_connection`, or
/// `add_and_activate_hidden_connection`) and handing this helper the resulting
/// active-connection path. The helper then:
///   1. emits an `"Associating with …"` info notification,
///   2. waits on the NM state machine,
///   3. emits exactly one terminal notification — success, bad-password, or
///      generic failure.
///
/// Notification send-errors are intentionally swallowed: by the time we have
/// an activation outcome the caller has typically been spawned onto a
/// background task with no error path back to the UI thread.
pub async fn watch_activation(
    client: Arc<NMClient>,
    active_path: String,
    device_path: String,
    ssid: String,
    sender: UnboundedSender<Event>,
) {
    let _ = Notification::send(
        format!("Associating with {}…", ssid),
        NotificationLevel::Info,
        &sender,
    );

    let outcome = match client.await_activation(&active_path, &device_path).await {
        Ok(outcome) => outcome,
        Err(e) => {
            let _ = Notification::send(
                format!("Failed to monitor connection to {}: {}", ssid, e),
                NotificationLevel::Error,
                &sender,
            );
            return;
        }
    };

    let (message, level) = match outcome {
        ActivationOutcome::Activated => (format!("Connected to {}", ssid), NotificationLevel::Info),
        ActivationOutcome::Failed(ActivationFailureReason::BadSecrets) => (
            format!("Wrong password for {}", ssid),
            NotificationLevel::Error,
        ),
        ActivationOutcome::Failed(ActivationFailureReason::SsidNotFound) => (
            format!(
                "Could not connect to {} — wrong password or network out of range",
                ssid
            ),
            NotificationLevel::Error,
        ),
        ActivationOutcome::Failed(ActivationFailureReason::Timeout) => (
            format!("Connection to {} timed out", ssid),
            NotificationLevel::Error,
        ),
        ActivationOutcome::Failed(ActivationFailureReason::Other(code)) => (
            format!("Failed to connect to {} (reason {})", ssid, code),
            NotificationLevel::Error,
        ),
    };

    let _ = Notification::send(message, level, &sender);
}

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
        // Kick off the activation. Each branch produces either an active
        // connection path (success) or a D-Bus-level error (start failure).
        let start_result = if let Some(known) = &self.known_network {
            self.client
                .activate_connection(&known.connection_path, &self.device_path)
                .await
        } else {
            self.client
                .add_and_activate_connection(&self.device_path, &self.ap_path, password)
                .await
        };

        match start_result {
            Ok(active_path) => {
                tokio::spawn(watch_activation(
                    self.client.clone(),
                    active_path.to_string(),
                    self.device_path.clone(),
                    self.name.clone(),
                    sender,
                ));
                Ok(())
            }
            Err(e) => {
                // A missing password on a secured new-network attempt is not a
                // user-visible failure — it's the trigger for the auth flow.
                if self.known_network.is_none()
                    && self.network_type.requires_password()
                    && password.is_none()
                {
                    return Err(anyhow::anyhow!("Password required"));
                }
                Notification::send(
                    format!("Failed to connect: {}", e),
                    NotificationLevel::Error,
                    &sender,
                )?;
                Ok(())
            }
        }
    }

    pub fn requires_password(&self) -> bool {
        self.known_network.is_none() && self.network_type.requires_password()
    }

    pub fn is_enterprise(&self) -> bool {
        self.network_type.is_enterprise()
    }
}
