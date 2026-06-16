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
    /// Assigned IPv4 (`addr/prefix`) while the tunnel is up; `None` otherwise.
    pub ipv4: Option<String>,
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

    /// Human-readable uptime ("up 14m", "up 2h 3m") derived from the profile's
    /// last-activation timestamp. `None` when down or the timestamp is missing
    /// or in the future (clock skew / never activated).
    pub fn uptime(&self) -> Option<String> {
        if !self.is_active() || self.info.timestamp == 0 {
            return None;
        }
        let now = chrono::Local::now().timestamp();
        let elapsed = now.checked_sub(self.info.timestamp as i64)?;
        if elapsed < 0 {
            return None;
        }
        Some(format!("up {}", format_duration(elapsed as u64)))
    }
}

/// Formats a span of seconds compactly: "45s", "14m", "2h 3m", "1d 4h".
fn format_duration(secs: u64) -> String {
    let (d, h, m, s) = (secs / 86_400, secs / 3_600 % 24, secs / 60 % 60, secs % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m")
    } else {
        format!("{s}s")
    }
}

/// Interactive modal state: the profile list plus a selection cursor.
pub struct VpnModal {
    pub entries: Vec<VpnEntry>,
    pub selected: usize,
    /// When set, a delete confirmation is pending for the selected entry; the
    /// hint line shows the prompt and only y/n are honored until resolved.
    pub pending_delete: bool,
}

impl VpnModal {
    /// Lists saved VPN profiles and resolves which are currently active.
    pub async fn load(nm: &NMClient) -> Result<Self> {
        Ok(Self {
            entries: list_entries(nm).await?,
            selected: 0,
            pending_delete: false,
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

    /// Flips autoconnect on the selected profile and re-reads state so the UI
    /// reflects the change. No-op on an empty list.
    pub async fn toggle_autoconnect(&mut self, nm: &NMClient) -> Result<Option<(String, bool)>> {
        let Some(entry) = self.selected_entry() else {
            return Ok(None);
        };
        let next = !entry.info.autoconnect;
        let (path, id) = (entry.info.path.clone(), entry.info.id.clone());
        nm.set_connection_autoconnect(&path, next).await?;
        self.refresh(nm).await?;
        Ok(Some((id, next)))
    }

    /// Deletes the selected profile, clearing the pending-confirm flag. Returns
    /// the removed profile's name. No-op on an empty list.
    pub async fn delete_selected(&mut self, nm: &NMClient) -> Result<Option<String>> {
        self.pending_delete = false;
        let Some(entry) = self.selected_entry() else {
            return Ok(None);
        };
        let (path, id) = (entry.info.path.clone(), entry.info.id.clone());
        nm.delete_connection(&path).await?;
        self.refresh(nm).await?;
        Ok(Some(id))
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

    let mut entries = Vec::with_capacity(profiles.len());
    for info in profiles {
        match active_by_profile.get(&info.path) {
            Some((active_path, state)) => {
                // Best-effort: a missing lease shouldn't drop the whole entry.
                let ipv4 = nm.active_connection_ipv4(active_path).await.ok().flatten();
                entries.push(VpnEntry {
                    info,
                    active_path: Some(active_path.clone()),
                    state: *state,
                    ipv4,
                });
            }
            None => entries.push(VpnEntry {
                info,
                active_path: None,
                state: ActiveConnectionState::Deactivated,
                ipv4: None,
            }),
        }
    }

    Ok(entries)
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
                autoconnect: false,
                timestamp: 0,
            },
            active_path,
            state,
            ipv4: None,
        }
    }

    fn modal(states: &[ActiveConnectionState]) -> VpnModal {
        VpnModal {
            entries: states.iter().map(|s| entry("vpn", *s)).collect(),
            selected: 0,
            pending_delete: false,
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
    fn format_duration_picks_coarsest_units() {
        assert_eq!(format_duration(45), "45s");
        assert_eq!(format_duration(14 * 60), "14m");
        assert_eq!(format_duration(2 * 3600 + 3 * 60), "2h 3m");
        assert_eq!(format_duration(86_400 + 4 * 3600), "1d 4h");
    }

    #[test]
    fn uptime_is_none_when_down_or_untimed() {
        assert!(
            entry("a", ActiveConnectionState::Deactivated)
                .uptime()
                .is_none()
        );
        // Active but no timestamp recorded.
        assert!(
            entry("a", ActiveConnectionState::Activated)
                .uptime()
                .is_none()
        );
    }

    #[test]
    fn move_selection_is_noop_when_empty() {
        let mut m = modal(&[]);
        m.move_selection(1);
        assert_eq!(m.selected, 0);
        assert!(m.selected_entry().is_none());
    }
}
