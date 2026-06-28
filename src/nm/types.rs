// NetworkManager types and enums

use std::fmt;
use std::net::IpAddr;

/// A WireGuard peer parsed from a `.conf` `[Peer]` section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgPeerConfig {
    pub public_key: String,
    pub endpoint: Option<String>,
    pub allowed_ips: Vec<String>,
    pub preshared_key: Option<String>,
    pub persistent_keepalive: Option<u32>,
}

/// A WireGuard tunnel parsed from a `.conf` file, ready to be turned into a
/// NetworkManager `wireguard` connection. Holds a single peer (the common case
/// for hosted providers like Proton/Mullvad).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WgConfig {
    pub private_key: String,
    /// Interface addresses as (ip, prefix); may mix IPv4 and IPv6.
    pub addresses: Vec<(IpAddr, u8)>,
    pub dns: Vec<IpAddr>,
    pub peer: WgPeerConfig,
}

/// Summary of an active wired (802-3-ethernet) connection. Tracked
/// independently of the WiFi device so link status stays visible even when the
/// WiFi radio is powered off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EthernetInfo {
    pub id: String,
    pub interface: Option<String>,
    pub ipv4: Option<String>,
}

/// Kind of physical link, derived from a NetworkManager connection type. Used
/// to flag and switch which link carries the default route.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkKind {
    Wifi,
    Ethernet,
    Other,
}

impl LinkKind {
    /// Maps a NetworkManager connection `type` string to a link kind.
    pub fn from_nm_type(nm_type: &str) -> Self {
        match nm_type {
            "802-11-wireless" => LinkKind::Wifi,
            "802-3-ethernet" => LinkKind::Ethernet,
            _ => LinkKind::Other,
        }
    }
}

impl fmt::Display for LinkKind {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            LinkKind::Wifi => write!(f, "WiFi"),
            LinkKind::Ethernet => write!(f, "Ethernet"),
            LinkKind::Other => write!(f, "connection"),
        }
    }
}

/// The connection NetworkManager is using as the default route — i.e. the link
/// that actually carries internet traffic. Resolved from the Manager's
/// `PrimaryConnection`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrimaryLink {
    pub id: String,
    pub kind: LinkKind,
}

/// Active IPv4 configuration pulled from an NM device's `Ip4Config` object.
#[derive(Debug, Clone, Default)]
pub struct Ip4Info {
    pub addresses: Vec<(String, u32)>,
    pub gateway: Option<String>,
    pub nameservers: Vec<String>,
}

/// NetworkManager connectivity state. Mirrors `NM_CONNECTIVITY_*` values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Connectivity {
    Unknown,
    None,
    Portal,
    Limited,
    Full,
}

impl From<u32> for Connectivity {
    fn from(value: u32) -> Self {
        match value {
            1 => Connectivity::None,
            2 => Connectivity::Portal,
            3 => Connectivity::Limited,
            4 => Connectivity::Full,
            _ => Connectivity::Unknown,
        }
    }
}

#[cfg(test)]
mod connectivity_tests {
    use super::Connectivity;

    #[test]
    fn known_nm_codes_map_correctly() {
        assert_eq!(Connectivity::from(1), Connectivity::None);
        assert_eq!(Connectivity::from(2), Connectivity::Portal);
        assert_eq!(Connectivity::from(3), Connectivity::Limited);
        assert_eq!(Connectivity::from(4), Connectivity::Full);
    }

    #[test]
    fn unknown_codes_fall_back() {
        assert_eq!(Connectivity::from(0), Connectivity::Unknown);
        assert_eq!(Connectivity::from(99), Connectivity::Unknown);
    }
}

impl fmt::Display for Connectivity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Connectivity::Unknown => write!(f, "unknown"),
            Connectivity::None => write!(f, "no connectivity"),
            Connectivity::Portal => write!(f, "captive portal"),
            Connectivity::Limited => write!(f, "limited"),
            Connectivity::Full => write!(f, "full internet"),
        }
    }
}

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

/// Minimum length of a WPA/WPA2-PSK passphrase, per the WPA spec (8–63 ASCII).
pub const WPA_PSK_MIN_LEN: usize = 8;

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

    /// Validate a PSK passphrase against this security type's length rules,
    /// returning a user-facing message on failure.
    ///
    /// Only WPA/WPA2-PSK impose the WPA minimum ([`WPA_PSK_MIN_LEN`]). WPA3-SAE
    /// has no minimum-length requirement, WEP uses its own 5/13-char key
    /// lengths, and Enterprise authenticates with EAP credentials (not via the
    /// PSK prompt), so all of those pass here and are left to NetworkManager's
    /// own validation.
    pub fn validate_psk(&self, psk: &str) -> Result<(), &'static str> {
        if matches!(self, SecurityType::WPA | SecurityType::WPA2) && psk.len() < WPA_PSK_MIN_LEN {
            return Err("Password must be at least 8 characters");
        }
        Ok(())
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

/// Kind of VPN profile, mirroring NetworkManager's `connection.type`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VpnKind {
    /// Plugin-based VPN (openvpn, openconnect, wireguard-via-plugin, ...).
    Vpn,
    /// NetworkManager-native WireGuard.
    WireGuard,
}

impl fmt::Display for VpnKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VpnKind::Vpn => write!(f, "vpn"),
            VpnKind::WireGuard => write!(f, "wireguard"),
        }
    }
}

