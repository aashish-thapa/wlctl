use async_trait::async_trait;

use crate::doctor::check::{DiagnosticCheck, Outcome};
use crate::doctor::context::DoctorContext;

/// Reports the active IPv4 address acquired via DHCP or static config.
pub struct IpAddressCheck;

#[async_trait]
impl DiagnosticCheck for IpAddressCheck {
    fn name(&self) -> &'static str {
        "ip address"
    }

    async fn run(&self, ctx: &DoctorContext) -> Outcome {
        match ctx.nm.get_ip4_info(&ctx.device_path).await {
            Ok(Some(info)) if !info.addresses.is_empty() => {
                let (addr, prefix) = &info.addresses[0];
                Outcome::ok(format!("{}/{}", addr, prefix))
            }
            Ok(_) => Outcome::fail(
                "no IPv4 address",
                "DHCP did not assign an address. Try reconnecting, or check the router's DHCP pool.",
            ),
            Err(e) => Outcome::skip(format!("could not read IP config: {}", e)),
        }
    }
}
