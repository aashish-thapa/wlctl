use async_trait::async_trait;

use crate::doctor::check::{DiagnosticCheck, Outcome};
use crate::doctor::context::DoctorContext;

/// Looks up the currently-associated access point and reports its SSID.
pub struct AssociationCheck;

#[async_trait]
impl DiagnosticCheck for AssociationCheck {
    fn name(&self) -> &'static str {
        "association"
    }

    async fn run(&self, ctx: &DoctorContext) -> Outcome {
        let ap_path = match ctx.nm.get_active_access_point(&ctx.device_path).await {
            Ok(Some(p)) => p,
            Ok(None) => {
                return Outcome::warn("not associated with any AP");
            }
            Err(e) => return Outcome::skip(format!("could not read active AP: {}", e)),
        };

        match ctx.nm.get_access_point_info(ap_path.as_str()).await {
            Ok(info) => {
                let ssid = if info.ssid.is_empty() {
                    "<hidden>"
                } else {
                    info.ssid.as_str()
                };
                Outcome::ok(format!("{} ({}% signal)", ssid, info.strength))
            }
            Err(_) => Outcome::ok("associated (AP details unreadable)"),
        }
    }
}
