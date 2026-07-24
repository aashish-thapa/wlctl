// WiFi-specific helpers for NetworkManager

use super::{AccessPointInfo, ManagedObjects, NMClient};
use anyhow::Result;
use zbus::zvariant::OwnedObjectPath;

impl NMClient {
    /// Get all visible networks, deduplicated by SSID
    pub async fn get_visible_networks(&self, device_path: &str) -> Result<Vec<AccessPointInfo>> {
        let managed = self.get_managed_objects().await?;
        self.get_visible_networks_from_managed(device_path, &managed)
            .await
    }

    /// Get all visible networks using an existing ObjectManager snapshot.
    pub async fn get_visible_networks_from_managed(
        &self,
        device_path: &str,
        managed: &ManagedObjects,
    ) -> Result<Vec<AccessPointInfo>> {
        const AP_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
        let aps = self.get_access_points(device_path).await?;
        let mut networks: Vec<AccessPointInfo> = Vec::new();

        for ap_path in aps {
            let Some(props) = managed
                .get(&ap_path)
                .and_then(|ifaces| ifaces.get(AP_IFACE))
            else {
                continue;
            };
            let ap_info = NMClient::access_point_info_from_props(ap_path.as_str(), props);

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

        // Sort by signal strength (strongest first)
        networks.sort_by_key(|n| std::cmp::Reverse(n.strength));

        Ok(networks)
    }

    /// Read one access point from an existing ObjectManager snapshot.
    pub fn get_access_point_info_from_managed(
        &self,
        ap_path: &OwnedObjectPath,
        managed: &ManagedObjects,
    ) -> Option<AccessPointInfo> {
        const AP_IFACE: &str = "org.freedesktop.NetworkManager.AccessPoint";
        let props = managed.get(ap_path)?.get(AP_IFACE)?;
        Some(NMClient::access_point_info_from_props(
            ap_path.as_str(),
            props,
        ))
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
