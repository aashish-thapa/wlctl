// Single-read view of NetworkManager's exported object graph.

use std::collections::HashMap;

use anyhow::Result;
use zbus::Proxy;
use zbus::zvariant::{OwnedObjectPath, OwnedValue};

use super::{
    AccessPointInfo, ActiveConnectionInfo, ActiveConnectionState, DeviceState, NM_BUS_NAME,
    NMClient, SecurityType, WifiMode,
};

/// NetworkManager exports its ObjectManager under the shared `/org/freedesktop`
/// path rather than beneath its own `/org/freedesktop/NetworkManager` tree.
const OBJECT_MANAGER_PATH: &str = "/org/freedesktop";

/// D-Bus interface names read out of a snapshot.
pub(crate) mod interface {
    pub const ACCESS_POINT: &str = "org.freedesktop.NetworkManager.AccessPoint";
    pub const CONNECTION_ACTIVE: &str = "org.freedesktop.NetworkManager.Connection.Active";
    pub const DEVICE: &str = "org.freedesktop.NetworkManager.Device";
    pub const DEVICE_WIRELESS: &str = "org.freedesktop.NetworkManager.Device.Wireless";
    pub const NETWORK_MANAGER: &str = "org.freedesktop.NetworkManager";
    pub const SETTINGS_CONNECTION: &str = "org.freedesktop.NetworkManager.Settings.Connection";
}

/// `NM_DEVICE_TYPE_WIFI`.
const WIFI_DEVICE_TYPE: u32 = 2;

/// Object path -> interface name -> property name -> value, as returned by a
/// single `GetManagedObjects` call.
type ManagedObjects = HashMap<OwnedObjectPath, HashMap<String, HashMap<String, OwnedValue>>>;

/// Reads one property out of a D-Bus property map, yielding `None` when it is
/// absent or not convertible to `T`.
fn prop<T>(props: &HashMap<String, OwnedValue>, key: &str) -> Option<T>
where
    T: TryFrom<OwnedValue>,
{
    props.get(key)?.try_clone().ok()?.try_into().ok()
}

/// Builds an `AccessPointInfo` from an `AccessPoint` interface's properties,
/// whichever way they were fetched.
///
/// Missing properties fall back to zero rather than failing: a snapshot can
/// legitimately race an access point out of existence, and a momentarily
/// under-described network reads better than a refresh that aborts.
pub(crate) fn access_point_info(
    ap_path: &str,
    props: &HashMap<String, OwnedValue>,
) -> AccessPointInfo {
    let ssid_bytes: Vec<u8> = prop(props, "Ssid").unwrap_or_default();

    AccessPointInfo {
        path: ap_path.to_string(),
        ssid: String::from_utf8_lossy(&ssid_bytes).to_string(),
        strength: prop(props, "Strength").unwrap_or(0),
        frequency: prop(props, "Frequency").unwrap_or(0),
        hw_address: prop(props, "HwAddress").unwrap_or_default(),
        security: SecurityType::from_flags(
            prop(props, "Flags").unwrap_or(0),
            prop(props, "WpaFlags").unwrap_or(0),
            prop(props, "RsnFlags").unwrap_or(0),
        ),
        mode: WifiMode::from(prop::<u32>(props, "Mode").unwrap_or(0)),
    }
}

/// Builds an `ActiveConnectionInfo` from a `Connection.Active` interface's
/// properties, whichever way they were fetched.
///
/// A connection can deactivate while it is being read, so absent properties
/// fall back to empty rather than failing and taking a whole refresh with them.
pub(crate) fn active_connection_info(
    path: &str,
    props: &HashMap<String, OwnedValue>,
) -> ActiveConnectionInfo {
    let devices: Vec<OwnedObjectPath> = prop(props, "Devices").unwrap_or_default();

    ActiveConnectionInfo {
        path: path.to_string(),
        id: prop(props, "Id").unwrap_or_default(),
        uuid: prop(props, "Uuid").unwrap_or_default(),
        connection_type: prop(props, "Type").unwrap_or_default(),
        state: ActiveConnectionState::from(prop::<u32>(props, "State").unwrap_or(0)),
        connection_path: prop::<OwnedObjectPath>(props, "Connection")
            .map(|path| path.as_str().to_string())
            .unwrap_or_default(),
        devices: devices
            .iter()
            .map(|path| path.as_str().to_string())
            .collect(),
    }
}

