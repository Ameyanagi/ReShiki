//! CPython 3.12.12 builtin_sum (Python/bltinmodule.c).
//! Copyright Python Software Foundation; PSF license, see licenses/cpython.
//! Keep the native compensated additions and expression order; no reassociation.
use super::{Error, Result};
use crate::chemistry::{depict::geometry::Coordinates, stereo::Point3};

fn sum(values: impl Iterator<Item = f64>) -> f64 {
    let (mut value, mut correction): (f64, f64) = (0., 0.);
    for next in values {
        let combined = value + next;
        correction += if value.abs() >= next.abs() {
            (value - combined) + next
        } else {
            (next - combined) + value
        };
        value = combined;
    }
    if correction != 0. && correction.is_finite() {
        value += correction;
    }
    value
}

pub(super) fn orient(
    old: &[Point3],
    new: &mut [Point3],
    fixed: &Coordinates,
    keep: bool,
) -> Result<()> {
    orient_with(old, new, fixed, keep, rotation)
}

fn rotation(angle: f64) -> Result<(f64, f64)> {
    // The Windows ARM reference is x64 CPython with the non-FMA UCRT path.
    // Its sine differs from ARM's host CRT at some cleanup angles. Keep the
    // same bounded, audited implementation used by abbreviation placement.
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    {
        crate::chemistry::windows_trigonometry::sin_cos(angle).ok_or(Error::Layout)
    }
    #[cfg(not(all(target_os = "windows", target_arch = "aarch64")))]
    {
        let c = angle.cos();
        let s = angle.sin();
        Ok((s, c))
    }
}

fn orient_with(
    old: &[Point3],
    new: &mut [Point3],
    fixed: &Coordinates,
    keep: bool,
    rotation: impl FnOnce(f64) -> Result<(f64, f64)>,
) -> Result<()> {
    if old.len() != new.len() || new.is_empty() {
        return Err(Error::Layout);
    }
    if fixed.len() >= 2 {
        return Ok(());
    }
    let (bx, by, ax, ay) = if let Some((&pivot, _)) = fixed.first_key_value() {
        let before = old.get(pivot).ok_or(Error::Layout)?;
        let after = new.get(pivot).ok_or(Error::Layout)?;
        (before.x, before.y, after.x, after.y)
    } else {
        let n = f64::from(u32::try_from(new.len()).map_err(|_| Error::Limit)?);
        (
            sum(old.iter().map(|p| p.x)) / n,
            sum(old.iter().map(|p| p.y)) / n,
            sum(new.iter().map(|p| p.x)) / n,
            sum(new.iter().map(|p| p.y)) / n,
        )
    };
    let mut angle = 0.;
    if keep {
        let dot = sum(new
            .iter()
            .zip(old)
            .map(|(p, q)| (p.x - ax) * (q.x - bx) + (p.y - ay) * (q.y - by)));
        let cross = sum(new
            .iter()
            .zip(old)
            .map(|(p, q)| (p.x - ax) * (q.y - by) - (p.y - ay) * (q.x - bx)));
        if dot.abs() + cross.abs() > 1e-12 {
            angle = cross.atan2(dot);
        }
    }
    let (s, c) = rotation(angle)?;
    for point in new {
        let x = point.x - ax;
        let y = point.y - ay;
        *point = Point3 {
            x: bx + c * x - s * y,
            y: by + s * x + c * y,
            z: 0.,
        };
    }
    Ok(())
}

#[cfg(all(test, feature = "rdkit-reference"))]
mod tests;

#[cfg(test)]
mod windows_tests;
