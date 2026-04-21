use async_trait::async_trait;
use tokio::fs;

use crate::doctor::check::{DiagnosticCheck, Outcome};
use crate::doctor::context::DoctorContext;

/// Reads /sys/class/net/<iface>/device/driver to confirm a kernel driver is
/// bound to the wireless interface.
pub struct DriverCheck;

#[async_trait]
impl DiagnosticCheck for DriverCheck {
    fn name(&self) -> &'static str {
        "driver"
    }

    async fn run(&self, ctx: &DoctorContext) -> Outcome {
        let link = format!("/sys/class/net/{}/device/driver", ctx.interface);

        match fs::read_link(&link).await {
            Ok(target) => {
                let name = target
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "unknown".into());
                Outcome::ok(format!("{} loaded", name))
            }
            Err(_) => Outcome::fail(
                "no driver bound",
                format!(
                    "No kernel driver for {}. Check `dmesg | grep -i {}` for firmware errors.",
                    ctx.interface, ctx.interface
                ),
            ),
        }
    }
}
