use async_trait::async_trait;

use super::context::DoctorContext;

/// The verdict of a single diagnostic step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Ok,
    Warn,
    Fail,
    Skip,
}

/// Result of one diagnostic check.
#[derive(Debug, Clone)]
pub struct Outcome {
    pub status: Status,
    pub summary: String,
    pub verdict: Option<String>,
}

impl Outcome {
    pub fn ok(summary: impl Into<String>) -> Self {
        Self {
            status: Status::Ok,
            summary: summary.into(),
            verdict: None,
        }
    }

    pub fn warn(summary: impl Into<String>) -> Self {
        Self {
            status: Status::Warn,
            summary: summary.into(),
            verdict: None,
        }
    }

    pub fn fail(summary: impl Into<String>, verdict: impl Into<String>) -> Self {
        Self {
            status: Status::Fail,
            summary: summary.into(),
            verdict: Some(verdict.into()),
        }
    }

    pub fn skip(summary: impl Into<String>) -> Self {
        Self {
            status: Status::Skip,
            summary: summary.into(),
            verdict: None,
        }
    }
}

/// A single diagnostic check. Each implementation owns exactly one concern —
/// rfkill, driver presence, DHCP lease, DNS resolution, etc.
#[async_trait]
pub trait DiagnosticCheck: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self, ctx: &DoctorContext) -> Outcome;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ok_has_no_verdict() {
        let o = Outcome::ok("fine");
        assert_eq!(o.status, Status::Ok);
        assert_eq!(o.summary, "fine");
        assert!(o.verdict.is_none());
    }

    #[test]
    fn fail_carries_verdict() {
        let o = Outcome::fail("broken", "do the thing");
        assert_eq!(o.status, Status::Fail);
        assert_eq!(o.verdict.as_deref(), Some("do the thing"));
    }

    #[test]
    fn warn_and_skip_have_no_verdict() {
        assert!(Outcome::warn("warn").verdict.is_none());
        assert!(Outcome::skip("skip").verdict.is_none());
    }
}
