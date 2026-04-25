//! Captive-portal detection.
//!
//! A `PortalWatcher` polls NetworkManager's connectivity state each tick and
//! emits an event when a fresh portal is encountered on the current SSID.
//! The watcher itself is passive — it only reports state transitions. The
//! action (notify the user, open a browser) is the caller's responsibility.

mod browser;

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;

use crate::nm::{Connectivity, NMClient};

pub use browser::launch as launch_browser;

/// Emitted by the watcher when a new captive portal is detected — i.e. when
/// the connectivity state transitions from non-`Portal` to `Portal` on an SSID
/// we haven't opened this session yet.
#[derive(Debug, Clone)]
pub struct PortalDetected {
    pub ssid: String,
    pub url: String,
}

pub struct PortalWatcher {
    previous_state: Option<Connectivity>,
    opened_for: HashSet<String>,
}

impl PortalWatcher {
    pub fn new() -> Self {
        Self {
            previous_state: None,
            opened_for: HashSet::new(),
        }
    }

    /// Poll connectivity and report a new portal if we just entered the Portal
    /// state on an SSID we haven't auto-opened for yet.
    ///
    /// State (`previous_state`, `opened_for`) is committed only after all
    /// required lookups succeed. A transient D-Bus error leaves the watcher
    /// eligible to retry on the next tick — otherwise a single failed SSID
    /// lookup would mark the portal transition as already handled and the
    /// auto-open would never fire for that session.
    pub async fn poll(
        &mut self,
        nm: &Arc<NMClient>,
        device_path: &str,
    ) -> Result<Option<PortalDetected>> {
        let state = nm.check_connectivity().await?;

        // Leaving Portal (or never entered): safe to record — re-entry will
        // flow back through the transition branch below.
        if state != Connectivity::Portal {
            self.previous_state = Some(state);
            return Ok(None);
        }

        // Already in Portal on the previous tick — no transition to report.
        if self.previous_state == Some(Connectivity::Portal) {
            return Ok(None);
        }

        // Fresh Portal transition. Gather details before committing any
        // state; transient lookup failures propagate via `?` (caller decides
        // how to react) so the next tick re-evaluates the same transition.
        // `Ok(None)` — no SSID currently associated — is a legitimate no-op,
        // distinct from a D-Bus failure, so we return cleanly without error.
        let Some(ssid) = nm.get_connected_ssid(device_path).await? else {
            return Ok(None);
        };

        if self.opened_for.contains(&ssid) {
            // Already handled this SSID this session; record the Portal state
            // so subsequent ticks short-circuit on the "previous == Portal"
            // guard above instead of re-entering this branch.
            self.previous_state = Some(Connectivity::Portal);
            return Ok(None);
        }

        let url = nm.get_connectivity_check_uri().await?;
        if url.is_empty() {
            return Ok(None);
        }

        // All lookups succeeded — commit the transition.
        self.previous_state = Some(Connectivity::Portal);
        self.opened_for.insert(ssid.clone());

        Ok(Some(PortalDetected { ssid, url }))
    }
}

impl Default for PortalWatcher {
    fn default() -> Self {
        Self::new()
    }
}

/// One-shot detection used by the `wlctl portal` CLI subcommand. Probes
/// connectivity, launches the browser if a portal is detected, and returns
/// a user-facing message.
pub async fn run_once() -> Result<()> {
    use anyhow::Context;

    let nm = Arc::new(
        NMClient::new()
            .await
            .context("Could not reach NetworkManager over D-Bus")?,
    );

    let state = nm.check_connectivity().await?;

    match state {
        Connectivity::Portal => {
            let url = nm.get_connectivity_check_uri().await?;
            if url.is_empty() {
                anyhow::bail!(
                    "NetworkManager reports a captive portal but has no probe URL configured."
                );
            }
            println!(
                "Captive portal detected. Opening {} in your browser...",
                url
            );
            launch_browser(&url).await?;
        }
        Connectivity::Full => println!("No portal — you have full internet access."),
        Connectivity::Limited => println!("Connected but no internet (limited)."),
        Connectivity::None => println!("No connectivity."),
        Connectivity::Unknown => println!("Connectivity state unknown."),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn watcher_with(prev: Connectivity, opened: &[&str]) -> PortalWatcher {
        PortalWatcher {
            previous_state: Some(prev),
            opened_for: opened.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn opened_for_tracks_per_ssid() {
        let mut w = watcher_with(Connectivity::Full, &[]);
        assert!(w.opened_for.insert("work".into()));
        assert!(!w.opened_for.insert("work".into()));
        assert!(w.opened_for.insert("home".into()));
    }

    #[test]
    fn event_construction() {
        let e = PortalDetected {
            ssid: "Hotel-Guest".into(),
            url: "http://example/probe".into(),
        };
        assert_eq!(e.ssid, "Hotel-Guest");
        assert_eq!(e.url, "http://example/probe");
    }
}
