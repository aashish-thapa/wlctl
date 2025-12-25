// NetworkManager types and enums

use std::fmt;

/// WiFi operation mode (replaces iwdrs::modes::Mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    #[default]
    Station,
    Ap,
}

impl fmt::Display for Mode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Mode::Station => write!(f, "station"),
            Mode::Ap => write!(f, "ap"),
        }
    }
}

impl TryFrom<&str> for Mode {
    type Error = anyhow::Error;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value.to_lowercase().as_str() {
            "station" => Ok(Mode::Station),
            "ap" => Ok(Mode::Ap),
            _ => Err(anyhow::anyhow!("Invalid mode: {}", value)),
        }
    }
}

/// NetworkManager device state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceState {
    Unknown,
    Unmanaged,
    Unavailable,
    Disconnected,
    Prepare,
    Config,
    NeedAuth,
    IpConfig,
    IpCheck,
    Secondaries,
    Activated,
    Deactivating,
    Failed,
}

impl From<u32> for DeviceState {
    fn from(value: u32) -> Self {
        match value {
            0 => DeviceState::Unknown,
            10 => DeviceState::Unmanaged,
            20 => DeviceState::Unavailable,
            30 => DeviceState::Disconnected,
            40 => DeviceState::Prepare,
            50 => DeviceState::Config,
            60 => DeviceState::NeedAuth,
            70 => DeviceState::IpConfig,
            80 => DeviceState::IpCheck,
            90 => DeviceState::Secondaries,
            100 => DeviceState::Activated,
            110 => DeviceState::Deactivating,
            120 => DeviceState::Failed,
            _ => DeviceState::Unknown,
        }
    }
}

impl fmt::Display for DeviceState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeviceState::Unknown => write!(f, "unknown"),
            DeviceState::Unmanaged => write!(f, "unmanaged"),
            DeviceState::Unavailable => write!(f, "unavailable"),
            DeviceState::Disconnected => write!(f, "disconnected"),
            DeviceState::Prepare => write!(f, "connecting"),
            DeviceState::Config => write!(f, "configuring"),
            DeviceState::NeedAuth => write!(f, "authenticating"),
            DeviceState::IpConfig => write!(f, "getting IP"),
            DeviceState::IpCheck => write!(f, "checking IP"),
            DeviceState::Secondaries => write!(f, "waiting"),
            DeviceState::Activated => write!(f, "connected"),
            DeviceState::Deactivating => write!(f, "disconnecting"),
            DeviceState::Failed => write!(f, "failed"),
        }
    }
}

/// WiFi access point mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WifiMode {
    Unknown,
    Adhoc,
    Infrastructure,
    Ap,
    Mesh,
}

impl From<u32> for WifiMode {
    fn from(value: u32) -> Self {
        match value {
            0 => WifiMode::Unknown,
            1 => WifiMode::Adhoc,
            2 => WifiMode::Infrastructure,
            3 => WifiMode::Ap,
            4 => WifiMode::Mesh,
            _ => WifiMode::Unknown,
        }
    }
}

/// Security type for WiFi networks
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SecurityType {
    #[default]
    Open,
    WEP,
    WPA,
    WPA2,
    WPA3,
    Enterprise,
}

impl SecurityType {
    pub fn from_flags(flags: u32, wpa_flags: u32, rsn_flags: u32) -> Self {
        // NM_802_11_AP_FLAGS_PRIVACY = 0x1
        let has_privacy = flags & 0x1 != 0;

        // Check for enterprise auth (EAP)
        // NM_802_11_AP_SEC_KEY_MGMT_802_1X = 0x200
        if wpa_flags & 0x200 != 0 || rsn_flags & 0x200 != 0 {
            return SecurityType::Enterprise;
        }

        // Check RSN (WPA2/WPA3)
        if rsn_flags != 0 {
            // NM_802_11_AP_SEC_KEY_MGMT_SAE = 0x400 (WPA3)
            if rsn_flags & 0x400 != 0 {
                return SecurityType::WPA3;
            }
            return SecurityType::WPA2;
        }

        // Check WPA
        if wpa_flags != 0 {
            return SecurityType::WPA;
        }

        // Check WEP
        if has_privacy {
            return SecurityType::WEP;
        }

        SecurityType::Open
    }

    pub fn requires_password(&self) -> bool {
        !matches!(self, SecurityType::Open)
    }

    pub fn is_enterprise(&self) -> bool {
        matches!(self, SecurityType::Enterprise)
    }
}

