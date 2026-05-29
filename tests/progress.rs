use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};

use fa10::grow::{self, GrowOptions, Target};
use fa10::progress::Progress;
use fa10::restore::{self, RestoreOptions};

#[derive(Default)]
struct Recorder {
    total: AtomicU64,
    added: AtomicU64,
    adds: AtomicU64,
    finished: AtomicU64,
}

impl Progress for Recorder {
    fn set_total(&self, total: u64) {
        self.total.store(total, Ordering::SeqCst);
    }
    fn add(&self, delta: u64) {
        self.added.fetch_add(delta, Ordering::SeqCst);
        self.adds.fetch_add(1, Ordering::SeqCst);
    }
    fn finish(&self) {
        self.finished.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn grow_drives_progress_to_completion() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("payload.bin");
    fs::write(&input, vec![1u8; 4096]).unwrap();

    let opts = GrowOptions::new(vec![input], Target::Multiplier(3.0));
    let rec = Recorder::default();
    let outcome = grow::grow(&opts, &rec).unwrap();

    let total = rec.total.load(Ordering::SeqCst);
    assert_eq!(
        total, outcome.output_size,
        "total should equal final archive size"
    );
    assert!(
        rec.adds.load(Ordering::SeqCst) >= 2,
        "progress should increment in steps"
    );
    assert_eq!(
        rec.added.load(Ordering::SeqCst),
        total,
        "increments must sum to the total"
    );
    assert_eq!(
        rec.finished.load(Ordering::SeqCst),
        1,
        "finish() called exactly once"
    );
}

#[test]
fn restore_drives_progress_to_completion() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("payload.bin");
    fs::write(&input, vec![7u8; 4096]).unwrap();
    let archive = grow::grow(
        &GrowOptions::new(vec![input], Target::Multiplier(2.0)),
        &fa10::progress::NoProgress,
    )
    .unwrap()
    .output_path;

    let out = tempfile::tempdir().unwrap();
    let mut ropts = RestoreOptions::new(archive);
    ropts.output = Some(out.path().to_path_buf());
    let rec = Recorder::default();
    restore::restore(&ropts, &rec).unwrap();

    let total = rec.total.load(Ordering::SeqCst);
    assert!(total > 0, "restore sets a positive total");
    assert_eq!(
        rec.added.load(Ordering::SeqCst),
        total,
        "increments must sum to the total"
    );
    assert_eq!(
        rec.finished.load(Ordering::SeqCst),
        1,
        "finish() called exactly once"
    );
}
