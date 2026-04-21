use std::env;

use anyhow::{Context, Result};
use tokio::process::Command;

/// Launch the user's preferred browser pointing at `url`.
///
/// Resolution order:
///  1. `$BROWSER` (if set and non-empty)
///  2. `xdg-open` (standard Linux fallback)
///
/// The child process is spawned detached — we neither wait for it to exit nor
/// pipe its stdio, so the TUI is unaffected.
pub async fn launch(url: &str) -> Result<()> {
    let (program, args) = match env::var("BROWSER") {
        Ok(b) if !b.is_empty() => (b, vec![url.to_string()]),
        _ => ("xdg-open".to_string(), vec![url.to_string()]),
    };

    Command::new(&program)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("Failed to launch browser `{}`", program))?;

    Ok(())
}
