//! Test-only stage counters; entering a stage is a no-op in production builds.

#[derive(Clone, Copy, Debug)]
pub(super) enum Stage {
    Prepare,
    Solver,
    Measure,
    Materialize,
    Snapshot,
}

#[cfg(not(test))]
#[inline]
pub(super) fn enter(_stage: Stage) {}

#[cfg(test)]
thread_local! {
    static STAGES: std::cell::Cell<[usize; 5]> = const { std::cell::Cell::new([0; 5]) };
}

#[cfg(test)]
pub(super) fn enter(stage: Stage) {
    STAGES.with(|s| {
        let mut counts = s.get();
        counts[stage as usize] += 1;
        s.set(counts);
    });
}

#[cfg(test)]
pub(super) fn reset() {
    STAGES.with(|s| s.set([0; 5]));
}

#[cfg(test)]
pub(super) fn counts() -> [usize; 5] {
    STAGES.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn count(stage: Stage) -> usize {
    counts()[stage as usize]
}
