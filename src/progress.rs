//! Minimal progress-reporting abstraction so the library stays UI-agnostic.
//! The binary wraps `indicatif`; tests and library callers can pass `&NoProgress`.

/// A sink for progress updates during long streaming operations.
pub trait Progress {
    /// Called once with the total number of bytes expected.
    fn set_total(&self, _total: u64) {}
    /// Called repeatedly with the number of additional bytes processed.
    fn add(&self, _delta: u64) {}
    /// Called once when the operation completes.
    fn finish(&self) {}
}

/// A no-op progress sink.
pub struct NoProgress;
impl Progress for NoProgress {}
