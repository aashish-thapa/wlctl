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