/// A point-in-time view of every object NetworkManager exports.
///
/// The UI re-reads every access point and saved profile on a timer, and reading
/// them one object at a time costs a D-Bus round trip each — and a zbus proxy
/// property read costs more than that, since it also registers a match rule and
/// spawns a cache task it immediately drops. One `GetManagedObjects` replaces
/// all of them, so a single snapshot can serve an entire refresh, and every
/// value read from it is consistent with every other.
///
/// Deliberately neither `Clone` nor `Debug`: copying the object graph is the
/// cost this type exists to avoid, and printing it would dump the whole bus.
pub struct NmSnapshot {
    objects: ManagedObjects,
}

impl NmSnapshot {
    pub async fn fetch(client: &NMClient) -> Result<Self> {
        let proxy = Proxy::new(
            client.connection(),
            NM_BUS_NAME,
            OBJECT_MANAGER_PATH,
            "org.freedesktop.DBus.ObjectManager",
        )
        .await?;

        Ok(Self {
            objects: proxy.call("GetManagedObjects", &()).await?,
        })
    }

    /// Whether the WiFi radio is enabled.
    pub fn wireless_enabled(&self) -> Option<bool> {
        let manager = self.interface_props(super::NM_PATH, interface::NETWORK_MANAGER)?;
        prop(manager, "WirelessEnabled")
    }

    /// WiFi devices, in the order NetworkManager lists them so the adapter
    /// table keeps a stable row order between refreshes.
    pub fn wifi_devices(&self) -> Vec<OwnedObjectPath> {
        let Some(manager) = self.interface_props(super::NM_PATH, interface::NETWORK_MANAGER) else {
            return Vec::new();
        };
        let devices: Vec<OwnedObjectPath> = prop(manager, "Devices").unwrap_or_default();

        devices
            .into_iter()
            .filter(|path| {
                self.device_props(path)
                    .and_then(|props| prop::<u32>(props, "DeviceType"))
                    == Some(WIFI_DEVICE_TYPE)
            })
            .collect()
    }

    /// A device's kernel interface name, e.g. `wlan0`.
    pub fn device_interface(&self, device_path: &str) -> Option<String> {
        prop(
            self.interface_props(device_path, interface::DEVICE)?,
            "Interface",
        )
    }

    /// A device's current NetworkManager state.
    pub fn device_state(&self, device_path: &str) -> Option<DeviceState> {
        let props = self.interface_props(device_path, interface::DEVICE)?;
        prop::<u32>(props, "State").map(DeviceState::from)
    }

    fn device_props(&self, path: &OwnedObjectPath) -> Option<&HashMap<String, OwnedValue>> {
        self.objects.get(path)?.get(interface::DEVICE)
    }

    /// Access points the device can currently see: deduplicated by SSID keeping
    /// the strongest signal, then sorted strongest first.
    pub fn visible_networks(&self, device_path: &str) -> Vec<AccessPointInfo> {
        let Some(device) = self.interface_props(device_path, interface::DEVICE_WIRELESS) else {
            return Vec::new();
        };
        let ap_paths: Vec<OwnedObjectPath> = prop(device, "AccessPoints").unwrap_or_default();

        let mut networks: Vec<AccessPointInfo> = Vec::new();
        for ap_path in ap_paths {
            let Some(ap) = self.access_point(&ap_path) else {
                continue;
            };
            // A hidden network is exported with an empty SSID until one is
            // observed, and has nothing to show in a network list.
            if ap.ssid.is_empty() {
                continue;
            }

            match networks.iter_mut().find(|known| known.ssid == ap.ssid) {
                Some(weaker) if ap.strength > weaker.strength => *weaker = ap,
                Some(_) => {}
                None => networks.push(ap),
            }
        }

        networks.sort_by_key(|n| std::cmp::Reverse(n.strength));
        networks
    }