impl fmt::Display for SecurityType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SecurityType::Open => write!(f, "open"),
            SecurityType::WEP => write!(f, "wep"),
            SecurityType::WPA => write!(f, "wpa"),
            SecurityType::WPA2 => write!(f, "wpa2"),
            SecurityType::WPA3 => write!(f, "wpa3"),
            SecurityType::Enterprise => write!(f, "8021x"),
        }
    }
}

/// Scanned access point information
#[derive(Debug, Clone)]
pub struct AccessPointInfo {
    pub path: String,
    pub ssid: String,
    pub strength: u8,
    pub frequency: u32,
    pub hw_address: String,
    pub security: SecurityType,
    pub mode: WifiMode,
}

impl AccessPointInfo {
    /// Get frequency band (2.4GHz or 5GHz)
    pub fn band(&self) -> &str {
        if self.frequency < 3000 {
            "2.4 GHz"
        } else {
            "5 GHz"
        }
    }

    /// Get channel from frequency
    pub fn channel(&self) -> u32 {
        if self.frequency < 3000 {
            // 2.4 GHz
            (self.frequency - 2407) / 5
        } else {
            // 5 GHz
            (self.frequency - 5000) / 5
        }
    }
}

/// Saved connection profile information
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub path: String,
    pub id: String,
    pub uuid: String,
    pub ssid: String,
    pub autoconnect: bool,
    pub timestamp: u64,
    pub hidden: bool,
    pub security: SecurityType,
}

/// Active connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveConnectionState {
    Unknown,
    Activating,
    Activated,
    Deactivating,
    Deactivated,
}

impl From<u32> for ActiveConnectionState {
    fn from(value: u32) -> Self {
        match value {
            0 => ActiveConnectionState::Unknown,
            1 => ActiveConnectionState::Activating,
            2 => ActiveConnectionState::Activated,
            3 => ActiveConnectionState::Deactivating,
            4 => ActiveConnectionState::Deactivated,
            _ => ActiveConnectionState::Unknown,
        }
    }
}

/// Active connection info
#[derive(Debug, Clone)]
pub struct ActiveConnectionInfo {
    pub path: String,
    pub id: String,
    pub uuid: String,
    pub state: ActiveConnectionState,
    pub connection_path: String,
    pub devices: Vec<String>,
}

/// Station state (compatible with iwd station states for UI)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StationState {
    #[default]
    Disconnected,
    Connecting,
    Connected,
    Disconnecting,
    Roaming,
}

impl From<DeviceState> for StationState {
    fn from(state: DeviceState) -> Self {
        match state {
            DeviceState::Activated => StationState::Connected,
            DeviceState::Prepare
            | DeviceState::Config
            | DeviceState::NeedAuth
            | DeviceState::IpConfig
            | DeviceState::IpCheck
            | DeviceState::Secondaries => StationState::Connecting,
            DeviceState::Deactivating => StationState::Disconnecting,
            _ => StationState::Disconnected,
        }
    }
}

impl fmt::Display for StationState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StationState::Disconnected => write!(f, "disconnected"),
            StationState::Connecting => write!(f, "connecting"),
            StationState::Connected => write!(f, "connected"),
            StationState::Disconnecting => write!(f, "disconnecting"),
            StationState::Roaming => write!(f, "roaming"),
        }
    }
}

/// Network diagnostic information
#[derive(Debug, Clone, Default)]
pub struct DiagnosticInfo {
    pub frequency: Option<u32>,
    pub signal_strength: Option<i32>,
    pub tx_bitrate: Option<u32>,
    pub rx_bitrate: Option<u32>,
    pub security: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_from_str() {
        assert_eq!(Mode::try_from("station").unwrap(), Mode::Station);
        assert_eq!(Mode::try_from("Station").unwrap(), Mode::Station);
        assert_eq!(Mode::try_from("ap").unwrap(), Mode::Ap);
        assert_eq!(Mode::try_from("AP").unwrap(), Mode::Ap);
        assert!(Mode::try_from("invalid").is_err());
    }

    #[test]
    fn test_mode_display() {
        assert_eq!(Mode::Station.to_string(), "station");
        assert_eq!(Mode::Ap.to_string(), "ap");
    }

    #[test]
    fn test_device_state_from_u32() {
        assert_eq!(DeviceState::from(0), DeviceState::Unknown);
        assert_eq!(DeviceState::from(30), DeviceState::Disconnected);
        assert_eq!(DeviceState::from(100), DeviceState::Activated);
        assert_eq!(DeviceState::from(120), DeviceState::Failed);
        assert_eq!(DeviceState::from(999), DeviceState::Unknown);
    }

