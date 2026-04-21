use async_trait::async_trait;
use tokio::fs;

use crate::doctor::check::{DiagnosticCheck, Outcome};
use crate::doctor::context::DoctorContext;

/// Reads /sys/class/rfkill to detect soft- or hard-blocked wireless radios.
pub struct RfkillCheck;

#[async_trait]
impl DiagnosticCheck for RfkillCheck {
    fn name(&self) -> &'static str {
        "rfkill"
    }

    async fn run(&self, _ctx: &DoctorContext) -> Outcome {
        let mut entries = match fs::read_dir("/sys/class/rfkill").await {
            Ok(d) => d,
            Err(_) => return Outcome::skip("rfkill sysfs not available"),
        };

        let mut saw_wlan = false;

        while let Ok(Some(entry)) = entries.next_entry().await {
            let path = entry.path();

            let kind = fs::read_to_string(path.join("type"))
                .await
                .unwrap_or_default();
            if kind.trim() != "wlan" {
                continue;
            }
            saw_wlan = true;

            if read_flag(&path, "hard").await {
                return Outcome::fail(
                    "wireless hard-blocked",
                    "Toggle the hardware WiFi switch (laptop kill switch or keyboard function key).",
                );
            }
            if read_flag(&path, "soft").await {
                return Outcome::fail("wireless soft-blocked", "Run: rfkill unblock wlan");
            }
        }

        if saw_wlan {
            Outcome::ok("not blocked")
        } else {
            Outcome::skip("no wlan rfkill entry found")
        }
    }
}

async fn read_flag(dir: &std::path::Path, name: &str) -> bool {
    fs::read_to_string(dir.join(name))
        .await
        .map(|s| s.trim() == "1")
        .unwrap_or(false)
}
