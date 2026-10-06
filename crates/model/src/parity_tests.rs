//! Captured-baseline harness for behavior-preserving refactors. Tests write
//! exact Debug text and heap figures, capture them once into a scratch
//! directory, then verify later builds against them on the same machine.
use reshiki_process_heap::allocation_metrics;

/// Heap figures for one measured call, relative to its reset baseline.
pub(crate) struct Allocations {
    pub(crate) count: usize,
    pub(crate) bytes: usize,
    pub(crate) peak: usize,
}
impl std::fmt::Display for Allocations {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "count={} bytes={} peak={}",
            self.count, self.bytes, self.peak
        )
    }
}

/// Run `f` and return its result with the allocations it made. Do no
/// formatting inside `f`, so only the measured workload is counted.
pub(crate) fn measured<T>(f: impl FnOnce() -> T) -> (T, Allocations) {
    let base = allocation_metrics::reset();
    let value = f();
    let snapshot = allocation_metrics::snapshot();
    (
        value,
        Allocations {
            count: snapshot.allocation_count,
            bytes: snapshot.allocated_bytes,
            peak: snapshot.peak_bytes.saturating_sub(base),
        },
    )
}

/// Capture `text` as `<RESHIKI_MODEL_PARITY>/<name>.txt` when
/// RESHIKI_MODEL_CAPTURE_BASELINE is `1`, otherwise compare it line by line.
pub(crate) fn check_baseline(name: &str, text: &str) {
    let dir = std::path::PathBuf::from(
        std::env::var("RESHIKI_MODEL_PARITY")
            .expect("Set RESHIKI_MODEL_PARITY to the baseline directory"),
    );
    let file = dir.join(format!("{name}.txt"));
    let lines = text.lines().count();
    if std::env::var("RESHIKI_MODEL_CAPTURE_BASELINE").as_deref() == Ok("1") {
        std::fs::create_dir_all(&dir).expect("Create the baseline directory");
        std::fs::write(&file, text).expect("Write the baseline");
        println!("captured {name}: {lines} lines");
        return;
    }
    let expected = std::fs::read_to_string(&file).expect("Read the captured baseline");
    for (index, (actual, wanted)) in text.lines().zip(expected.lines()).enumerate() {
        if actual != wanted {
            panic!(
                "{name} differs at line {}:\n  baseline: {wanted}\n  current:  {actual}",
                index + 1
            );
        }
    }
    let wanted = expected.lines().count();
    if lines != wanted {
        panic!("{name} has {lines} lines; the baseline has {wanted}");
    }
    println!("matched {name}: {lines} lines");
}