    #[test]
    fn test_security_type_from_flags() {
        // Open network
        assert_eq!(SecurityType::from_flags(0, 0, 0), SecurityType::Open);

        // WEP (privacy flag set, no WPA/RSN)
        assert_eq!(SecurityType::from_flags(0x1, 0, 0), SecurityType::WEP);

        // WPA
        assert_eq!(SecurityType::from_flags(0, 0x1, 0), SecurityType::WPA);

        // WPA2
        assert_eq!(SecurityType::from_flags(0, 0, 0x1), SecurityType::WPA2);

        // WPA3
        assert_eq!(SecurityType::from_flags(0, 0, 0x400), SecurityType::WPA3);

        // Enterprise
        assert_eq!(
            SecurityType::from_flags(0, 0x200, 0),
            SecurityType::Enterprise
        );
        assert_eq!(
            SecurityType::from_flags(0, 0, 0x200),
            SecurityType::Enterprise
        );
    }

    #[test]
    fn test_security_type_requires_password() {
        assert!(!SecurityType::Open.requires_password());
        assert!(SecurityType::WEP.requires_password());
        assert!(SecurityType::WPA.requires_password());
        assert!(SecurityType::WPA2.requires_password());
        assert!(SecurityType::WPA3.requires_password());
        assert!(SecurityType::Enterprise.requires_password());
    }

    #[test]
    fn test_security_type_is_enterprise() {
        assert!(!SecurityType::Open.is_enterprise());
        assert!(!SecurityType::WPA2.is_enterprise());
        assert!(SecurityType::Enterprise.is_enterprise());
    }

    #[test]
    fn test_access_point_band() {
        let ap_2g = AccessPointInfo {
            path: String::new(),
            ssid: "Test".to_string(),
            strength: 80,
            frequency: 2412,
            hw_address: String::new(),
            security: SecurityType::Open,
            mode: WifiMode::Infrastructure,
        };
        assert_eq!(ap_2g.band(), "2.4 GHz");

        let ap_5g = AccessPointInfo {
            path: String::new(),
            ssid: "Test".to_string(),
            strength: 80,
            frequency: 5180,
            hw_address: String::new(),
            security: SecurityType::Open,
            mode: WifiMode::Infrastructure,
        };
        assert_eq!(ap_5g.band(), "5 GHz");
    }

    #[test]
    fn test_access_point_channel() {
        let ap = AccessPointInfo {
            path: String::new(),
            ssid: "Test".to_string(),
            strength: 80,
            frequency: 2412,
            hw_address: String::new(),
            security: SecurityType::Open,
            mode: WifiMode::Infrastructure,
        };
        assert_eq!(ap.channel(), 1);
    }

    #[test]
    fn test_station_state_from_device_state() {
        assert_eq!(
            StationState::from(DeviceState::Activated),
            StationState::Connected
        );
        assert_eq!(
            StationState::from(DeviceState::Prepare),
            StationState::Connecting
        );
        assert_eq!(
            StationState::from(DeviceState::Deactivating),
            StationState::Disconnecting
        );
        assert_eq!(
            StationState::from(DeviceState::Disconnected),
            StationState::Disconnected
        );
    }

    #[test]
    fn test_wifi_mode_from_u32() {
        assert_eq!(WifiMode::from(0), WifiMode::Unknown);
        assert_eq!(WifiMode::from(1), WifiMode::Adhoc);
        assert_eq!(WifiMode::from(2), WifiMode::Infrastructure);
        assert_eq!(WifiMode::from(3), WifiMode::Ap);
        assert_eq!(WifiMode::from(4), WifiMode::Mesh);
        assert_eq!(WifiMode::from(99), WifiMode::Unknown);
    }

    #[test]
    fn test_active_connection_state_from_u32() {
        assert_eq!(
            ActiveConnectionState::from(0),
            ActiveConnectionState::Unknown
        );
        assert_eq!(
            ActiveConnectionState::from(1),
            ActiveConnectionState::Activating
        );
        assert_eq!(
            ActiveConnectionState::from(2),
            ActiveConnectionState::Activated
        );
        assert_eq!(
            ActiveConnectionState::from(3),
            ActiveConnectionState::Deactivating
        );
        assert_eq!(
            ActiveConnectionState::from(4),
            ActiveConnectionState::Deactivated
        );
    }
}
