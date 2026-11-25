//! Test-only, nested wall timers. Times exclude instrumented child phases.
//! Run with cargo test --release -p reshiki-optimize mmff_phase_profile -- --ignored --nocapture.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::ControlFlow;
use std::time::{Duration, Instant};

use crate::{
    ANGSTROM_PER_BOHR, Accuracy, EvalError, KCAL_PER_HARTREE, Method, Options, Potential, Status,
    optimize,
};

#[path = "../../examples/mmff_benchmark/fixture.rs"]
mod fixture;

#[derive(Default)]
struct Sample {
    calls: usize,
    exclusive: Duration,
    inclusive: Duration,
}

#[derive(Default)]
struct Timers {
    samples: BTreeMap<&'static str, Sample>,
    active: Option<&'static str>,
    switched: Option<Instant>,
}

impl Timers {
    fn switch(&mut self, next: Option<&'static str>, now: Instant) -> Option<&'static str> {
        if let (Some(name), Some(started)) = (self.active, self.switched) {
            self.samples.entry(name).or_default().exclusive += now - started;
        }
        self.switched = Some(now);
        std::mem::replace(&mut self.active, next)
    }
}

thread_local! {
    static TIMERS: RefCell<Option<Timers>> = const { RefCell::new(None) };
}

pub(super) struct Span {
    name: &'static str,
    parent: Option<&'static str>,
    started: Instant,
}

impl Span {
    pub(super) fn start(name: &'static str) -> Option<Self> {
        TIMERS.with_borrow_mut(|timers| {
            let timers = timers.as_mut()?;
            let started = Instant::now();
            let parent = timers.switch(Some(name), started);
            Some(Self {
                name,
                parent,
                started,
            })
        })
    }
}

impl Drop for Span {
    fn drop(&mut self) {
        TIMERS.with_borrow_mut(|timers| {
            let timers = timers.as_mut().unwrap();
            let now = Instant::now();
            timers.switch(self.parent, now);
            let sample = timers.samples.entry(self.name).or_default();
            sample.calls += 1;
            sample.inclusive += now - self.started;
        });
    }
}

struct TimedPotential(fixture::MmffPotential);

impl Potential for TimedPotential {
    fn evaluate(&mut self, x: &[f64], g: &mut [f64], accuracy: Accuracy) -> Result<f64, EvalError> {
        let _timer = Span::start("MMFF94");
        self.0.evaluate(x, g, accuracy)
    }
}

#[test]
#[ignore = "release-mode MMFF94 phase benchmark"]
fn mmff_phase_profile() {
    for (name, smiles) in fixture::MOLECULES {
        if !["aspirin", "propranolol", "imatinib"].contains(&name) {
            continue;
        }
        let fixture = fixture::prepare(smiles).unwrap();
        let mut potential = TimedPotential(fixture::MmffPotential(fixture.new_field().unwrap()));
        let options = Options {
            method: Method::Internal,
            ..Options::default()
        };
        TIMERS.set(Some(Timers::default()));
        let start = Instant::now();
        let result = optimize(
            &mut potential,
            &fixture.numbers,
            &fixture.positions,
            &options,
            |_| ControlFlow::Continue(()),
        );
        let elapsed = start.elapsed();
        let timers = TIMERS.take().unwrap();
        assert_eq!(result.status, Status::Converged);
        assert_eq!(result.method, Method::Internal);
        println!(
            "{name} {} atoms ({} heavy), {}/{} steps/evals, {:.3} ms/step",
            fixture.numbers.len(),
            fixture.heavy_atoms,
            result.steps,
            result.evaluations,
            elapsed.as_secs_f64() * 1000. / result.steps as f64
        );
        for (phase, sample) in timers.samples {
            println!(
                "  {phase:20} {:5} calls  {:8.4} ms/step excl  {:8.4} ms/call incl",
                sample.calls,
                sample.exclusive.as_secs_f64() * 1000. / result.steps as f64,
                sample.inclusive.as_secs_f64() * 1000. / sample.calls as f64
            );
        }
    }
}
