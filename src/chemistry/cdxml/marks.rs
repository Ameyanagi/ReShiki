//! Detached patches from engine/marks_exchange.py::read_marks.
use super::{
    Result,
    association::{self, ImportPoint, ObjectMapEntry, PreparedAtoms, invalid, numbers},
    tree::Tree,
};
use crate::scientific::MarkKind;
use serde::Serialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Serialize)]
pub struct NativeMark {
    pub kind: MarkKind,
    pub offset: ImportPoint,
    pub angle: f64,
    pub size_pt: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct AtomMarks {
    pub id: u64,
    pub marks: Vec<NativeMark>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Marks {
    /// First encounter order. Append each list to the first base atom with this
    /// ID, creating that atom at the end if absent; retain all other fields.
    pub atoms: Vec<AtomMarks>,
    pub objects: Vec<ObjectMapEntry>,
}

/// Read attached symbols without modifying source XML, prepared chemistry or
/// an existing drawing. Source positions use `source_scale`; symbol size does
/// not. Object-map updates retain source element identity, including absent IDs.
pub fn read_marks(xml: &str, prepared: &PreparedAtoms<'_>, source_scale: f64) -> Result<Marks> {
    association::scale(source_scale)?;
    let tree = Tree::parse(xml)?;
    let order = tree.descendants(0)?;
    let mut nodes = HashMap::new();
    for &index in &order {
        let node = tree.node(index)?;
        if node.tag == "n" {
            nodes.insert(node.attr("id"), index);
        }
    }
    let mut result = Marks {
        atoms: Vec::new(),
        objects: Vec::new(),
    };
    let mut entries = HashMap::new();
    let mut cache = HashMap::new();
    for (source, &index) in order.iter().enumerate() {
        let graphic = tree.node(index)?;
        if graphic.tag != "graphic" {
            continue;
        }
        let reps = tree.children(index, "represent")?;
        let Some(&first) = reps.first() else {
            continue;
        };
        let owner = tree.node(first)?.attr("object");
        for rep in reps {
            let rep = tree.node(rep)?;
            if rep.attr("object") != owner
                || !matches!(rep.attr("attribute"), Some("Charge" | "Radical"))
            {
                return Err(invalid("Unsupported chemical-symbol attachment"));
            }
        }
        let &node = nodes
            .get(&owner)
            .ok_or_else(|| invalid("Chemical symbol refers to a missing atom"))?;
        let (id, xy) = if let Some(&cached) = cache.get(&node) {
            cached
        } else {
            let values = numbers(
                tree.node(node)?
                    .attr("p")
                    .ok_or_else(|| invalid("Missing chemical symbol atom coordinates"))?,
                "Invalid chemical symbol atom coordinates",
            )?;
            let x = values
                .first()
                .copied()
                .ok_or_else(|| invalid("Invalid chemical symbol atom coordinates"))?
                * source_scale;
            let y = values
                .get(1)
                .copied()
                .ok_or_else(|| invalid("Invalid chemical symbol atom coordinates"))?
                * source_scale;
            let xy = ImportPoint { x, y };
            let id = prepared.identify(
                &tree,
                node,
                xy,
                "Could not safely associate a chemical symbol with its atom",
            )?;
            cache.insert(node, (id, xy));
            (id, xy)
        };
        let atom = prepared.atom(id)?;
        let symbol = graphic.attr("SymbolType");
        let kind = match symbol {
            Some(symbol @ ("Plus" | "Minus" | "CirclePlus" | "CircleMinus")) => {
                if atom.charge == 0 {
                    return Err(invalid("A charge symbol refers to an uncharged atom"));
                }
                if (atom.charge > 0) != symbol.ends_with("Plus") {
                    return Err(invalid("Charge-symbol sign conflicts with its atom"));
                }
                if symbol.starts_with("Circle") {
                    MarkKind::CircledCharge
                } else {
                    MarkKind::Charge
                }
            }
            Some("Electron") => {
                if atom.radical_electrons != 1 {
                    return Err(invalid(
                        "Single-electron mark conflicts with atom radical count",
                    ));
                }
                MarkKind::Radical
            }
            Some("LonePair") => {
                if atom.radical_electrons == 2 {
                    MarkKind::Radical
                } else {
                    MarkKind::LonePair
                }
            }
            _ => return Err(invalid("This attached CDXML symbol is not supported yet")),
        };
        let bounds = numbers(
            graphic
                .attr("BoundingBox")
                .ok_or_else(|| invalid("Missing chemical symbol coordinates"))?,
            "Invalid chemical symbol coordinates",
        )?;
        let [x1, y1, x2, y2] = bounds.as_slice() else {
            return Err(invalid("Invalid chemical symbol coordinates"));
        };
        if !bounds.iter().all(|v| v.is_finite()) {
            return Err(invalid("Invalid chemical symbol coordinates"));
        }
        let (dx, dy) = (x1 - x2, y1 - y2);
        let mut size = native_hypot(dx, dy);
        if matches!(symbol, Some("Electron" | "LonePair")) {
            size *= 2.;
        }
        if !(0.5..=96.).contains(&size) {
            return Err(invalid("Unsupported chemical symbol size"));
        }
        if ["color", "LineWidth", "BoldWidth"]
            .iter()
            .any(|key| graphic.attr(key).is_some())
        {
            return Err(invalid(
                "Per-symbol color/line overrides are not supported yet",
            ));
        }
        let mark = NativeMark {
            kind,
            offset: ImportPoint {
                x: x1 * source_scale - xy.x,
                y: y1 * source_scale - xy.y,
            },
            angle: dy.atan2(dx).to_degrees(),
            size_pt: size,
        };
        let entry = *entries.entry(id).or_insert_with(|| {
            let next = result.atoms.len();
            result.atoms.push(AtomMarks {
                id,
                marks: Vec::new(),
            });
            next
        });
        result
            .atoms
            .get_mut(entry)
            .ok_or_else(|| invalid("Missing mark output atom"))?
            .marks
            .push(mark);
        result.objects.push(ObjectMapEntry {
            source,
            atoms: vec![id],
        });
    }
    Ok(result)
}

/// CPython 3.12.12 Modules/mathmodule.c compensated vector_norm, specialized
/// to two coordinates. Copyright Python Software Foundation; PSF license, see
/// licenses/cpython. Adapted from chemistry/abbreviations/replacement.rs in
/// checkpoint 18c8aa6. Only mark sizes in [0.5,96] can be accepted, even after
/// doubling, so maxima outside [0.125,96] can return directly for rejection.
/// This keeps the power-of-two scaling within the normal f64 range.
fn native_hypot(x: f64, y: f64) -> f64 {
    let maximum = x.abs().max(y.abs());
    if !maximum.is_finite() || !(0.125..=96.).contains(&maximum) {
        return maximum;
    }
    let exponent = ((maximum.to_bits() >> 52) & 0x7ff) as i32;
    let scale = 2.0_f64.powi(1022 - exponent);
    let mut sum = 1.0;
    let mut products = 0.0;
    let mut additions = 0.0;
    for value in [x.abs(), y.abs()] {
        let value = value * scale;
        let square = value * value;
        let error = value.mul_add(value, -square);
        let combined = sum + square;
        additions += square - (combined - sum);
        products += error;
        sum = combined;
    }
    let mut norm = (sum - 1.0 + (products + additions)).sqrt();
    let square = -norm * norm;
    let error = (-norm).mul_add(norm, -square);
    let combined = sum + square;
    additions += square - (combined - sum);
    products += error;
    sum = combined;
    norm += (sum - 1.0 + (products + additions)) / (2.0 * norm);
    norm / scale
}
