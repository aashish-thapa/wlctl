use async_trait::async_trait;

use crate::doctor::check::{DiagnosticCheck, Outcome};
use crate::doctor::context::DoctorContext;
use crate::nm::Connectivity;

/// Asks NetworkManager to re-run its connectivity probe and reports the result.
pub struct PortalCheck;

#[async_trait]
impl DiagnosticCheck for PortalCheck {
    fn name(&self) -> &'static str {
        "connectivity"
    }

    async fn run(&self, ctx: &DoctorContext) -> Outcome {
        match ctx.nm.check_connectivity().await {
            Ok(Connectivity::Full) => Outcome::ok("full internet access"),
            Ok(Connectivity::Portal) => Outcome::fail(
                "captive portal detected",
                "Open a browser and complete the login. (See issue #61 for auto-login.)",
            ),
            Ok(Connectivity::Limited) => Outcome::warn("limited — network reachable, no internet"),
            Ok(Connectivity::None) => Outcome::warn("no connectivity"),
            Ok(Connectivity::Unknown) => Outcome::skip("connectivity state unknown"),
            Err(e) => Outcome::skip(format!("could not check connectivity: {}", e)),
        }
    }
}