    /// The access point the device is currently associated with.
    pub fn active_access_point(&self, device_path: &str) -> Option<AccessPointInfo> {
        let device = self.interface_props(device_path, interface::DEVICE_WIRELESS)?;
        let ap_path: OwnedObjectPath = prop(device, "ActiveAccessPoint")?;

        // NetworkManager reports "no active access point" as the root path.
        if ap_path.as_str() == "/" {
            return None;
        }
        self.access_point(&ap_path)
    }

    /// Every connection NetworkManager currently reports as active, in no
    /// particular order.
    pub fn active_connections(&self) -> Vec<ActiveConnectionInfo> {
        self.objects
            .iter()
            .filter_map(|(path, interfaces)| {
                let props = interfaces.get(interface::CONNECTION_ACTIVE)?;
                Some(active_connection_info(path.as_str(), props))
            })
            .collect()
    }

    /// The connection carrying the default route — the link actually reaching
    /// the internet — when NetworkManager reports one.
    pub fn primary_connection(&self) -> Option<ActiveConnectionInfo> {
        let manager = self.interface_props(super::NM_PATH, interface::NETWORK_MANAGER)?;
        let path: OwnedObjectPath = prop(manager, "PrimaryConnection")?;
        if path.as_str() == "/" {
            return None;
        }

        let props = self.objects.get(&path)?.get(interface::CONNECTION_ACTIVE)?;
        Some(active_connection_info(path.as_str(), props))
    }

    /// `(path, VersionId)` for every saved profile, sorted so it can be compared
    /// as a whole. NetworkManager bumps `VersionId` on every edit, so an equal
    /// pair of these means no profile was added, removed, or changed.
    ///
    /// An unreadable version stays `None` rather than defaulting: this is a
    /// cache key, and a version that silently reads as a real one would pin the
    /// profile to a stale entry for the rest of the process.
    pub(crate) fn connection_versions(&self) -> Vec<(String, Option<u64>)> {
        let mut versions: Vec<(String, Option<u64>)> = self
            .objects
            .iter()
            .filter_map(|(path, interfaces)| {
                let props = interfaces.get(interface::SETTINGS_CONNECTION)?;
                Some((path.as_str().to_string(), prop(props, "VersionId")))
            })
            .collect();

        versions.sort();
        versions
    }

    fn access_point(&self, ap_path: &OwnedObjectPath) -> Option<AccessPointInfo> {
        let props = self.objects.get(ap_path)?.get(interface::ACCESS_POINT)?;
        Some(access_point_info(ap_path.as_str(), props))
    }

    fn interface_props(&self, path: &str, interface: &str) -> Option<&HashMap<String, OwnedValue>> {
        let object_path = OwnedObjectPath::try_from(path.to_string()).ok()?;
        self.objects.get(&object_path)?.get(interface)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zbus::zvariant::Value;

    fn value<'a>(v: impl Into<Value<'a>>) -> OwnedValue {
        OwnedValue::try_from(v.into()).expect("value is convertible")
    }

    fn object_path(path: &str) -> OwnedObjectPath {
        OwnedObjectPath::try_from(path.to_string()).expect("valid object path")
    }

    fn access_point_props(ssid: &str, strength: u8) -> HashMap<String, OwnedValue> {
        HashMap::from([
            ("Ssid".to_string(), value(ssid.as_bytes().to_vec())),
            ("Strength".to_string(), value(strength)),
            ("Frequency".to_string(), value(2437u32)),
            ("HwAddress".to_string(), value("00:11:22:33:44:55")),
            ("Flags".to_string(), value(0u32)),
            ("WpaFlags".to_string(), value(0u32)),
            ("RsnFlags".to_string(), value(0u32)),
            ("Mode".to_string(), value(2u32)),
        ])
    }

