//! VPN connections modal — lists saved VPN / WireGuard profiles and toggles
//! them on or off, mirroring `nmtui`'s connection list for the common case of
//! pre-configured tunnels with saved credentials. Rendering lives in `render`;
//! this module owns the modal state and NM orchestration.

mod render;

pub use render::render_modal;

use std::collections::HashMap;

use anyhow::Result;

use crate::nm::{ActiveConnectionState, NMClient, VpnConnectionInfo};

/// A saved VPN profile paired with its live activation state.
#[derive(Debug, Clone)]
pub struct VpnEntry {
    pub info: VpnConnectionInfo,
    /// Active-connection object path when the tunnel is up; used to deactivate.
    pub active_path: Option<String>,
    pub state: ActiveConnectionState,
}

impl VpnEntry {
    /// True while the tunnel is up or coming up — i.e. the toggle action should
    /// bring it *down*.
    pub fn is_active(&self) -> bool {
        matches!(
            self.state,
            ActiveConnectionState::Activated | ActiveConnectionState::Activating
        )
    }
}

/// Interactive modal state: the profile list plus a selection cursor.
pub struct VpnModal {
    pub entries: Vec<VpnEntry>,
    pub selected: usize,
}

impl VpnModal {
    /// Lists saved VPN profiles and resolves which are currently active.
    pub async fn load(nm: &NMClient) -> Result<Self> {
        Ok(Self {
            entries: list_entries(nm).await?,
            selected: 0,
        })
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Moves the cursor, wrapping around. No-op on an empty list.
    pub fn move_selection(&mut self, delta: isize) {
        let len = self.entries.len();
        if len == 0 {
            return;
        }
        self.selected = (self.selected as isize + delta).rem_euclid(len as isize) as usize;
    }

    pub fn selected_entry(&self) -> Option<&VpnEntry> {
        self.entries.get(self.selected)
    }

    /// Re-fetches profiles and active state, clamping the cursor if the list
    /// shrank. Profiles are sorted by name, so the cursor stays meaningful.
    pub async fn refresh(&mut self, nm: &NMClient) -> Result<()> {
        self.entries = list_entries(nm).await?;
        self.selected = self.selected.min(self.entries.len().saturating_sub(1));
        Ok(())
    }
}

/// Activates the entry if it is down, deactivates it if up. VPNs attach to no
/// specific device, so activation passes the null device path (`/`) and lets
/// NetworkManager pick.
pub async fn toggle(nm: &NMClient, entry: &VpnEntry) -> Result<()> {
    match &entry.active_path {
        Some(active) => nm.deactivate_connection(active).await,
        None => {
            nm.activate_connection(&entry.info.path, "/").await?;
            Ok(())
        }
    }
}

/// Joins saved VPN profiles with the set of active connections so each entry
/// carries its current state and (when up) its active-connection path.
async fn list_entries(nm: &NMClient) -> Result<Vec<VpnEntry>> {
    let profiles = nm.get_vpn_connections().await?;

    // Map saved-profile path -> (active-connection path, state) for what's up.
    let mut active_by_profile: HashMap<String, (String, ActiveConnectionState)> = HashMap::new();
    for active_path in nm.get_active_connections().await? {
        if let Ok(info) = nm.get_active_connection_info(active_path.as_str()).await {
            active_by_profile.insert(info.connection_path, (info.path, info.state));
        }
    }

    Ok(profiles
        .into_iter()
        .map(|info| match active_by_profile.get(&info.path) {
            Some((active_path, state)) => VpnEntry {
                info,
                active_path: Some(active_path.clone()),
                state: *state,
            },
            None => VpnEntry {
                info,
                active_path: None,
                state: ActiveConnectionState::Deactivated,
            },
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::nm::{VpnConnectionInfo, VpnKind};

    fn entry(id: &str, state: ActiveConnectionState) -> VpnEntry {
        let active_path = match state {
            ActiveConnectionState::Deactivated => None,
            _ => Some(format!("/active/{id}")),
        };
        VpnEntry {
            info: VpnConnectionInfo {
                path: format!("/conn/{id}"),
                id: id.to_string(),
                uuid: id.to_string(),
                kind: VpnKind::Vpn,
            },
            active_path,
            state,
        }
    }

    fn modal(states: &[ActiveConnectionState]) -> VpnModal {
        VpnModal {
            entries: states.iter().map(|s| entry("vpn", *s)).collect(),
            selected: 0,
        }
    }

    #[test]
    fn is_active_covers_up_and_coming_up() {
        assert!(entry("a", ActiveConnectionState::Activated).is_active());
        assert!(entry("a", ActiveConnectionState::Activating).is_active());
        assert!(!entry("a", ActiveConnectionState::Deactivated).is_active());
        assert!(!entry("a", ActiveConnectionState::Deactivating).is_active());
    }

    #[test]
    fn move_selection_wraps_both_directions() {
        let mut m = modal(&[ActiveConnectionState::Deactivated; 3]);
        m.move_selection(1);
        assert_eq!(m.selected, 1);
        m.move_selection(-1);
        m.move_selection(-1);
        assert_eq!(m.selected, 2, "wraps below zero to the last entry");
        m.move_selection(1);
        assert_eq!(m.selected, 0, "wraps past the end to the first entry");
    }

    #[test]
    fn move_selection_is_noop_when_empty() {
        let mut m = modal(&[]);
        m.move_selection(1);
        assert_eq!(m.selected, 0);
        assert!(m.selected_entry().is_none());
    }
}
