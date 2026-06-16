//! VPN connections modal — lists saved VPN / WireGuard profiles and toggles
//! them on or off, mirroring `nmtui`'s connection list for the common case of
//! pre-configured tunnels with saved credentials. Rendering lives in `render`;
//! this module owns the modal state and NM orchestration.

mod render;
mod wg;

pub use render::render_modal;

use std::collections::{HashMap, HashSet};

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

/// A transient sub-mode layered over the list. Modeled as one enum so the modal
/// can never be in two prompts at once, and so the delete target is captured up
/// front (immune to the list reordering under a background refresh).
pub enum VpnPrompt {
    /// Awaiting y/n to delete the named profile at `path`.
    ConfirmDelete { path: String, id: String },
    /// Capturing import input: a pasted WireGuard config or a `.conf` path.
    Import(String),
}

/// Interactive modal state: the profile list, a selection cursor, and an
/// optional active prompt (delete confirmation or import input).
pub struct VpnModal {
    pub entries: Vec<VpnEntry>,
    pub selected: usize,
    pub prompt: Option<VpnPrompt>,
}

impl VpnModal {
    /// Lists saved VPN profiles and resolves which are currently active.
    pub async fn load(nm: &NMClient) -> Result<Self> {
        Ok(Self {
            entries: list_entries(nm).await?,
            selected: 0,
            prompt: None,
        })
    }

    /// The import buffer, when the import prompt is active.
    pub fn import_buffer(&self) -> Option<&str> {
        match &self.prompt {
            Some(VpnPrompt::Import(buf)) => Some(buf),
            _ => None,
        }
    }

    /// The profile pending deletion, when the confirm prompt is active.
    pub fn pending_delete(&self) -> Option<&str> {
        match &self.prompt {
            Some(VpnPrompt::ConfirmDelete { id, .. }) => Some(id),
            _ => None,
        }
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

    /// Arms a delete confirmation for the selected profile, capturing its path
    /// so the eventual delete targets that profile even if the list reorders.
    /// No-op on an empty list.
    pub fn begin_delete(&mut self) {
        if let Some(entry) = self.selected_entry() {
            self.prompt = Some(VpnPrompt::ConfirmDelete {
                path: entry.info.path.clone(),
                id: entry.info.id.clone(),
            });
        }
    }

    /// Opens the import prompt with an empty buffer.
    pub fn begin_import(&mut self) {
        self.prompt = Some(VpnPrompt::Import(String::new()));
    }

    /// Dismisses any active prompt.
    pub fn cancel_prompt(&mut self) {
        self.prompt = None;
    }

    /// Appends text to the import buffer (used for pasted input and typed keys).
    pub fn import_append(&mut self, text: &str) {
        if let Some(VpnPrompt::Import(buf)) = &mut self.prompt {
            buf.push_str(text);
        }
    }

    /// Removes the last character from the import buffer.
    pub fn import_backspace(&mut self) {
        if let Some(VpnPrompt::Import(buf)) = &mut self.prompt {
            buf.pop();
        }
    }

    /// Closes the import prompt and returns the captured buffer, if any.
    pub fn take_import(&mut self) -> Option<String> {
        match self.prompt.take() {
            Some(VpnPrompt::Import(buf)) => Some(buf),
            other => {
                self.prompt = other;
                None
            }
        }
    }

    /// Deletes the profile captured by the active confirm prompt. Returns the
    /// removed profile's name. No-op when no delete is pending.
    pub async fn delete_confirmed(&mut self, nm: &NMClient) -> Result<Option<String>> {
        let Some(VpnPrompt::ConfirmDelete { path, id }) = self.prompt.take() else {
            return Ok(None);
        };
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
    let text = tokio::fs::read_to_string(&expanded)
        .await
        .with_context(|| format!("reading {expanded}"))?;
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

/// Creates the profile under a display name and interface name that don't
/// collide with existing saved profiles. A single fetch backs both dedup checks.
async fn create_profile(nm: &NMClient, base_name: &str, cfg: &WgConfig) -> Result<String> {
    let existing = nm.get_vpn_connections().await?;
    let ids: HashSet<&str> = existing.iter().map(|v| v.id.as_str()).collect();
    let ifaces: HashSet<&str> = existing
        .iter()
        .map(|v| v.interface_name.as_str())
        .filter(|s| !s.is_empty())
        .collect();

    let id = dedupe(&ids, base_name);
    let interface = unique_interface(&ifaces, &sanitize_ifname(base_name));
    nm.add_wireguard_connection(&id, &interface, cfg).await?;
    Ok(id)
}

/// Returns `base` if free in `taken`, otherwise appends `-2`, `-3`, … until the
/// name is unique.
fn dedupe(taken: &HashSet<&str>, base: &str) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let candidate = format!("{base}-{n}");
        if !taken.contains(candidate.as_str()) {
            return candidate;
        }
        n += 1;
    }
}

/// Like [`dedupe`] but keeps the result a valid interface name (≤15 chars) by
/// truncating the base to make room for the numeric suffix.
fn unique_interface(taken: &HashSet<&str>, base: &str) -> String {
    if !taken.contains(base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let suffix = format!("-{n}");
        let keep = 15usize.saturating_sub(suffix.len());
        let head: String = base.chars().take(keep).collect();
        let candidate = format!("{}{suffix}", head.trim_end_matches('-'));
        if !taken.contains(candidate.as_str()) {
            return candidate;
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
                interface_name: String::new(),
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
            prompt: None,
        }
    }

    fn set(items: &[&'static str]) -> HashSet<&'static str> {
        items.iter().copied().collect()
    }

    #[test]
    fn dedupe_appends_suffix_until_free() {
        assert_eq!(dedupe(&set(&[]), "wg-home"), "wg-home");
        assert_eq!(dedupe(&set(&["wg-home"]), "wg-home"), "wg-home-2");
        assert_eq!(
            dedupe(&set(&["wg-home", "wg-home-2"]), "wg-home"),
            "wg-home-3"
        );
    }

    #[test]
    fn unique_interface_stays_within_15_chars() {
        // Free name is returned as-is.
        assert_eq!(unique_interface(&set(&[]), "proton-nl"), "proton-nl");
        // Collisions get a suffix while staying a valid (<=15) interface name.
        let taken = set(&["wg-185-185-50-2"]);
        let out = unique_interface(&taken, "wg-185-185-50-2");
        assert_ne!(out, "wg-185-185-50-2");
        assert!(out.len() <= 15, "got {out:?}");
    }

    #[test]
    fn prompt_helpers_model_one_mode_at_a_time() {
        let mut m = modal(&[ActiveConnectionState::Deactivated]);
        assert!(m.import_buffer().is_none() && m.pending_delete().is_none());

        m.begin_import();
        m.import_append("~/wg.conf");
        assert_eq!(m.import_buffer(), Some("~/wg.conf"));
        assert!(m.pending_delete().is_none());

        // Opening the delete confirm replaces the import prompt (no dual state).
        m.begin_delete();
        assert!(m.import_buffer().is_none());
        assert_eq!(m.pending_delete(), Some("vpn"));

        m.cancel_prompt();
        assert!(m.pending_delete().is_none());
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
