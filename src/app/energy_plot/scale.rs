//! Round axis intervals shared by the Path plot and GIF export.

/// Round a scale in the displayed unit, rather than making kJ or eV ticks
/// inherit awkward converted kcal intervals.
pub(in crate::app) fn nice_scale(lo: f64, hi: f64) -> (f64, f64, Vec<f64>) {
    let raw = (hi - lo).max(0.000_001) / 4.;
    let magnitude = 10_f64.powf(raw.log10().floor());
    let fraction = raw / magnitude;
    let step = magnitude
        * if fraction <= 1. {
            1.
        } else if fraction <= 2. {
            2.
        } else if fraction <= 5. {
            5.
        } else {
            10.
        };
    let bottom = (lo / step).floor() * step;
    let top = (hi / step).ceil() * step;
    let count = ((top - bottom) / step).round() as usize;
    let ticks = (0..=count.min(12))
        .map(|index| bottom + index as f64 * step)
        .collect();
    (bottom, top, ticks)
}
