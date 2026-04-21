use std::time::Duration;

use async_trait::async_trait;
use tokio::net::lookup_host;
use tokio::time::timeout;

use crate::doctor::check::{DiagnosticCheck, Outcome};
use crate::doctor::context::DoctorContext;

const PROBE_HOST: &str = "one.one.one.one:80";
const DNS_TIMEOUT: Duration = Duration::from_secs(3);

/// Resolves a well-known host to confirm DNS is working.
pub struct DnsCheck;

#[async_trait]
impl DiagnosticCheck for DnsCheck {
    fn name(&self) -> &'static str {
        "dns"
    }

    async fn run(&self, _ctx: &DoctorContext) -> Outcome {
        match timeout(DNS_TIMEOUT, lookup_host(PROBE_HOST)).await {
            Ok(Ok(mut addrs)) => match addrs.next() {
                Some(addr) => Outcome::ok(format!("resolves ({})", addr.ip())),
                None => Outcome::fail(
                    "DNS returned no records",
                    "Resolver reachable but empty response. Check /etc/resolv.conf.",
                ),
            },
            Ok(Err(e)) => Outcome::fail(
                format!("DNS lookup failed: {}", e),
                "Check your DNS servers (nmcli -g IP4.DNS device show) or try overriding with 1.1.1.1.",
            ),
            Err(_) => Outcome::fail(
                format!("DNS lookup timed out after {}s", DNS_TIMEOUT.as_secs()),
                "DNS server is unreachable. Check /etc/resolv.conf or the router's DNS.",
            ),
        }
    }
}
