use std::io::ErrorKind;
use std::time::Duration;

use async_trait::async_trait;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::doctor::check::{DiagnosticCheck, Outcome};
use crate::doctor::context::DoctorContext;

const GATEWAY_TIMEOUT: Duration = Duration::from_secs(2);

/// Probes the default gateway with a short TCP SYN to confirm L3 reachability.
pub struct GatewayCheck;

#[async_trait]
impl DiagnosticCheck for GatewayCheck {
    fn name(&self) -> &'static str {
        "gateway"
    }

    async fn run(&self, ctx: &DoctorContext) -> Outcome {
        let gateway = match ctx.nm.get_ip4_info(&ctx.device_path).await {
            Ok(Some(info)) => info.gateway,
            Ok(None) => None,
            Err(e) => return Outcome::skip(format!("could not read IP config: {}", e)),
        };

        let Some(gw) = gateway else {
            return Outcome::warn("no default gateway configured");
        };

        // Port 80 picked arbitrarily; a RST from a closed port still proves
        // the host is up. ICMP would need raw sockets / CAP_NET_RAW.
        let addr = format!("{}:80", gw);
        match timeout(GATEWAY_TIMEOUT, TcpStream::connect(&addr)).await {
            Ok(Ok(_)) => Outcome::ok(format!("{} reachable", gw)),
            Ok(Err(e)) if e.kind() == ErrorKind::ConnectionRefused => {
                Outcome::ok(format!("{} reachable (refused port 80)", gw))
            }
            Ok(Err(e)) => Outcome::fail(
                format!("{} unreachable ({})", gw, e),
                "Router-side issue. Try power-cycling the AP or moving closer.",
            ),
            Err(_) => Outcome::fail(
                format!("{} unreachable (timeout)", gw),
                "Router-side issue. Try power-cycling the AP or moving closer.",
            ),
        }
    }
}
