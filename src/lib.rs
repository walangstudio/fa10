//! `fa10` — grow a file into a larger, fully-reversible test file with
//! recognizable padding.
//!
//! The library is UI-agnostic: operations report progress through the
//! [`progress::Progress`] trait and return rich outcome structs. The CLI
//! ([`fa10` binary](../fa10/index.html)) wraps these with `clap` and
//! `indicatif`.

pub mod error;
pub mod format;
pub mod grow;
pub mod info;
pub mod progress;
pub mod restore;
pub mod safety;
pub mod size;

pub use error::{Fa10Error, Result};
pub use format::{Footer, DEFAULT_PATTERN};
pub use grow::{grow, GrowOptions, GrowOutcome, Target};
pub use info::{info, Fa10Info};
pub use progress::{NoProgress, Progress};
pub use restore::{restore, verify_file, RestoreOptions, RestoreOutcome};
pub use size::parse_size;
