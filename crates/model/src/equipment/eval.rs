//! Evaluates a definition at parameter values: moves parts, picks the shapes
//! drawn in the current states and poses the joints, holds and handles. The
//! result is in the piece's own millimetres; `plan` places it on the page.
use super::format::*;
use super::geometry::{Affine, Seg, add, flatten, mul, parse_path, sub};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(untagged)]
pub enum Value {
    Number(f64),
    State(String),
    Color(Rgb),
}

pub type Values = BTreeMap<String, Value>;

#[derive(Debug, Clone)]
pub struct Drawn<'a> {
    pub shape: &'a Shape,
    pub path: Vec<Seg>,
}

#[derive(Debug, Clone)]
pub struct PosedJoint<'a> {
    pub joint: &'a Joint,
    pub at: Xy,
    pub dir: Xy,
}

#[derive(Debug, Clone)]
pub struct PosedHold<'a> {
    pub hold: &'a Hold,
    pub from: Xy,
    pub to: Xy,
}

#[derive(Debug, Clone)]
pub struct PosedLiquid<'a> {
    pub liquid: &'a Liquid,
    /// Flattened closed contours.
    pub region: Vec<Vec<Xy>>,
    pub fill: f64,
    pub color: Rgb,
}

#[derive(Debug, Clone)]
pub struct Evaluated<'a> {
    pub shapes: Vec<Drawn<'a>>,
    pub joints: Vec<PosedJoint<'a>>,
    pub holds: Vec<PosedHold<'a>>,
    pub liquids: Vec<PosedLiquid<'a>>,
    /// Part ID and where its handle is.
    pub handles: Vec<(&'a str, Xy)>,
}

pub const DEFAULT_LIQUID: Rgb = [79, 143, 208];

/// The value of every parameter: the given one, else the default.
pub fn resolve(def: &Definition, values: &Values) -> Result<Values, String> {
    let mut out = Values::new();
    for p in &def.params {
        let given = values.get(p.id());
        let v = match (p, given) {
            (Param::Range { .. } | Param::Fill { .. }, Some(Value::Number(n)))
                if !n.is_finite() =>
            {
                return Err(format!("\"{}\" needs a finite number.", p.id()));
            }
            (Param::Range { min, max, .. }, Some(Value::Number(n))) => {
                Value::Number(n.clamp(*min, *max))
            }
            (Param::Fill { .. }, Some(Value::Number(n))) => Value::Number(n.clamp(0., 1.)),
            (Param::States { states, .. }, Some(Value::State(s))) => {
                if !states.iter().any(|x| &x.id == s) {
                    return Err(format!("\"{}\" has no state \"{s}\".", p.id()));
                }
                Value::State(s.clone())
            }
            (Param::Color { .. }, Some(Value::Color(c))) => Value::Color(*c),
            (_, Some(_)) => return Err(format!("\"{}\" has a value of the wrong kind.", p.id())),
            (Param::Range { default, .. } | Param::Fill { default, .. }, None) => {
                Value::Number(*default)
            }
            (Param::States { default, .. }, None) => Value::State(default.clone()),
            (Param::Color { default, .. }, None) => Value::Color(*default),
        };
        out.insert(p.id().to_string(), v);
    }
    if let Some(k) = values.keys().find(|k| !out.contains_key(*k)) {
        return Err(format!("The piece has no parameter \"{k}\"."));
    }
    Ok(out)
}

fn number(def: &Definition, values: &Values, id: &str) -> f64 {
    let default = def.params.iter().find(|p| p.id() == id).map(|p| match p {
        Param::Range { default, .. } | Param::Fill { default, .. } => *default,
        _ => 0.,
    });
    match values.get(id) {
        Some(Value::Number(n)) => n - default.unwrap_or(0.),
        _ => 0.,
    }
}

fn state<'v>(values: &'v Values, id: &str) -> Option<&'v str> {
    match values.get(id) {
        Some(Value::State(s)) => Some(s),
        _ => None,
    }
}

/// The transform of every part, composed with its parents.
pub(crate) fn part_transforms<'a>(
    def: &'a Definition,
    values: &Values,
) -> BTreeMap<&'a str, Affine> {
    let own = |p: &Part| -> Affine {
        let t = number(def, values, &p.param);
        match &p.motion {
            Motion::Translate(v) => Affine::translate(mul(*v, t)),
            Motion::Rotate {
                pivot,
                degrees_per_unit,
            } => Affine::rotate(degrees_per_unit * t, *pivot),
            Motion::Scale { pivot, per_unit } => Affine::scale(1. + per_unit * t, *pivot),
            Motion::Stretch {
                pivot,
                axis,
                per_unit,
            } => Affine::stretch(1. + per_unit * t, *axis, *pivot),
            Motion::PerState(m) => {
                // Relative to the default state, whose pose is the one drawn.
                let at = |s: Option<&str>| {
                    s.and_then(|s| m.get(s))
                        .map_or(Affine::IDENTITY, |m| Affine(*m))
                };
                let default = def.params.iter().find_map(|q| match q {
                    Param::States { id, default, .. } if id == &p.param => Some(default.as_str()),
                    _ => None,
                });
                at(state(values, &p.param)).then(at(default).inverse().unwrap_or(Affine::IDENTITY))
            }
        }
    };
    let by_id: BTreeMap<&str, &Part> = def.parts.iter().map(|p| (p.id.as_str(), p)).collect();
    let mut out = BTreeMap::new();
    for p in &def.parts {
        let mut m = own(p);
        let mut parent = p.parent.as_deref();
        while let Some(k) = parent.and_then(|k| by_id.get(k)) {
            m = own(k).then(m);
            parent = k.parent.as_deref();
        }
        out.insert(p.id.as_str(), m);
    }
    out
}

