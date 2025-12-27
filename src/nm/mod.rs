// NetworkManager D-Bus abstraction layer
// Replaces iwdrs with direct NetworkManager D-Bus calls

use anyhow::{Context, Result};
use std::collections::HashMap;
use zbus::zvariant::{ObjectPath, OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy};

pub mod dbus_interfaces;
pub mod types;
pub mod wifi;

pub use types::*;

const NM_BUS_NAME: &str = "org.freedesktop.NetworkManager";
const NM_PATH: &str = "/org/freedesktop/NetworkManager";

/// Main NetworkManager client
#[derive(Clone, Debug)]
pub struct NMClient {
    connection: Connection,
}

impl NMClient {
    /// Create a new NetworkManager client
    pub async fn new() -> Result<Self> {
        let connection = Connection::system()
            .await
            .context("Failed to connect to system D-Bus")?;

        // Verify NetworkManager is running
        let proxy = Proxy::new(
            &connection,
            NM_BUS_NAME,
            NM_PATH,
            "org.freedesktop.NetworkManager",
        )
        .await?;

        // Try to get version to verify NM is accessible
        let _version: String = proxy.get_property("Version").await.context(
            "NetworkManager is not running or not accessible. Please ensure NetworkManager service is active.",
        )?;

        Ok(Self { connection })
    }

    /// Get the D-Bus connection
    pub fn connection(&self) -> &Connection {
        &self.connection
    }

