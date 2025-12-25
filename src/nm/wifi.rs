// WiFi-specific helpers for NetworkManager

use super::{AccessPointInfo, NMClient};
use anyhow::Result;

impl NMClient {
    /// Get all visible networks, deduplicated by SSID
    pub async fn get_visible_networks(&self, device_path: &str) -> Result<Vec<AccessPointInfo>> {
        let aps = self.get_access_points(device_path).await?;
        let mut networks: Vec<AccessPointInfo> = Vec::new();

        for ap_path in aps {
            if let Ok(ap_info) = self.get_access_point_info(ap_path.as_str()).await {
                // Skip empty SSIDs (hidden networks show up with empty SSID)
                if ap_info.ssid.is_empty() {
                    continue;
                }

                // Deduplicate by SSID, keeping the one with strongest signal
                if let Some(existing) = networks.iter_mut().find(|n| n.ssid == ap_info.ssid) {
                    if ap_info.strength > existing.strength {
                        *existing = ap_info;
                    }
                } else {
                    networks.push(ap_info);
                }
            }
        }

        // Sort by signal strength (strongest first)
        networks.sort_by(|a, b| b.strength.cmp(&a.strength));

        Ok(networks)
    }

    /// Find a saved connection for an SSID
    pub async fn find_connection_for_ssid(&self, ssid: &str) -> Result<Option<String>> {
        let connections = self.get_wifi_connections().await?;
        Ok(connections
            .into_iter()
            .find(|c| c.ssid == ssid)
            .map(|c| c.path))
    }

    /// Check if currently connected to any network
    pub async fn is_connected(&self, device_path: &str) -> Result<bool> {
        let state = self.get_device_state(device_path).await?;
        Ok(state == super::DeviceState::Activated)
    }

    /// Get the currently connected network name
    pub async fn get_connected_ssid(&self, device_path: &str) -> Result<Option<String>> {
        if let Some(ap_path) = self.get_active_access_point(device_path).await? {
            let ap_info = self.get_access_point_info(ap_path.as_str()).await?;
            Ok(Some(ap_info.ssid))
        } else {
            Ok(None)
        }
    }
}
