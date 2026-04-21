//! Network diagnostic runner — walks the stack (rfkill → driver → interface →
//! association → DHCP → DNS → gateway → internet → captive portal) and prints
//! an interpreted verdict instead of raw logs.

mod check;
mod checks;
mod context;
mod report;

use std::sync::Arc;

use anyhow::{Context as _, Result};

use crate::nm::NMClient;

use check::{DiagnosticCheck, Outcome};
use checks::{
    AssociationCheck, DeviceStateCheck, DnsCheck, DriverCheck, GatewayCheck, InternetCheck,
    IpAddressCheck, PortalCheck, RfkillCheck,
};
use context::DoctorContext;
use report::Report;

/// Composes the default set of diagnostic checks in stack order.
pub struct Doctor {
    checks: Vec<Box<dyn DiagnosticCheck>>,
}

impl Default for Doctor {
    fn default() -> Self {
        Self {
            checks: vec![
                Box::new(RfkillCheck),
                Box::new(DriverCheck),
                Box::new(DeviceStateCheck),
                Box::new(AssociationCheck),
                Box::new(IpAddressCheck),
                Box::new(DnsCheck),
                Box::new(GatewayCheck),
                Box::new(InternetCheck),
                Box::new(PortalCheck),
            ],
        }
    }
}

impl Doctor {
    /// Runs every check sequentially and returns an ordered list of results.
    pub async fn run(&self, ctx: &DoctorContext) -> Vec<(&'static str, Outcome)> {
        let mut results = Vec::with_capacity(self.checks.len());
        for check in &self.checks {
            let outcome = check.run(ctx).await;
            results.push((check.name(), outcome));
        }
        results
    }
}

/// Entry point invoked by the CLI. Builds a context for the first WiFi device
/// and prints a formatted report to stdout.
pub async fn run() -> Result<()> {
    let nm = Arc::new(
        NMClient::new()
            .await
            .context("Could not reach NetworkManager over D-Bus")?,
    );

    let device_path = nm.get_wifi_device().await.context("No WiFi device found")?;
    let device_path_str = device_path.as_str().to_string();
    let interface = nm.get_device_interface(&device_path_str).await?;

    let ctx = DoctorContext {
        nm,
        device_path: device_path_str,
        interface,
    };

    let doctor = Doctor::default();
    let results = doctor.run(&ctx).await;

    Report { entries: &results }.print();

    Ok(())
}