    /// Get all WiFi devices
    pub async fn get_wifi_devices(&self) -> Result<Vec<OwnedObjectPath>> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_PATH,
            "org.freedesktop.NetworkManager",
        )
        .await?;

        let devices: Vec<OwnedObjectPath> = proxy.call("GetDevices", &()).await?;

        let mut wifi_devices = Vec::new();
        for device_path in devices {
            let device_proxy = Proxy::new(
                &self.connection,
                NM_BUS_NAME,
                device_path.as_str(),
                "org.freedesktop.NetworkManager.Device",
            )
            .await?;

            // DeviceType 2 = WiFi
            let device_type: u32 = device_proxy.get_property("DeviceType").await?;
            if device_type == 2 {
                wifi_devices.push(device_path);
            }
        }

        Ok(wifi_devices)
    }

    /// Get the first WiFi device
    pub async fn get_wifi_device(&self) -> Result<OwnedObjectPath> {
        let devices = self.get_wifi_devices().await?;
        devices.into_iter().next().context("No WiFi device found")
    }

    /// Get device interface name
    pub async fn get_device_interface(&self, device_path: &str) -> Result<String> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            device_path,
            "org.freedesktop.NetworkManager.Device",
        )
        .await?;

        Ok(proxy.get_property("Interface").await?)
    }

    /// Get device hardware address
    pub async fn get_device_hw_address(&self, device_path: &str) -> Result<String> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            device_path,
            "org.freedesktop.NetworkManager.Device",
        )
        .await?;

        Ok(proxy.get_property("HwAddress").await?)
    }

    /// Check if device is powered/enabled
    pub async fn is_wireless_enabled(&self) -> Result<bool> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_PATH,
            "org.freedesktop.NetworkManager",
        )
        .await?;

        Ok(proxy.get_property("WirelessEnabled").await?)
    }

    /// Enable/disable wireless
    pub async fn set_wireless_enabled(&self, enabled: bool) -> Result<()> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_PATH,
            "org.freedesktop.NetworkManager",
        )
        .await?;

        proxy.set_property("WirelessEnabled", enabled).await?;
        Ok(())
    }

    /// Get device state
    pub async fn get_device_state(&self, device_path: &str) -> Result<DeviceState> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            device_path,
            "org.freedesktop.NetworkManager.Device",
        )
        .await?;

        let state: u32 = proxy.get_property("State").await?;
        Ok(DeviceState::from(state))
    }

    /// Request a WiFi scan on a device
    pub async fn request_scan(&self, device_path: &str) -> Result<()> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            device_path,
            "org.freedesktop.NetworkManager.Device.Wireless",
        )
        .await?;

        // Empty options map for scan
        let options: HashMap<&str, Value> = HashMap::new();
        let _: () = proxy.call("RequestScan", &(options,)).await?;
        Ok(())
    }

    /// Get all access points (scanned networks)
    pub async fn get_access_points(&self, device_path: &str) -> Result<Vec<OwnedObjectPath>> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            device_path,
            "org.freedesktop.NetworkManager.Device.Wireless",
        )
        .await?;

        Ok(proxy.call("GetAllAccessPoints", &()).await?)
    }

    /// Get current active access point
    pub async fn get_active_access_point(
        &self,
        device_path: &str,
    ) -> Result<Option<OwnedObjectPath>> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            device_path,
            "org.freedesktop.NetworkManager.Device.Wireless",
        )
        .await?;

        let ap_path: OwnedObjectPath = proxy.get_property("ActiveAccessPoint").await?;
        if ap_path.as_str() == "/" {
            Ok(None)
        } else {
            Ok(Some(ap_path))
        }
    }

    /// Get access point details
    pub async fn get_access_point_info(&self, ap_path: &str) -> Result<AccessPointInfo> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            ap_path,
            "org.freedesktop.NetworkManager.AccessPoint",
        )
        .await?;

        let ssid_bytes: Vec<u8> = proxy.get_property("Ssid").await?;
        let ssid = String::from_utf8_lossy(&ssid_bytes).to_string();
        let strength: u8 = proxy.get_property("Strength").await?;
        let frequency: u32 = proxy.get_property("Frequency").await?;
        let hw_address: String = proxy.get_property("HwAddress").await?;
        let flags: u32 = proxy.get_property("Flags").await?;
        let wpa_flags: u32 = proxy.get_property("WpaFlags").await?;
        let rsn_flags: u32 = proxy.get_property("RsnFlags").await?;
        let mode: u32 = proxy.get_property("Mode").await?;

        let security = SecurityType::from_flags(flags, wpa_flags, rsn_flags);

        Ok(AccessPointInfo {
            path: ap_path.to_string(),
            ssid,
            strength,
            frequency,
            hw_address,
            security,
            mode: WifiMode::from(mode),
        })
    }

    /// Get all saved connections
    pub async fn get_connections(&self) -> Result<Vec<OwnedObjectPath>> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            "/org/freedesktop/NetworkManager/Settings",
            "org.freedesktop.NetworkManager.Settings",
        )
        .await?;

        Ok(proxy.call("ListConnections", &()).await?)
    }

    /// Get connection settings
    pub async fn get_connection_settings(
        &self,
        connection_path: &str,
    ) -> Result<HashMap<String, HashMap<String, OwnedValue>>> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            connection_path,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?;

        Ok(proxy.call("GetSettings", &()).await?)
    }

    /// Get connection secrets (passwords, etc.)
    pub async fn get_connection_secrets(
        &self,
        connection_path: &str,
        setting_name: &str,
    ) -> Result<HashMap<String, HashMap<String, OwnedValue>>> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            connection_path,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?;

        Ok(proxy.call("GetSecrets", &(setting_name,)).await?)
    }

    /// Get WiFi PSK (password) for a connection
    pub async fn get_wifi_psk(&self, connection_path: &str) -> Result<Option<String>> {
        let secrets = self
            .get_connection_secrets(connection_path, "802-11-wireless-security")
            .await?;

        if let Some(wifi_security) = secrets.get("802-11-wireless-security") {
            if let Some(psk) = wifi_security.get("psk") {
                let psk_str: String = psk.try_clone()?.try_into()?;
                return Ok(Some(psk_str));
            }
        }

        Ok(None)
    }

    /// Get WiFi connection profiles
    #[allow(clippy::collapsible_if, clippy::map_flatten)]
    pub async fn get_wifi_connections(&self) -> Result<Vec<ConnectionInfo>> {
        let connections = self.get_connections().await?;
        let mut wifi_connections = Vec::new();

        for conn_path in connections {
            if let Ok(settings) = self.get_connection_settings(conn_path.as_str()).await {
                if let Some(connection_settings) = settings.get("connection") {
                    if let Some(conn_type) = connection_settings.get("type") {
                        let type_str: String = conn_type.try_clone()?.try_into()?;
                        if type_str == "802-11-wireless" {
                            let id: String = connection_settings
                                .get("id")
                                .map(|v| v.try_clone().ok().and_then(|v| v.try_into().ok()))
                                .flatten()
                                .unwrap_or_default();

                            let uuid: String = connection_settings
                                .get("uuid")
                                .map(|v| v.try_clone().ok().and_then(|v| v.try_into().ok()))
                                .flatten()
                                .unwrap_or_default();

                            let autoconnect: bool = connection_settings
                                .get("autoconnect")
                                .map(|v| v.try_clone().ok().and_then(|v| v.try_into().ok()))
                                .flatten()
                                .unwrap_or(true);

                            let timestamp: u64 = connection_settings
                                .get("timestamp")
                                .map(|v| v.try_clone().ok().and_then(|v| v.try_into().ok()))
                                .flatten()
                                .unwrap_or(0);

                            // Get SSID from wireless settings
                            let ssid =
                                if let Some(wireless_settings) = settings.get("802-11-wireless") {
                                    wireless_settings
                                        .get("ssid")
                                        .map(|v| {
                                            v.try_clone().ok().and_then(|v| {
                                                let bytes: Result<Vec<u8>, _> = v.try_into();
                                                bytes.ok().map(|b| {
                                                    String::from_utf8_lossy(&b).to_string()
                                                })
                                            })
                                        })
                                        .flatten()
                                        .unwrap_or(id.clone())
                                } else {
                                    id.clone()
                                };

                            // Check if it's a hidden network
                            let hidden =
                                if let Some(wireless_settings) = settings.get("802-11-wireless") {
                                    wireless_settings
                                        .get("hidden")
                                        .map(|v| v.try_clone().ok().and_then(|v| v.try_into().ok()))
                                        .flatten()
                                        .unwrap_or(false)
                                } else {
                                    false
                                };

                            // Get security type from wireless-security settings
                            let security = if settings.contains_key("802-11-wireless-security") {
                                if settings.contains_key("802-1x") {
                                    SecurityType::Enterprise
                                } else {
                                    SecurityType::WPA
                                }
                            } else {
                                SecurityType::Open
                            };

                            wifi_connections.push(ConnectionInfo {
                                path: conn_path.to_string(),
                                id,
                                uuid,
                                ssid,
                                autoconnect,
                                timestamp,
                                hidden,
                                security,
                            });
                        }
                    }
                }
            }
        }

        // Sort by timestamp (most recent first)
        wifi_connections.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));

        Ok(wifi_connections)
    }

    /// Connect to a network using an existing connection profile
    pub async fn activate_connection(
        &self,
        connection_path: &str,
        device_path: &str,
    ) -> Result<OwnedObjectPath> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_PATH,
            "org.freedesktop.NetworkManager",
        )
        .await?;

        let specific_object = ObjectPath::try_from("/").unwrap();
        let active_connection: OwnedObjectPath = proxy
            .call(
                "ActivateConnection",
                &(
                    ObjectPath::try_from(connection_path)?,
                    ObjectPath::try_from(device_path)?,
                    specific_object,
                ),
            )
            .await?;

        Ok(active_connection)
    }

    /// Connect to a new network (creates connection profile)
    pub async fn add_and_activate_connection(
        &self,
        device_path: &str,
        ap_path: &str,
        password: Option<&str>,
    ) -> Result<OwnedObjectPath> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_PATH,
            "org.freedesktop.NetworkManager",
        )
        .await?;

        // Build connection settings
        let mut connection_settings: HashMap<&str, HashMap<&str, Value>> = HashMap::new();

        // Get AP info to determine security
        let ap_info = self.get_access_point_info(ap_path).await?;

        // Connection section
        let mut conn: HashMap<&str, Value> = HashMap::new();
        conn.insert("type", Value::from("802-11-wireless"));
        conn.insert("id", Value::from(ap_info.ssid.clone()));
        connection_settings.insert("connection", conn);

        // Wireless section
        let mut wireless: HashMap<&str, Value> = HashMap::new();
        wireless.insert("ssid", Value::from(ap_info.ssid.as_bytes().to_vec()));
        connection_settings.insert("802-11-wireless", wireless);

        // Security section (if needed)
        if ap_info.security != SecurityType::Open {
            let mut security: HashMap<&str, Value> = HashMap::new();

            match ap_info.security {
                SecurityType::WEP => {
                    security.insert("key-mgmt", Value::from("none"));
                    if let Some(pwd) = password {
                        security.insert("wep-key0", Value::from(pwd));
                    }
                }
                SecurityType::WPA | SecurityType::WPA2 | SecurityType::WPA3 => {
                    security.insert("key-mgmt", Value::from("wpa-psk"));
                    if let Some(pwd) = password {
                        security.insert("psk", Value::from(pwd));
                    }
                }
                SecurityType::Enterprise => {
                    security.insert("key-mgmt", Value::from("wpa-eap"));
                    // Enterprise auth needs additional 802-1x settings
                }
                _ => {}
            }
            connection_settings.insert("802-11-wireless-security", security);
        }

        // IPv4 section (auto)
        let mut ipv4: HashMap<&str, Value> = HashMap::new();
        ipv4.insert("method", Value::from("auto"));
        connection_settings.insert("ipv4", ipv4);

        // IPv6 section (auto)
        let mut ipv6: HashMap<&str, Value> = HashMap::new();
        ipv6.insert("method", Value::from("auto"));
        connection_settings.insert("ipv6", ipv6);

        let result: (OwnedObjectPath, OwnedObjectPath) = proxy
            .call(
                "AddAndActivateConnection",
                &(
                    connection_settings,
                    ObjectPath::try_from(device_path)?,
                    ObjectPath::try_from(ap_path)?,
                ),
            )
            .await?;

        Ok(result.1) // Return active connection path
    }

    /// Disconnect from current network
    pub async fn disconnect_device(&self, device_path: &str) -> Result<()> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            device_path,
            "org.freedesktop.NetworkManager.Device",
        )
        .await?;

        let _: () = proxy.call("Disconnect", &()).await?;
        Ok(())
    }

    /// Delete a saved connection
    pub async fn delete_connection(&self, connection_path: &str) -> Result<()> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            connection_path,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?;

        let _: () = proxy.call("Delete", &()).await?;
        Ok(())
    }

    /// Update connection autoconnect setting
    pub async fn set_connection_autoconnect(
        &self,
        connection_path: &str,
        autoconnect: bool,
    ) -> Result<()> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            connection_path,
            "org.freedesktop.NetworkManager.Settings.Connection",
        )
        .await?;

        // Get current settings
        let mut settings: HashMap<String, HashMap<String, OwnedValue>> =
            proxy.call("GetSettings", &()).await?;

        // Update autoconnect
        if let Some(connection) = settings.get_mut("connection") {
            connection.insert("autoconnect".to_string(), OwnedValue::from(autoconnect));
        }

        // Update the connection
        let _: () = proxy.call("Update", &(settings,)).await?;
        Ok(())
    }

    /// Get active connections
    pub async fn get_active_connections(&self) -> Result<Vec<OwnedObjectPath>> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_PATH,
            "org.freedesktop.NetworkManager",
        )
        .await?;

        Ok(proxy.get_property("ActiveConnections").await?)
    }

    /// Get active connection info
    pub async fn get_active_connection_info(
        &self,
        active_conn_path: &str,
    ) -> Result<ActiveConnectionInfo> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            active_conn_path,
            "org.freedesktop.NetworkManager.Connection.Active",
        )
        .await?;

        let id: String = proxy.get_property("Id").await?;
        let uuid: String = proxy.get_property("Uuid").await?;
        let state: u32 = proxy.get_property("State").await?;
        let connection_path: OwnedObjectPath = proxy.get_property("Connection").await?;
        let devices: Vec<OwnedObjectPath> = proxy.get_property("Devices").await?;

        Ok(ActiveConnectionInfo {
            path: active_conn_path.to_string(),
            id,
            uuid,
            state: ActiveConnectionState::from(state),
            connection_path: connection_path.to_string(),
            devices: devices.iter().map(|p| p.to_string()).collect(),
        })
    }

    /// Create a WiFi hotspot
    pub async fn create_hotspot(
        &self,
        device_path: &str,
        ssid: &str,
        password: &str,
    ) -> Result<OwnedObjectPath> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_PATH,
            "org.freedesktop.NetworkManager",
        )
        .await?;

        // Build hotspot connection settings
        let mut connection_settings: HashMap<&str, HashMap<&str, Value>> = HashMap::new();

        // Connection section
        let mut conn: HashMap<&str, Value> = HashMap::new();
        conn.insert("type", Value::from("802-11-wireless"));
        conn.insert("id", Value::from(format!("Hotspot {}", ssid)));
        conn.insert("autoconnect", Value::from(false));
        connection_settings.insert("connection", conn);

        // Wireless section
        let mut wireless: HashMap<&str, Value> = HashMap::new();
        wireless.insert("ssid", Value::from(ssid.as_bytes().to_vec()));
        wireless.insert("mode", Value::from("ap"));
        wireless.insert("band", Value::from("bg")); // 2.4GHz
        connection_settings.insert("802-11-wireless", wireless);

        // Security section
        let mut security: HashMap<&str, Value> = HashMap::new();
        security.insert("key-mgmt", Value::from("wpa-psk"));
        security.insert("psk", Value::from(password));
        connection_settings.insert("802-11-wireless-security", security);

        // IPv4 section (shared = NAT/DHCP for clients)
        let mut ipv4: HashMap<&str, Value> = HashMap::new();
        ipv4.insert("method", Value::from("shared"));
        connection_settings.insert("ipv4", ipv4);

        // IPv6 section (ignore for hotspot)
        let mut ipv6: HashMap<&str, Value> = HashMap::new();
        ipv6.insert("method", Value::from("ignore"));
        connection_settings.insert("ipv6", ipv6);

        let result: (OwnedObjectPath, OwnedObjectPath) = proxy
            .call(
                "AddAndActivateConnection",
                &(
                    connection_settings,
                    ObjectPath::try_from(device_path)?,
                    ObjectPath::try_from("/")?,
                ),
            )
            .await?;

        Ok(result.1)
    }

    /// Stop hotspot (deactivate connection)
    pub async fn deactivate_connection(&self, active_connection_path: &str) -> Result<()> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_PATH,
            "org.freedesktop.NetworkManager",
        )
        .await?;

        let _: () = proxy
            .call(
                "DeactivateConnection",
                &(ObjectPath::try_from(active_connection_path)?,),
            )
            .await?;
        Ok(())
    }

    /// Add 802.1X enterprise connection via D-Bus
    #[allow(clippy::too_many_arguments, clippy::collapsible_if)]
    pub async fn add_enterprise_connection(
        &self,
        ssid: &str,
        eap_method: &str,
        identity: &str,
        password: Option<&str>,
        phase2_auth: Option<&str>,
        ca_cert: Option<&str>,
        client_cert: Option<&str>,
        private_key: Option<&str>,
        private_key_password: Option<&str>,
    ) -> Result<OwnedObjectPath> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            "/org/freedesktop/NetworkManager/Settings",
            "org.freedesktop.NetworkManager.Settings",
        )
        .await?;

        let mut connection_settings: HashMap<&str, HashMap<&str, Value>> = HashMap::new();

        // Connection section
        let mut conn: HashMap<&str, Value> = HashMap::new();
        conn.insert("type", Value::from("802-11-wireless"));
        conn.insert("id", Value::from(ssid));
        connection_settings.insert("connection", conn);

        // Wireless section
        let mut wireless: HashMap<&str, Value> = HashMap::new();
        wireless.insert("ssid", Value::from(ssid.as_bytes().to_vec()));
        connection_settings.insert("802-11-wireless", wireless);

        // Wireless security section
        let mut security: HashMap<&str, Value> = HashMap::new();
        security.insert("key-mgmt", Value::from("wpa-eap"));
        connection_settings.insert("802-11-wireless-security", security);

        // 802.1X section
        let mut eap: HashMap<&str, Value> = HashMap::new();
        eap.insert("eap", Value::from(vec![eap_method]));
        eap.insert("identity", Value::from(identity));

        if let Some(pwd) = password {
            eap.insert("password", Value::from(pwd));
        }

        if let Some(phase2) = phase2_auth {
            eap.insert("phase2-auth", Value::from(phase2));
        }

        if let Some(ca) = ca_cert {
            if !ca.is_empty() {
                eap.insert(
                    "ca-cert",
                    Value::from(format!("file://{}", ca).as_bytes().to_vec()),
                );
            }
        }

        if let Some(cert) = client_cert {
            if !cert.is_empty() {
                eap.insert(
                    "client-cert",
                    Value::from(format!("file://{}", cert).as_bytes().to_vec()),
                );
            }
        }

        if let Some(key) = private_key {
            if !key.is_empty() {
                eap.insert(
                    "private-key",
                    Value::from(format!("file://{}", key).as_bytes().to_vec()),
                );
            }
        }

        if let Some(key_pwd) = private_key_password {
            if !key_pwd.is_empty() {
                eap.insert("private-key-password", Value::from(key_pwd));
            }
        }

        connection_settings.insert("802-1x", eap);

        // IPv4 section
        let mut ipv4: HashMap<&str, Value> = HashMap::new();
        ipv4.insert("method", Value::from("auto"));
        connection_settings.insert("ipv4", ipv4);

        // IPv6 section
        let mut ipv6: HashMap<&str, Value> = HashMap::new();
        ipv6.insert("method", Value::from("auto"));
        connection_settings.insert("ipv6", ipv6);

        let connection_path: OwnedObjectPath =
            proxy.call("AddConnection", &(connection_settings,)).await?;

        Ok(connection_path)
    }

    /// Add and activate 802.1X enterprise connection
    #[allow(clippy::too_many_arguments, clippy::collapsible_if)]
    pub async fn add_and_activate_enterprise_connection(
        &self,
        device_path: &str,
        ssid: &str,
        eap_method: &str,
        identity: &str,
        password: Option<&str>,
        phase2_auth: Option<&str>,
        ca_cert: Option<&str>,
        client_cert: Option<&str>,
        private_key: Option<&str>,
        private_key_password: Option<&str>,
    ) -> Result<OwnedObjectPath> {
        let proxy = Proxy::new(
            &self.connection,
            NM_BUS_NAME,
            NM_PATH,
            "org.freedesktop.NetworkManager",
        )
        .await?;

        let mut connection_settings: HashMap<&str, HashMap<&str, Value>> = HashMap::new();

        // Connection section
        let mut conn: HashMap<&str, Value> = HashMap::new();
        conn.insert("type", Value::from("802-11-wireless"));
        conn.insert("id", Value::from(ssid));
        connection_settings.insert("connection", conn);

        // Wireless section
        let mut wireless: HashMap<&str, Value> = HashMap::new();
        wireless.insert("ssid", Value::from(ssid.as_bytes().to_vec()));
        connection_settings.insert("802-11-wireless", wireless);

        // Wireless security section
        let mut security: HashMap<&str, Value> = HashMap::new();
        security.insert("key-mgmt", Value::from("wpa-eap"));
        connection_settings.insert("802-11-wireless-security", security);

        // 802.1X section
        let mut eap: HashMap<&str, Value> = HashMap::new();
        eap.insert("eap", Value::from(vec![eap_method]));
        eap.insert("identity", Value::from(identity));

        if let Some(pwd) = password {
            eap.insert("password", Value::from(pwd));
        }

        if let Some(phase2) = phase2_auth {
            eap.insert("phase2-auth", Value::from(phase2));
        }

        if let Some(ca) = ca_cert {
            if !ca.is_empty() {
                eap.insert(
                    "ca-cert",
                    Value::from(format!("file://{}", ca).as_bytes().to_vec()),
                );
            }
        }

        if let Some(cert) = client_cert {
            if !cert.is_empty() {
                eap.insert(
                    "client-cert",
                    Value::from(format!("file://{}", cert).as_bytes().to_vec()),
                );
            }
        }

        if let Some(key) = private_key {
            if !key.is_empty() {
                eap.insert(
                    "private-key",
                    Value::from(format!("file://{}", key).as_bytes().to_vec()),
                );
            }
        }

        if let Some(key_pwd) = private_key_password {
            if !key_pwd.is_empty() {
                eap.insert("private-key-password", Value::from(key_pwd));
            }
        }

        connection_settings.insert("802-1x", eap);

        // IPv4 section
        let mut ipv4: HashMap<&str, Value> = HashMap::new();
        ipv4.insert("method", Value::from("auto"));
        connection_settings.insert("ipv4", ipv4);

        // IPv6 section
        let mut ipv6: HashMap<&str, Value> = HashMap::new();
        ipv6.insert("method", Value::from("auto"));
        connection_settings.insert("ipv6", ipv6);

        let result: (OwnedObjectPath, OwnedObjectPath) = proxy
            .call(
                "AddAndActivateConnection",
                &(
                    connection_settings,
                    ObjectPath::try_from(device_path)?,
                    ObjectPath::try_from("/")?,
                ),
            )
            .await?;

        Ok(result.1)
    }
}
