use std::sync::Arc;

use crate::nm::NMClient;

/// Shared state passed to every diagnostic check. Checks are read-only — they
/// must not mutate the NM state while running.
pub struct DoctorContext {
    pub nm: Arc<NMClient>,
    pub device_path: String,
    pub interface: String,
}
