//! VPN connections modal — lists saved VPN / WireGuard profiles and toggles
//! them on or off, mirroring `nmtui`'s connection list for the common case of
//! pre-configured tunnels with saved credentials. Rendering lives in `render`;
//! this module owns the modal state and NM orchestration.

mod render;
mod wg;

pub use render::render_modal;

use std::collections::HashMap;

use anyhow::{Context, Result};

use crate::nm::{ActiveConnectionState, NMClient, VpnConnectionInfo, WgConfig};

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
    /// When set, the modal is capturing import input — either a pasted WireGuard
    /// config or a path to a `.conf` file; the hint line becomes the input field
    /// until Enter/Esc.
    pub import_input: Option<String>,
}

impl VpnModal {
    /// Lists saved VPN profiles and resolves which are currently active.
    pub async fn load(nm: &NMClient) -> Result<Self> {
        Ok(Self {
            entries: list_entries(nm).await?,
            selected: 0,
            pending_delete: false,
            import_input: None,
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

/// Reads a WireGuard `.conf` from `path`, parses it, and creates a matching
/// NetworkManager profile (without activating it). Returns the new profile's
/// display name (derived from the file name). `~` is expanded to `$HOME`.
pub async fn import_from_file(nm: &NMClient, path: &str) -> Result<String> {
    let expanded = expand_tilde(path);
    let text = std::fs::read_to_string(&expanded).with_context(|| format!("reading {expanded}"))?;
    let cfg = wg::parse(&text)?;

    let base = std::path::Path::new(&expanded)
        .file_stem()
        .and_then(|s| s.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or("wireguard");

    create_profile(nm, base, &cfg).await
}

/// Parses pasted WireGuard config text and creates a NetworkManager profile
/// (without activating it). The display name is derived from the peer endpoint
/// since pasted text carries no file name. Returns the new profile's name.
pub async fn import_from_text(nm: &NMClient, text: &str) -> Result<String> {
    let cfg = wg::parse(text)?;
    let base = name_from_config(&cfg);
    create_profile(nm, &base, &cfg).await
}

/// Creates the profile under a name that doesn't collide with an existing one.
async fn create_profile(nm: &NMClient, base_name: &str, cfg: &WgConfig) -> Result<String> {
    let id = dedupe_name(nm, base_name).await?;
    let interface = sanitize_ifname(&id);
    nm.add_wireguard_connection(&id, &interface, cfg).await?;
    Ok(id)
}

/// Returns `base` if no saved VPN profile already uses it, otherwise appends a
/// numeric suffix (`base-2`, `base-3`, …) until the name is free.
async fn dedupe_name(nm: &NMClient, base: &str) -> Result<String> {
    let existing: std::collections::HashSet<String> = nm
        .get_vpn_connections()
        .await?
        .into_iter()
        .map(|v| v.id)
        .collect();
    if !existing.contains(base) {
        return Ok(base.to_string());
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !existing.contains(&candidate) {
            return Ok(candidate);
        }
        n += 1;
    }
}

/// Derives a friendly profile name from the peer endpoint (e.g. `wg-nl247` from
/// a hostname, `wg-185-185-50-27` from an IP). Falls back to `wg-imported`.
fn name_from_config(cfg: &WgConfig) -> String {
    let Some(endpoint) = &cfg.peer.endpoint else {
        return "wg-imported".to_string();
    };

    // Drop the port. Handle both `host:port` and `[v6]:port`.
    let host = if let Some(rest) = endpoint.strip_prefix('[') {
        rest.split(']').next().unwrap_or(rest)
    } else {
        endpoint
            .rsplit_once(':')
            .map(|(h, _)| h)
            .unwrap_or(endpoint)
    }
    .trim();

    if host.is_empty() {
        "wg-imported".to_string()
    } else if host.parse::<std::net::IpAddr>().is_ok() {
        format!("wg-{}", host.replace(['.', ':'], "-"))
    } else {
        let label = host.split('.').next().unwrap_or(host);
        format!("wg-{label}")
    }
}

/// Expands a leading `~` to `$HOME`. Leaves the path untouched otherwise.
fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Ok(home) = std::env::var("HOME")
    {
        return format!("{home}/{rest}");
    }
    path.to_string()
}

/// Derives a valid Linux interface name from a profile name: lowercased, only
/// `[a-z0-9-]`, capped at 15 chars. Falls back to `wg0` if nothing survives.
fn sanitize_ifname(name: &str) -> String {
    let cleaned: String = name
        .to_ascii_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    let trimmed = cleaned.trim_matches('-');
    let capped: String = trimmed.chars().take(15).collect();
    let capped = capped.trim_end_matches('-');
    if capped.is_empty() {
        "wg0".to_string()
    } else {
        capped.to_string()
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
            import_input: None,
        }
    }

    #[test]
    fn sanitize_ifname_keeps_it_valid() {
        assert_eq!(
            sanitize_ifname("proton-nl-NL-FREE-247"),
            "proton-nl-NL-Fr".to_ascii_lowercase()
        );
        assert_eq!(sanitize_ifname("wg-proton"), "wg-proton");
        assert_eq!(sanitize_ifname("US#FREE#55"), "us-free-55");
        assert_eq!(sanitize_ifname("***"), "wg0");
        assert!(sanitize_ifname("a-very-long-name-indeed").len() <= 15);
    }

    #[test]
    fn expand_tilde_uses_home() {
        // Non-tilde paths are returned verbatim.
        assert_eq!(expand_tilde("/etc/wg.conf"), "/etc/wg.conf");
    }

    #[test]
    fn name_from_config_derives_from_endpoint() {
        let make = |endpoint: Option<&str>| WgConfig {
            private_key: "k=".into(),
            addresses: vec![],
            dns: vec![],
            peer: crate::nm::WgPeerConfig {
                public_key: "p=".into(),
                endpoint: endpoint.map(str::to_string),
                allowed_ips: vec![],
                preshared_key: None,
                persistent_keepalive: None,
            },
        };
        assert_eq!(
            name_from_config(&make(Some("185.185.50.27:51820"))),
            "wg-185-185-50-27"
        );
        assert_eq!(
            name_from_config(&make(Some("nl247.example.com:51820"))),
            "wg-nl247"
        );
        assert_eq!(name_from_config(&make(None)), "wg-imported");
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