/// Saved VPN connection profile.
#[derive(Debug, Clone)]
pub struct VpnConnectionInfo {
    pub path: String,
    pub id: String,
    pub uuid: String,
    pub kind: VpnKind,
    /// NetworkManager interface name (`connection.interface-name`); empty when
    /// the profile doesn't pin one. Used to avoid interface-name collisions on
    /// import.
    pub interface_name: String,
    /// Whether NetworkManager brings this profile up automatically.
    pub autoconnect: bool,
    /// Epoch seconds of the profile's last successful activation (`0` if never).
    pub timestamp: u64,
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

/// Classification of why an activation attempt did not reach `Activated`.
/// Maps NetworkManager's `NMActiveConnectionStateReason` codes into the
/// distinctions the UI actually surfaces to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationFailureReason {
    /// Wrong / missing passphrase (NM reasons `NoSecrets` and `LoginFailed`).
    BadSecrets,
    /// NM reported the SSID could not be found (device reason `SsidNotFound`,
    /// 53). This is genuinely ambiguous: NM emits it both when the network is
    /// out of range / gone and, on many drivers, when a wrong PSK fails the
    /// 4-way handshake and the AP deauthenticates. We surface it honestly
    /// rather than guessing "wrong password".
    SsidNotFound,
    /// Activation did not reach a terminal state within our wait window.
    Timeout,
    /// Any other terminal failure; carries the raw NM reason code so callers
    /// can log it without inventing a name for every value.
    Other(u32),
}

impl ActivationFailureReason {
    /// Map a raw `NMActiveConnectionStateReason` to our classification.
    ///
    /// The Active-Connection layer almost always reports `DeviceDisconnected`
    /// (3) when the real failure happened at the device layer — use
    /// [`Self::from_nm_device_reason`] in tandem to get the canonical reason.
    pub fn from_nm_active_reason(reason: u32) -> Self {
        // 9  = NM_ACTIVE_CONNECTION_STATE_REASON_NO_SECRETS
        // 10 = NM_ACTIVE_CONNECTION_STATE_REASON_LOGIN_FAILED
        match reason {
            9 | 10 => Self::BadSecrets,
            other => Self::Other(other),
        }
    }

    /// Map a raw `NMDeviceStateReason` to our classification.
    ///
    /// This is where the wifi-specific auth-failure signals actually live —
    /// the Active-Connection signal only ever reports `DeviceDisconnected` (3)
    /// for these cases, which is useless to the user.
    pub fn from_nm_device_reason(reason: u32) -> Self {
        // 7  = NM_DEVICE_STATE_REASON_NO_SECRETS
        // 8  = NM_DEVICE_STATE_REASON_SUPPLICANT_DISCONNECT
        // 9  = NM_DEVICE_STATE_REASON_SUPPLICANT_CONFIG_FAILED
        // 10 = NM_DEVICE_STATE_REASON_SUPPLICANT_FAILED
        // 53 = NM_DEVICE_STATE_REASON_SSID_NOT_FOUND (also a common
        //      wrong-PSK symptom — see `SsidNotFound`, kept distinct so the
        //      message stays truthful rather than asserting "wrong password").
        match reason {
            7..=10 => Self::BadSecrets,
            53 => Self::SsidNotFound,
            other => Self::Other(other),
        }
    }
}

/// Terminal result of waiting on an active connection's state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActivationOutcome {
    Activated,
    Failed(ActivationFailureReason),
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

    #[test]
    fn test_activation_failure_reason_from_nm_active_reason() {
        assert_eq!(
            ActivationFailureReason::from_nm_active_reason(9),
            ActivationFailureReason::BadSecrets
        );
        assert_eq!(
            ActivationFailureReason::from_nm_active_reason(10),
            ActivationFailureReason::BadSecrets
        );
        // 3 (DeviceDisconnected) is the AC-layer code we see for device-side
        // auth failures — must NOT classify as BadSecrets at this layer.
        assert_eq!(
            ActivationFailureReason::from_nm_active_reason(3),
            ActivationFailureReason::Other(3)
        );
        assert_eq!(
            ActivationFailureReason::from_nm_active_reason(0),
            ActivationFailureReason::Other(0)
        );
    }

    #[test]
    fn test_activation_failure_reason_from_nm_device_reason() {
        for code in [7, 8, 9, 10] {
            assert_eq!(
                ActivationFailureReason::from_nm_device_reason(code),
                ActivationFailureReason::BadSecrets,
                "device reason {} should classify as BadSecrets",
                code
            );
        }
        // 53 = SSID_NOT_FOUND: kept distinct so the message stays honest
        // rather than asserting a wrong password.
        assert_eq!(
            ActivationFailureReason::from_nm_device_reason(53),
            ActivationFailureReason::SsidNotFound
        );
        assert_eq!(
            ActivationFailureReason::from_nm_device_reason(0),
            ActivationFailureReason::Other(0)
        );
        assert_eq!(
            ActivationFailureReason::from_nm_device_reason(6),
            ActivationFailureReason::Other(6)
        );
    }

    #[test]
    fn test_validate_psk_enforces_min_only_for_wpa_psk() {
        // WPA/WPA2-PSK reject short passphrases but accept >= 8.
        for ty in [SecurityType::WPA, SecurityType::WPA2] {
            assert!(ty.validate_psk("short").is_err(), "{:?} short", ty);
            assert!(ty.validate_psk("12345678").is_ok(), "{:?} 8 chars", ty);
        }
        // WPA3-SAE has no minimum; WEP/Enterprise/Open are not PSK prompts.
        for ty in [
            SecurityType::WPA3,
            SecurityType::WEP,
            SecurityType::Enterprise,
            SecurityType::Open,
        ] {
            assert!(
                ty.validate_psk("short").is_ok(),
                "{:?} should not enforce the 8-char PSK rule",
                ty
            );
        }
    }
}
