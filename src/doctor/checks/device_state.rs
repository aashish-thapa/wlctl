use async_trait::async_trait;

use crate::doctor::check::{DiagnosticCheck, Outcome};
use crate::doctor::context::DoctorContext;
use crate::nm::DeviceState;

/// Reports the NetworkManager device state (DISCONNECTED, CONNECTING, ACTIVATED...).
pub struct DeviceStateCheck;

#[async_trait]
impl DiagnosticCheck for DeviceStateCheck {
    fn name(&self) -> &'static str {
        "interface"
    }

    async fn run(&self, ctx: &DoctorContext) -> Outcome {
        match ctx.nm.get_device_state(&ctx.device_path).await {
            Ok(state) => match state {
                DeviceState::Activated => Outcome::ok(format!("{} is ACTIVATED", ctx.interface)),
                DeviceState::Config | DeviceState::IpConfig | DeviceState::IpCheck => {
                    Outcome::warn(format!("{} still configuring ({:?})", ctx.interface, state))
                }
                DeviceState::Disconnected | DeviceState::Deactivating => Outcome::warn(format!(
                    "{} is {:?} — no active connection",
                    ctx.interface, state
                )),
                DeviceState::Unavailable => Outcome::fail(
                    format!("{} is UNAVAILABLE", ctx.interface),
                    "NetworkManager cannot manage this device. Check rfkill and driver state above.",
                ),
                DeviceState::Failed => Outcome::fail(
                    format!("{} is FAILED", ctx.interface),
                    "Last connection attempt failed. Check `journalctl -u NetworkManager -n 50`.",
                ),
                _ => Outcome::warn(format!("{} state: {:?}", ctx.interface, state)),
            },
            Err(e) => Outcome::skip(format!("could not read device state: {}", e)),
        }
    }
}
