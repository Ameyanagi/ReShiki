//! Original aromatic.circle/removal arithmetic, retaining CPython 3.12 sums.
//! The compensated sum follows Python/bltinmodule.c from CPython 3.12.12;
//! PSF License Version 2, see licenses/cpython/LICENSE and NOTICE.
use super::{Result, SceneError};
use crate::chemistry::cdxml::{
    ImportPoint, PreparedCdxml, arrows::native_hypot, numeric, tree::Tree,
};
use std::collections::HashMap;

fn sum(values: impl IntoIterator<Item = f64>) -> f64 {
    let mut values = values.into_iter();
    // sum starts with integer zero; its first float enters via ordinary 0+x.
    let mut total = 0.0 + values.next().unwrap_or(0.0);
    let mut correction = 0.0;
    for value in values {
        let next = total + value;
        correction += if total.abs() >= value.abs() {
            (total - next) + value
        } else {
            (value - next) + total
        };
        total = next;
    }
    if correction != 0.0 && correction.is_finite() {
        total += correction;
    }
    total
}
fn pair(a: u64, b: u64) -> (u64, u64) {
    (a.min(b), a.max(b))
}

pub(super) fn remove(tree: &mut Tree, prepared: &PreparedCdxml) -> Result<Vec<usize>> {
    if prepared.conformer_3d.is_none() {
        return Ok(Vec::new());
    }
    let edges: HashMap<_, _> = prepared
        .bonds
        .iter()
        .map(|b| (pair(b.a, b.b), b.order))
        .collect();
    let mut expected = Vec::new();
    for ring in &prepared.molecule.state.rings.atoms {
        tree.spend(ring.len())?;
        let n = ring.len();
        if n == 0 {
            continue;
        }
        let positions = ring
            .iter()
            .map(|&i| {
                prepared
                    .molecule
                    .positions
                    .get(i)
                    .map(|p| ImportPoint {
                        x: p.x * 28.0,
                        y: -p.y * 28.0,
                    })
                    .ok_or(SceneError::Invalid("Missing aromatic ring position"))
            })
            .collect::<Result<Vec<_>>>()?;
        let mut aromatic = true;
        for (&a, &b) in ring.iter().zip(ring.iter().cycle().skip(1)) {
            let a = *prepared
                .molecule
                .ids
                .get(a)
                .ok_or(SceneError::Invalid("Missing aromatic atom ID"))?;
            let b = *prepared
                .molecule
                .ids
                .get(b)
                .ok_or(SceneError::Invalid("Missing aromatic atom ID"))?;
            if *edges
                .get(&pair(a, b))
                .ok_or(SceneError::Invalid("Missing aromatic drawing bond"))?
                != 4
            {
                aromatic = false;
            }
        }
        if !aromatic {
            continue;
        }
        let center = ImportPoint {
            x: sum(positions.iter().map(|p| p.x)) / n as f64,
            y: sum(positions.iter().map(|p| p.y)) / n as f64,
        };
        let mut distances = Vec::new();
        let mut lengths = Vec::new();
        for (a, z) in positions.iter().zip(positions.iter().cycle().skip(1)) {
            let (dx, dy) = (z.x - a.x, z.y - a.y);
            let length = native_hypot(dx, dy);
            if length < 0.1 {
                break;
            }
            let t = (((center.x - a.x) * dx + (center.y - a.y) * dy)
                / length.powf(std::hint::black_box(2.0)))
            .clamp(0.0, 1.0);
            distances.push(native_hypot(
                center.x - a.x - t * dx,
                center.y - a.y - t * dy,
            ));
            lengths.push(length);
        }
        if distances.len() != n {
            continue;
        }
        let min = distances
            .into_iter()
            .reduce(f64::min)
            .ok_or(SceneError::Invalid("Missing circle distances"))?;
        let radius = min - sum(lengths) / n as f64 * prepared.drawing_style.bond_spacing_ratio;
        if radius > prepared.drawing_style.line_width_pt * 42.0 / 14.4 * 2.0 {
            expected.push((center, radius));
        }
    }
    let mut removed = Vec::new();
    for fragment in tree.descendants(0)? {
        if tree.node(fragment)?.tag != "fragment" {
            continue;
        }
        for index in tree.children(fragment, "graphic")? {
            let node = tree.node(index)?;
            if node.attr("GraphicType") != Some("Oval") || node.attr("OvalType") != Some("Circle") {
                continue;
            }
            let point = |key| -> Option<ImportPoint> {
                let mut v = node
                    .attr(key)
                    .unwrap_or("")
                    .split_whitespace()
                    .take(2)
                    .map(numeric::float);
                let x = v.next()?.ok()? * prepared.source_scale;
                let y = v.next()?.ok()? * prepared.source_scale;
                Some(ImportPoint { x, y })
            };
            let Some((center, major)) = point("Center3D").zip(point("MajorAxisEnd3D")) else {
                continue;
            };
            let radius = native_hypot(center.x - major.x, center.y - major.y);
            let mut owned = false;
            for (c, r) in &expected {
                tree.spend(1)?;
                if native_hypot(center.x - c.x, center.y - c.y) < 0.25
                    && (radius - r).abs() < 1.0f64.max(r * 0.2)
                {
                    owned = true;
                    break;
                }
            }
            if owned {
                removed.push(index);
                tree.detach(index)?;
            }
        }
    }
    removed.extend(super::ellipses::remove(tree, prepared)?);
    Ok(removed)
}