    fn snapshot_of(objects: Vec<(&str, &str, HashMap<String, OwnedValue>)>) -> NmSnapshot {
        let mut map: ManagedObjects = HashMap::new();
        for (path, interface, props) in objects {
            map.entry(object_path(path))
                .or_default()
                .insert(interface.to_string(), props);
        }
        NmSnapshot { objects: map }
    }

    #[test]
    fn access_point_info_reads_properties() {
        let info = access_point_info("/ap/1", &access_point_props("home", 71));

        assert_eq!(info.path, "/ap/1");
        assert_eq!(info.ssid, "home");
        assert_eq!(info.strength, 71);
        assert_eq!(info.frequency, 2437);
        assert_eq!(info.hw_address, "00:11:22:33:44:55");
    }

    /// A snapshot can race an access point out of existence, so a partial
    /// property map must degrade rather than panic or fail the refresh.
    #[test]
    fn access_point_info_defaults_absent_properties() {
        let info = access_point_info("/ap/1", &HashMap::new());

        assert_eq!(info.ssid, "");
        assert_eq!(info.strength, 0);
        assert_eq!(info.frequency, 0);
        assert_eq!(info.hw_address, "");
    }

    #[test]
    fn visible_networks_keeps_strongest_of_duplicate_ssids() {
        let snapshot = snapshot_of(vec![
            (
                "/dev/1",
                interface::DEVICE_WIRELESS,
                HashMap::from([(
                    "AccessPoints".to_string(),
                    value(vec![object_path("/ap/1"), object_path("/ap/2")]),
                )]),
            ),
            (
                "/ap/1",
                interface::ACCESS_POINT,
                access_point_props("home", 40),
            ),
            (
                "/ap/2",
                interface::ACCESS_POINT,
                access_point_props("home", 90),
            ),
        ]);

        let networks = snapshot.visible_networks("/dev/1");

        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].strength, 90);
    }

    #[test]
    fn visible_networks_skips_empty_ssids_and_sorts_by_strength() {
        let snapshot = snapshot_of(vec![
            (
                "/dev/1",
                interface::DEVICE_WIRELESS,
                HashMap::from([(
                    "AccessPoints".to_string(),
                    value(vec![
                        object_path("/ap/1"),
                        object_path("/ap/2"),
                        object_path("/ap/3"),
                    ]),
                )]),
            ),
            (
                "/ap/1",
                interface::ACCESS_POINT,
                access_point_props("weak", 20),
            ),
            ("/ap/2", interface::ACCESS_POINT, access_point_props("", 99)),
            (
                "/ap/3",
                interface::ACCESS_POINT,
                access_point_props("strong", 80),
            ),
        ]);

        let ssids: Vec<String> = snapshot
            .visible_networks("/dev/1")
            .into_iter()
            .map(|n| n.ssid)
            .collect();

        assert_eq!(ssids, vec!["strong", "weak"]);
    }

    #[test]
    fn visible_networks_is_empty_for_unknown_device() {
        let snapshot = snapshot_of(vec![]);
        assert!(snapshot.visible_networks("/dev/missing").is_empty());
    }

    #[test]
    fn active_access_point_is_none_when_unassociated() {
        let snapshot = snapshot_of(vec![(
            "/dev/1",
            interface::DEVICE_WIRELESS,
            HashMap::from([("ActiveAccessPoint".to_string(), value(object_path("/")))]),
        )]);

        assert!(snapshot.active_access_point("/dev/1").is_none());
    }

    #[test]
    fn active_access_point_resolves_through_the_snapshot() {
        let snapshot = snapshot_of(vec![
            (
                "/dev/1",
                interface::DEVICE_WIRELESS,
                HashMap::from([("ActiveAccessPoint".to_string(), value(object_path("/ap/7")))]),
            ),
            (
                "/ap/7",
                interface::ACCESS_POINT,
                access_point_props("home", 55),
            ),
        ]);

        let active = snapshot.active_access_point("/dev/1").expect("resolves");
        assert_eq!(active.ssid, "home");
        assert_eq!(active.strength, 55);
    }

    /// The list is compared as a whole to decide whether saved profiles changed,
    /// so it has to be ordered independently of the map's iteration order.
    #[test]
    fn connection_versions_are_sorted() {
        let snapshot = snapshot_of(vec![
            (
                "/settings/2",
                interface::SETTINGS_CONNECTION,
                HashMap::from([("VersionId".to_string(), value(9u64))]),
            ),
            (
                "/settings/1",
                interface::SETTINGS_CONNECTION,
                HashMap::from([("VersionId".to_string(), value(3u64))]),
            ),
        ]);

        assert_eq!(
            snapshot.connection_versions(),
            vec![
                ("/settings/1".to_string(), Some(3)),
                ("/settings/2".to_string(), Some(9)),
            ]
        );
    }

    /// An unreadable version must stay distinguishable from a real one, since
    /// it is the key that decides whether cached profiles are still valid.
    #[test]
    fn connection_versions_keep_unreadable_versions_unknown() {
        let snapshot = snapshot_of(vec![(
            "/settings/1",
            interface::SETTINGS_CONNECTION,
            HashMap::new(),
        )]);

        assert_eq!(
            snapshot.connection_versions(),
            vec![("/settings/1".to_string(), None)]
        );
    }

    #[test]
    fn connection_versions_ignore_non_profile_objects() {
        let snapshot = snapshot_of(vec![(
            "/ap/1",
            interface::ACCESS_POINT,
            access_point_props("home", 50),
        )]);

        assert!(snapshot.connection_versions().is_empty());
    }

    /// Adapter rows are diffed positionally each refresh, so this must follow
    /// NetworkManager's own ordering rather than map iteration order.
    #[test]
    fn wifi_devices_keep_manager_order_and_skip_other_types() {
        let device = |kind: u32| HashMap::from([("DeviceType".to_string(), value(kind))]);
        let snapshot = snapshot_of(vec![
            (
                "/org/freedesktop/NetworkManager",
                interface::NETWORK_MANAGER,
                HashMap::from([(
                    "Devices".to_string(),
                    value(vec![
                        object_path("/dev/3"),
                        object_path("/dev/1"),
                        object_path("/dev/2"),
                    ]),
                )]),
            ),
            ("/dev/1", interface::DEVICE, device(WIFI_DEVICE_TYPE)),
            ("/dev/2", interface::DEVICE, device(1)),
            ("/dev/3", interface::DEVICE, device(WIFI_DEVICE_TYPE)),
        ]);

        let paths: Vec<String> = snapshot
            .wifi_devices()
            .iter()
            .map(|p| p.as_str().to_string())
            .collect();

        assert_eq!(paths, vec!["/dev/3", "/dev/1"]);
    }

    #[test]
    fn device_state_and_interface_read_from_the_device_interface() {
        let snapshot = snapshot_of(vec![(
            "/dev/1",
            interface::DEVICE,
            HashMap::from([
                ("Interface".to_string(), value("wlan0")),
                ("State".to_string(), value(100u32)),
            ]),
        )]);

        assert_eq!(
            snapshot.device_interface("/dev/1").as_deref(),
            Some("wlan0")
        );
        assert_eq!(
            snapshot.device_state("/dev/1"),
            Some(DeviceState::Activated)
        );
        assert!(snapshot.device_state("/dev/missing").is_none());
    }

    #[test]
    fn active_connection_info_defaults_absent_properties() {
        let info = active_connection_info("/active/1", &HashMap::new());

        assert_eq!(info.path, "/active/1");
        assert_eq!(info.id, "");
        assert_eq!(info.connection_type, "");
        assert!(info.devices.is_empty());
    }

    #[test]
    fn active_connection_info_reads_devices() {
        let props = HashMap::from([
            ("Id".to_string(), value("wired")),
            ("Type".to_string(), value("802-3-ethernet")),
            ("Devices".to_string(), value(vec![object_path("/dev/2")])),
        ]);

        let info = active_connection_info("/active/1", &props);

        assert_eq!(info.id, "wired");
        assert_eq!(info.connection_type, "802-3-ethernet");
        assert_eq!(info.devices, vec!["/dev/2".to_string()]);
    }
}
