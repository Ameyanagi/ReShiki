//! Test-only stage counters; entering a stage is a no-op in production builds.

#[derive(Clone, Copy, Debug)]
pub(crate) enum Stage {
    Prepare,
    Solver,
    Measure,
    Materialize,
    Snapshot,
    ScopeCount,
    ScopeParticipants,
    ScopeTraverse,
    RepeatCount,
    Validate,
    #[cfg(test)]
    GroupPairs,
}

#[cfg(not(test))]
#[inline]
pub(crate) fn enter(_stage: Stage) {}

#[cfg(test)]
thread_local! {
    static STAGES: std::cell::Cell<[usize; 11]> = const { std::cell::Cell::new([0; 11]) };
}

#[cfg(test)]
pub(crate) fn enter(stage: Stage) {
    STAGES.with(|s| {
        let mut counts = s.get();
        counts[stage as usize] += 1;
        s.set(counts);
    });
}

#[cfg(test)]
pub(super) fn reset() {
    STAGES.with(|s| s.set([0; 11]));
}

#[cfg(test)]
pub(super) fn counts() -> [usize; 11] {
    STAGES.with(std::cell::Cell::get)
}

#[cfg(test)]
pub(super) fn count(stage: Stage) -> usize {
    counts()[stage as usize]
}
