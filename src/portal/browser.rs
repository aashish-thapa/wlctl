use std::env;

use anyhow::{Context, Result};
use tokio::process::Command;

/// Fallback when `$BROWSER` is unset, empty, or contains no usable candidate.
/// `xdg-open` itself handles desktop-environment-aware selection.
const DEFAULT_BROWSER: &str = "xdg-open";

/// Launch the user's preferred browser pointing at `url`.
///
/// Resolution order:
///  1. `$BROWSER` (xdg-utils convention — colon-separated list of candidates,
///     each of which may carry its own args; we take the first non-empty
///     candidate and delegate further fallback to the system)
///  2. `xdg-open` when the env var is unset, empty, or all-whitespace
///
/// The child process is spawned detached — we neither wait for it to exit nor
/// pipe its stdio, so the TUI is unaffected.
pub async fn launch(url: &str) -> Result<()> {
    let (program, mut args) = resolve_command(env::var("BROWSER").ok());
    args.push(url.to_string());

    Command::new(&program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to launch browser `{}`", program))?;

    Ok(())
}

/// Parse the value of `$BROWSER` into a program path plus any leading args,
/// honouring the xdg-utils convention: colons separate fallback candidates,
/// whitespace within a candidate separates program from args.
///
/// Pure function so it's unit-testable without touching the process env.
fn resolve_command(browser_env: Option<String>) -> (String, Vec<String>) {
    let candidate = browser_env
        .as_deref()
        .and_then(|raw| raw.split(':').map(str::trim).find(|s| !s.is_empty()));

    let Some(cmd) = candidate else {
        return (DEFAULT_BROWSER.to_string(), Vec::new());
    };

    let mut parts = cmd.split_whitespace();
    // Invariant: `cmd` is the result of `find(|s| !s.is_empty())` on trimmed
    // slices, so at least one whitespace-separated token exists.
    let program = parts
        .next()
        .expect("candidate survived non-empty filter")
        .to_string();
    let args = parts.map(str::to_string).collect();
    (program, args)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unset_falls_back_to_xdg_open() {
        assert_eq!(resolve_command(None), ("xdg-open".into(), vec![]));
    }

    #[test]
    fn empty_or_whitespace_falls_back() {
        assert_eq!(
            resolve_command(Some("".into())),
            ("xdg-open".into(), vec![])
        );
        assert_eq!(
            resolve_command(Some("   ".into())),
            ("xdg-open".into(), vec![])
        );
    }

    #[test]
    fn plain_program_has_no_args() {
        assert_eq!(
            resolve_command(Some("firefox".into())),
            ("firefox".into(), vec![])
        );
    }

    #[test]
    fn whitespace_separates_program_from_args() {
        assert_eq!(
            resolve_command(Some("firefox --private-window".into())),
            ("firefox".into(), vec!["--private-window".into()])
        );
    }

    #[test]
    fn colon_picks_first_candidate() {
        assert_eq!(
            resolve_command(Some("firefox:chromium".into())),
            ("firefox".into(), vec![])
        );
        assert_eq!(
            resolve_command(Some("firefox --new-tab:chromium".into())),
            ("firefox".into(), vec!["--new-tab".into()])
        );
    }

    #[test]
    fn colon_skips_empty_candidates() {
        assert_eq!(
            resolve_command(Some(":firefox".into())),
            ("firefox".into(), vec![])
        );
        assert_eq!(
            resolve_command(Some("  :  :firefox".into())),
            ("firefox".into(), vec![])
        );
        assert_eq!(
            resolve_command(Some("::".into())),
            (DEFAULT_BROWSER.into(), vec![])
        );
    }
}