/// The joints of `def` posed at `values` (already resolved), without drawing
/// its shapes: snapping asks for them on every pointer move.
pub fn joints<'a>(def: &'a Definition, values: &Values) -> Vec<PosedJoint<'a>> {
    posed(def, values, &part_transforms(def, values))
}

/// The holds of `def` posed at `values` (already resolved).
pub fn holds<'a>(def: &'a Definition, values: &Values) -> Vec<PosedHold<'a>> {
    posed_holds(def, values, &part_transforms(def, values))
}

/// Whether something drawn or held only in some states is there at `values`.
fn present(when: &BTreeMap<String, Vec<String>>, values: &Values) -> bool {
    when.iter()
        .all(|(k, states)| state(values, k).is_some_and(|v| states.iter().any(|x| x == v)))
}

fn posed_holds<'a>(
    def: &'a Definition,
    values: &Values,
    parts: &BTreeMap<&str, Affine>,
) -> Vec<PosedHold<'a>> {
    def.holds
        .iter()
        .filter(|h| present(&h.when, values))
        .map(|h| {
            let m = (h.part.as_deref())
                .and_then(|k| parts.get(k).copied())
                .unwrap_or(Affine::IDENTITY);
            PosedHold {
                hold: h,
                from: m.apply(h.from),
                to: m.apply(h.to),
            }
        })
        .collect()
}

fn posed<'a>(
    def: &'a Definition,
    values: &Values,
    parts: &BTreeMap<&str, Affine>,
) -> Vec<PosedJoint<'a>> {
    def.joints
        .iter()
        .map(|j| {
            let (mut at, mut dir) = (j.at(), j.dir());
            for (k, poses) in j.poses() {
                if let Some(p) = state(values, k).and_then(|s| poses.get(s)) {
                    (at, dir) = (p.at, p.dir);
                }
            }
            let m = j
                .part()
                .and_then(|k| parts.get(k).copied())
                .unwrap_or(Affine::IDENTITY);
            PosedJoint {
                joint: j,
                at: m.apply(at),
                dir: m.apply_dir(dir),
            }
        })
        .collect()
}

/// Evaluate `def` at `values` (already resolved with `resolve`).
pub fn evaluate<'a>(def: &'a Definition, values: &Values) -> Result<Evaluated<'a>, String> {
    let parts = part_transforms(def, values);
    let of = |part: &Option<String>| {
        part.as_deref()
            .and_then(|k| parts.get(k).copied())
            .unwrap_or(Affine::IDENTITY)
    };
    let mut shapes = Vec::new();
    for s in &def.shapes {
        if present(&s.when, values) {
            let path = parse_path(&s.path).map_err(|e| format!("Shape \"{}\": {e}", s.id))?;
            shapes.push(Drawn {
                shape: s,
                path: of(&s.part).map(&path),
            });
        }
    }
    let joints = posed(def, values, &parts);
    let holds = posed_holds(def, values, &parts);
    let mut liquids = Vec::new();
    for l in &def.liquids {
        let fill = def.params.iter().find_map(|p| match p {
            Param::Fill { id, liquid, .. } if liquid == &l.id => match values.get(id) {
                Some(Value::Number(n)) => Some(*n),
                _ => None,
            },
            _ => None,
        });
        let color = def.params.iter().find_map(|p| match p {
            Param::Color { id, liquid, .. } if liquid == &l.id => match values.get(id) {
                Some(Value::Color(c)) => Some(*c),
                _ => None,
            },
            _ => None,
        });
        let segs = parse_path(&l.region).map_err(|e| format!("Liquid \"{}\": {e}", l.id))?;
        liquids.push(PosedLiquid {
            liquid: l,
            region: flatten(&segs).into_iter().map(|(p, _)| p).collect(),
            fill: fill.unwrap_or(0.),
            color: color.unwrap_or(DEFAULT_LIQUID),
        });
    }
    let handles = def
        .parts
        .iter()
        .filter_map(|p| {
            Some((
                p.id.as_str(),
                parts.get(p.id.as_str())?.apply(p.handle.as_ref()?.at),
            ))
        })
        .collect();
    Ok(Evaluated {
        shapes,
        joints,
        holds,
        liquids,
        handles,
    })
}

/// A closed tube of width `w` along a polyline: hoses and other tubing.
pub fn tube_outline(centre: &[Xy], w: f64) -> Vec<Seg> {
    // Mitred joins keep the width at bends; very sharp bends are capped.
    let normal = |a: &Xy, b: &Xy| {
        let d = sub(*b, *a);
        let l = d[0].hypot(d[1]).max(1e-9);
        [-d[1] / l, d[0] / l]
    };
    let offsets: Vec<(Xy, Xy)> = centre
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let before = centre.get(i.wrapping_sub(1)).map(|a| normal(a, p));
            let after = centre.get(i + 1).map(|b| normal(p, b));
            let n = match (before, after) {
                (Some(a), Some(b)) => {
                    let m = add(a, b);
                    let l = m[0].hypot(m[1]).max(1e-9);
                    let m = [m[0] / l, m[1] / l];
                    mul(m, 1. / (m[0] * b[0] + m[1] * b[1]).max(0.25))
                }
                (Some(n), None) | (None, Some(n)) => n,
                (None, None) => [0., 0.],
            };
            (*p, mul(n, w / 2.))
        })
        .collect();
    let left: Vec<Xy> = offsets.iter().map(|(p, n)| add(*p, *n)).collect();
    let right: Vec<Xy> = offsets
        .iter()
        .rev()
        .map(|(p, n)| add(*p, mul(*n, -1.)))
        .collect();
    super::geometry::polygon(&[left, right].concat(), true)
}
