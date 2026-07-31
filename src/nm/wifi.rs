// WiFi-specific helpers for NetworkManager

use super::{AccessPointInfo, NMClient, NmSnapshot};
use anyhow::Result;

impl NMClient {
    /// Get all visible networks, deduplicated by SSID.
    ///
    /// Prefer [`NmSnapshot::visible_networks`] when the caller already holds a
    /// snapshot; this reads a fresh one.
    pub async fn get_visible_networks(&self, device_path: &str) -> Result<Vec<AccessPointInfo>> {
        let snapshot = NmSnapshot::fetch(self).await?;
        Ok(snapshot.visible_networks(device_path))
    }

    /// Find a saved connection for an SSID
    pub async fn find_connection_for_ssid(&self, ssid: &str) -> Result<Option<String>> {
        let connections = self.get_wifi_connections().await?;
        Ok(connections
            .iter()
            .find(|c| c.ssid == ssid)
            .map(|c| c.path.clone()))
    }

    /// Check if currently connected to any network
    pub async fn is_connected(&self, device_path: &str) -> Result<bool> {
        let state = self.get_device_state(device_path).await?;
        Ok(state == super::DeviceState::Activated)
    }
}
