use std::time::Duration;

use async_trait::async_trait;
use tokio::net::TcpStream;
use tokio::time::timeout;

use crate::doctor::check::{DiagnosticCheck, Outcome};
use crate::doctor::context::DoctorContext;

const INTERNET_HOSTS: &[&str] = &["1.1.1.1:443", "8.8.8.8:443"];
const INTERNET_TIMEOUT: Duration = Duration::from_secs(3);

/// TCP-connects to well-known public endpoints to confirm end-to-end reachability.
pub struct InternetCheck;

#[async_trait]
impl DiagnosticCheck for InternetCheck {
    fn name(&self) -> &'static str {
        "internet"
    }

    async fn run(&self, _ctx: &DoctorContext) -> Outcome {
        for host in INTERNET_HOSTS {
            if let Ok(Ok(_)) = timeout(INTERNET_TIMEOUT, TcpStream::connect(host)).await {
                return Outcome::ok(format!("reachable via {}", host));
            }
        }

        Outcome::fail(
            "no public endpoint reachable",
            "Internet is unreachable. Possible causes: captive portal, ISP outage, firewall.",
        )
    }
}
