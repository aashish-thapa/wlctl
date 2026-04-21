//! Network diagnostic runner — walks the stack (rfkill → driver → interface →
//! association → DHCP → DNS → gateway → internet → captive portal) and prints
//! an interpreted verdict instead of raw logs.

mod check;
mod checks;
mod context;
mod render;
mod report;

pub use render::render_modal;

use std::sync::Arc;

use anyhow::{Context as _, Result};

use crate::nm::NMClient;

pub use check::{Outcome, Status};

use check::DiagnosticCheck;
use checks::{
    AssociationCheck, DeviceStateCheck, DnsCheck, DriverCheck, GatewayCheck, InternetCheck,
    IpAddressCheck, PortalCheck, RfkillCheck,
};
use context::DoctorContext;
use report::Report;

/// One row in a completed diagnostic report.
pub type CheckEntry = (&'static str, Outcome);

/// TUI-visible state of the doctor modal.
pub enum DoctorModal {
    Running,
    Ready(Vec<CheckEntry>),
}

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
    async fn run(&self, ctx: &DoctorContext) -> Vec<CheckEntry> {
        let mut results = Vec::with_capacity(self.checks.len());
        for check in &self.checks {
            let outcome = check.run(ctx).await;
            results.push((check.name(), outcome));
        }
        results
    }
}

/// Runs the default check suite against the given NM device and returns
/// results. Used by the TUI modal; the CLI uses `run()` below.
pub async fn check_now(
    nm: Arc<NMClient>,
    device_path: String,
    interface: String,
) -> Vec<CheckEntry> {
    let ctx = DoctorContext {
        nm,
        device_path,
        interface,
    };
    Doctor::default().run(&ctx).await
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
    let interface = nm
        .get_device_interface(&device_path_str)
        .await
        .context("Could not read interface name for WiFi device")?;

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
