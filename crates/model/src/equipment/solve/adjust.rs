//! Movable parts. Setting a parameter moves a piece's parts, and with them
//! what is seated on those parts: a clamp's head tilts the flask it holds, an
//! opened jaw lets go. Handles on the canvas set parameters by dragging a
//! part or a liquid's surface, or by clicking a part that has states.
use super::*;
use crate::equipment::eval::{Values, part_transforms};
use crate::equipment::format::{Gesture, Motion, Param, Unit};
use crate::equipment::geometry::{flatten, parse_path};
use crate::equipment::plan;

/// A handle on a placed piece: a movable part's, or a liquid's surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Handle {
    /// The parameter it sets.
    pub param: String,
    /// World units.
    pub at: [f64; 2],
    /// A click switches to the next state; otherwise a drag sets the value.
    pub toggle: bool,
    /// The part it moves; `None` for a liquid's surface.
    pub part: Option<String>,
    /// A hose's end or middle, dragged to reshape it; `param` is empty then.
    pub grip: Option<super::hose::Grip>,
    /// A straight tube's cut end. Dragging it keeps the opposite end fixed.
    pub tube_end: Option<String>,
}

/// Recognize the ordinary single-span compression tube by its mechanics,
/// including adjustable definitions embedded before `tube_spec` existed.
/// Bent tubes retain their independently editable leg handles.
fn straight_tube(d: &Definition) -> Option<(&str, &str, [&str; 2])> {
    let [part] = d.parts.as_slice() else {
        return None;
    };
    let [a, b] = d.joints.as_slice() else {
        return None;
    };
    let [span] = d.holds.as_slice() else {
        return None;
    };
    let Motion::Stretch {
        pivot,
        axis,
        per_unit,
    } = part.motion
    else {
        return None;
    };
    let Some(Param::Range {
        min,
        default,
        unit: Unit::Mm,
        ..
    }) = d.params.iter().find(|p| p.id() == part.param)
    else {
        return None;
    };
    let compression = |j: &Joint| {
        matches!(
            j,
            Joint::Plug {
                family: PlugFamily::Compression,
                gender: Gender::M,
                ..
            }
        )
    };
    let delta = sub(b.at(), a.at());
    let length = len(delta);
    if d.grounded
        || d.tube.is_some()
        || !d.liquids.is_empty()
        || part.parent.is_some()
        || !compression(a)
        || !compression(b)
        || a.part() != Some(part.id.as_str())
        || b.part() != Some(part.id.as_str())
        || span.kind != HoldKind::TubeSpan
        || span.part.as_deref() != Some(part.id.as_str())
        || span.from != a.at()
        || span.to != b.at()
        || d.shapes
            .iter()
            .any(|s| s.part.as_deref() != Some(part.id.as_str()))
        || *min <= 0.
        || !length.is_finite()
        || length <= 0.
        || (length - default).abs() > 1e-6
        || (per_unit * default - 1.).abs() > 1e-6
        || pivot != a.at()
        || !len(axis).is_finite()
        || len(axis) <= 0.
        || !len(a.dir()).is_finite()
        || len(a.dir()) <= 0.
        || !len(b.dir()).is_finite()
        || len(b.dir()) <= 0.
        || !a.poses().is_empty()
        || !b.poses().is_empty()
        || dot(normalize(axis), normalize(delta)).abs() < 1. - 1e-6
        || dot(normalize(a.dir()), normalize(delta)) > -1. + 1e-6
        || dot(normalize(b.dir()), normalize(delta)) < 1. - 1e-6
    {
        return None;
    }
    Some((&part.param, &part.id, [a.id(), b.id()]))
}

fn definition(g: &Graphic) -> Option<(&Arc<Definition>, Values)> {
    let piece = g.piece.as_ref()?;
    let d = piece.linked.as_ref()?;
    Some((d, resolve(d, &piece.params).ok()?))
}

/// A liquid's region on the page.
fn region(g: &Graphic, d: &Definition, liquid: &str) -> Vec<Vec<[f64; 2]>> {
    let t = frame(g);
    let path = d.liquids.iter().find(|l| l.id == liquid).map(|l| &l.region);
    let segs = path.and_then(|p| parse_path(p).ok()).unwrap_or_default();
    (flatten(&segs).into_iter())
        .map(|(c, _)| c.into_iter().map(|p| t.apply(p)).collect())
        .collect()
}

/// The fill parameter of each liquid that no part's handle drives.
fn fills(d: &Definition) -> impl Iterator<Item = (&str, &str)> {
    d.params.iter().filter_map(|p| match p {
        Param::Fill { id, liquid, .. }
            if !(d.parts.iter()).any(|k| k.param == *id && k.handle.is_some()) =>
        {
            Some((id.as_str(), liquid.as_str()))
        }
        _ => None,
    })
}

/// Every handle of a placed piece; none for any other graphic.
pub fn handles(g: &Graphic) -> Vec<Handle> {
    let Some((d, values)) = definition(g) else {
        return Vec::new();
    };
    let t = frame(g);
    let parts = part_transforms(d, &values);
    let mut out: Vec<Handle> = (d.parts.iter())
        .filter_map(|p| {
            let h = p.handle.as_ref()?;
            Some(Handle {
                param: p.param.clone(),
                at: t.apply(parts.get(p.id.as_str())?.apply(h.at)),
                toggle: h.gesture == Gesture::Toggle,
                part: Some(p.id.clone()),
                grip: None,
                tube_end: None,
            })
        })
        .collect();
    if let Some((param, part, ends)) = straight_tube(d) {
        out.retain(|h| h.part.as_deref() != Some(part));
        let ports = ports(g);
        for end in ends {
            if let Some(port) = ports.iter().find(|p| p.at.joint == end) {
                out.push(Handle {
                    param: param.to_owned(),
                    at: port.point,
                    toggle: false,
                    part: Some(part.to_owned()),
                    grip: None,
                    tube_end: Some(end.to_owned()),
                });
            }
        }
    }
    for (id, liquid) in fills(d) {
        let fill = match values.get(id) {
            Some(Value::Number(n)) => *n,
            _ => 0.,
        };
        if let Some(at) = plan::surface(&region(g, d, liquid), fill) {
            out.push(Handle {
                param: id.to_owned(),
                at,
                toggle: false,
                part: None,
                grip: None,
                tube_end: None,
            });
        }
    }
    out.extend(super::hose::grips(g).into_iter().map(|(grip, at)| Handle {
        param: String::new(),
        at,
        toggle: false,
        part: None,
        grip: Some(grip),
        tube_end: None,
    }));
    out
}

/// The parameter's range and the step a drag rounds to.
fn range(p: &Param) -> Option<(f64, f64, f64)> {
    match p {
        Param::Range { min, max, unit, .. } => Some((
            *min,
            *max,
            match unit {
                Unit::Deg => 1.,
                Unit::Mm => 0.5,
                Unit::Fraction => 0.01,
            },
        )),
        Param::Fill { .. } => Some((0., 1., 0.01)),
        _ => None,
    }
}

/// The value a drag of `handle` to `to` (world units) sets: the one that
/// brings the handle nearest the pointer, or a liquid's level to it.
pub fn dragged(g: &Graphic, handle: &Handle, to: [f64; 2]) -> Option<Value> {
    if !to.iter().all(|v| v.is_finite()) {
        return None;
    }
    let (d, mut values) = definition(g)?;
    let (lo, hi, step) = range(d.params.iter().find(|p| p.id() == handle.param)?)?;
    let round = |v: f64| ((v / step).round() * step).clamp(lo, hi);
    if let Some(end) = &handle.tube_end {
        let (param, _, ends) = straight_tube(d)?;
        if param != handle.param || !ends.contains(&end.as_str()) {
            return None;
        }
        let local = frame(g).inverse()?.apply(to);
        let joints = crate::equipment::eval::joints(d, &values);
        let current = joints.iter().find(|j| j.joint.id() == end)?;
        let other = joints.iter().find(|j| j.joint.id() != end)?;
        let Value::Number(length) = values.get(param)? else {
            return None;
        };
        return Some(Value::Number(round(
            length + dot(sub(local, current.at), normalize(sub(current.at, other.at))),
        )));
    }
    let Some(part) = &handle.part else {
        let (_, liquid) = fills(d).find(|(id, _)| *id == handle.param)?;
        return Some(Value::Number(round(plan::share_below(
            &region(g, d, liquid),
            to[1],
        ))));
    };
    let at = d.parts.iter().find(|p| p.id == *part)?.handle.as_ref()?.at;
    let t = frame(g);
    let mut off = |v: f64| {
        values.insert(handle.param.clone(), Value::Number(v));
        let p = part_transforms(d, &values)
            .get(part.as_str())
            .map_or(at, |m| m.apply(at));
        len(sub(t.apply(p), to))
    };
    // Coarse samples, then a golden-section search around the nearest.
    let n = 48;
    let width = (hi - lo) / f64::from(n);
    let best = (0..=n)
        .map(|i| lo + width * f64::from(i))
        .min_by(|a, b| off(*a).total_cmp(&off(*b)))?;
    let (mut a, mut b) = ((best - width).max(lo), (best + width).min(hi));
    let r = (5_f64.sqrt() - 1.) / 2.;
    for _ in 0..40 {
        let (x, y) = (b - r * (b - a), a + r * (b - a));
        if off(x) < off(y) {
            b = y;
        } else {
            a = x;
        }
    }
    Some(Value::Number(round((a + b) / 2.)))
}

/// The state after the current one of the states parameter `param`.
pub fn toggled(g: &Graphic, param: &str) -> Option<Value> {
    let (d, values) = definition(g)?;
    let Some(Param::States { states, .. }) = d.params.iter().find(|p| p.id() == param) else {
        return None;
    };
    let now = match values.get(param) {
        Some(Value::State(s)) => states.iter().position(|x| x.id == *s).unwrap_or(0),
        _ => 0,
    };
    Some(Value::State(
        states.get((now + 1) % states.len())?.id.clone(),
    ))
}

/// The rigid motion between the poses of the attachment.
fn motion(from: &Port, to: &Port) -> Affine {
    let angle = |v: [f64; 2]| v[1].atan2(v[0]).to_degrees();
    let (a, b) = if from.insertion > 0. || to.insertion > 0. {
        (from.contact(), to.contact())
    } else {
        (from.point, to.point)
    };
    Affine::translate(b)
        .then(Affine::rotate(angle(to.dir) - angle(from.dir), [0., 0.]))
        .then(Affine::translate(mul(a, -1.)))
}

fn same(a: &Affine, b: &Affine) -> bool {
    (a.0.iter().zip(b.0)).all(|(x, y)| (x - y).abs() <= 1e-6 * (1. + x.abs()))
}

/// Move pieces `ids` by the similarity `t`.
fn reframe(doc: &mut Document, ids: &BTreeSet<u64>, t: Affine) {
    let p = |v: [f64; 2]| Point::new(v[0] as f32, v[1] as f32);
    let f = |v: Point| [f64::from(v.x), f64::from(v.y)];
    let [a, b, c, d, ..] = t.0;
    let linear = |v: Point| {
        let [x, y] = f(v);
        p([a * x + c * y, b * x + d * y])
    };
    for g in (doc.graphics.iter_mut()).filter(|g| g.piece.is_some() && ids.contains(&g.id)) {
        g.origin = p(t.apply(f(g.origin)));
        g.axis_x = linear(g.axis_x);
        g.axis_y = linear(g.axis_y);
    }
}

/// The rest of `id`'s assembly, split where it meets `id`.
fn components(doc: &Document, id: u64) -> Vec<BTreeSet<u64>> {
    let mut out: Vec<BTreeSet<u64>> = Vec::new();
    for m in assembly(doc, id).into_iter().filter(|m| *m != id) {
        if !out.iter().any(|c| c.contains(&m)) {
            out.push(reach(doc, m, |s| !s.has(id)));
        }
    }
    out
}

/// The motion classes of `id`'s ports that a component touches: 0 for
/// ports that stay, k for the k-th distinct motion.
fn touching(
    doc: &Document,
    id: u64,
    c: &BTreeSet<u64>,
    class: &BTreeMap<JointRef, usize>,
) -> BTreeSet<usize> {
    (doc.seats.iter())
        .filter(|s| s.has(id))
        .filter_map(|s| {
            let (mine, other) = if s.a.piece == id {
                (&s.a, &s.b)
            } else {
                (&s.b, &s.a)
            };
            c.contains(&other.piece)
                .then(|| class.get(mine).copied().unwrap_or(0))
        })
        .collect()
}

/// For each edge, the biconnected component it is in (Tarjan).
fn biconnected(n: usize, edges: impl IntoIterator<Item = (usize, usize)>) -> Vec<usize> {
    struct T {
        disc: Vec<usize>,
        time: usize,
        stack: Vec<usize>,
        comp: Vec<usize>,
        next: usize,
    }
    fn visit(
        t: &mut T,
        adj: &[Vec<(usize, usize)>],
        u: usize,
        parent: Option<usize>,
    ) -> Option<usize> {
        let neighbors = adj.get(u)?;
        let entry = t.disc.get_mut(u)?;
        t.time += 1;
        let disc = t.time;
        *entry = disc;
        let mut low = disc;
        if let Some(e) = parent {
            t.stack.push(e);
        }
        for &(v, e) in neighbors {
            if Some(e) == parent {
                continue;
            }
            let Some(&v_disc) = t.disc.get(v) else {
                continue;
            };
            if v_disc == 0 {
                let Some(v_low) = visit(t, adj, v, Some(e)) else {
                    continue;
                };
                low = low.min(v_low);
                if v_low >= disc {
                    while let Some(x) = t.stack.pop() {
                        if let Some(component) = t.comp.get_mut(x) {
                            *component = t.next;
                        }
                        if x == e {
                            break;
                        }
                    }
                    t.next += 1;
                }
            } else if v_disc < disc {
                t.stack.push(e);
                low = low.min(v_disc);
            }
        }
        Some(low)
    }
    let mut t = T {
        disc: vec![0; n],
        time: 0,
        stack: Vec::new(),
        comp: Vec::new(),
        next: 0,
    };
    let mut adj = vec![Vec::new(); n];
    for (e, (u, v)) in edges.into_iter().enumerate() {
        t.comp.push(usize::MAX);
        if u >= n || v >= n {
            continue;
        }
        for (from, to) in [(u, v), (v, u)] {
            if let Some(neighbors) = adj.get_mut(from) {
                neighbors.push((to, e));
            }
        }
    }
    for u in 0..n {
        if t.disc.get(u) == Some(&0) {
            let _ = visit(&mut t, &adj, u, None);
        }
    }
    t.comp
}

/// The seat to release in a component that touches ports moving in
/// different ways: of those on a path between two of them, a hold first,
/// then a plug, then a taper; the nearest a moving port first; the oldest
/// seat first.
fn choose(
    doc: &Document,
    id: u64,
    c: &BTreeSet<u64>,
    class: &BTreeMap<JointRef, usize>,
    pinned: bool,
) -> Option<Seat> {
    // Nodes: the component's pieces, then one per class of `id`'s ports.
    let pieces: Vec<u64> = c.iter().copied().collect();
    let mut classes = touching(doc, id, c, class);
    if pinned && fixed(doc, c) {
        classes.insert(0);
    }
    let classes: Vec<usize> = classes.into_iter().collect();
    let piece_count = pieces.len();
    let node = |j: &JointRef| -> Option<usize> {
        if j.piece == id {
            let k = class.get(j).copied().unwrap_or(0);
            classes
                .iter()
                .position(|x| *x == k)
                .map(|i| piece_count + i)
        } else {
            pieces.iter().position(|p| *p == j.piece)
        }
    };
    let seats: Vec<_> = doc
        .seats
        .iter()
        .filter_map(|s| Some(((node(&s.a)?, node(&s.b)?), s)))
        .collect();
    let n = piece_count + classes.len();
    // For an explicit end drag the opposite end, and any grounded branch,
    // stay. Include ground locks in the same weak-seat-first release rule.
    let zero = classes
        .iter()
        .position(|k| *k == 0)
        .map(|i| piece_count + i);
    let locks: Vec<_> = pieces
        .iter()
        .enumerate()
        .filter(|(_, piece)| pinned && grounded(doc, **piece))
        .filter_map(|(i, _)| Some((i, zero?)))
        .collect();
    let virtual_edges = (piece_count..n).flat_map(|u| (u + 1..n).map(move |v| (u, v)));
    let edges = seats
        .iter()
        .map(|&(edge, _)| edge)
        .chain(locks.iter().copied())
        .chain(virtual_edges);
    let comp = biconnected(n, edges);
    let between: BTreeSet<usize> = comp
        .iter()
        .skip(seats.len() + locks.len())
        .copied()
        .filter(|&k| k != usize::MAX)
        .collect();
    // Seats away from a moving port.
    let mut dist = vec![usize::MAX; n];
    let mut queue: std::collections::VecDeque<usize> = (classes.iter().enumerate())
        .filter(|(_, k)| **k != 0)
        .map(|(i, _)| piece_count + i)
        .collect();
    for &q in &queue {
        if let Some(d) = dist.get_mut(q) {
            *d = 0;
        }
    }
    while let Some(u) = queue.pop_front() {
        let Some(next) = dist.get(u).and_then(|d| d.checked_add(1)) else {
            continue;
        };
        for &((a, b), _) in &seats {
            let v = if a == u {
                b
            } else if b == u {
                a
            } else {
                continue;
            };
            if let Some(d) = dist.get_mut(v).filter(|d| **d == usize::MAX) {
                *d = next;
                queue.push_back(v);
            }
        }
    }
    let rank = |s: &Seat| match port(doc, &s.a).map(|p| p.kind) {
        Some(Kind::Hold(..)) | None => 0,
        Some(Kind::Plug(..)) => 1,
        Some(Kind::Taper(_)) => 2,
    };
    let distance = |u| dist.get(u).copied().unwrap_or(usize::MAX);
    seats
        .into_iter()
        .zip(comp)
        .filter(|(_, k)| between.contains(k))
        // Equal ranks keep the first seat in document order.
        .min_by_key(|&(((u, v), s), _)| (rank(s), distance(u).min(distance(v))))
        .map(|((_, s), _)| s)
        .cloned()
}

/// Set parameter `param` of the piece `id` to `value` (clamped to its range)
/// and re-solve what is seated on it. Each part of the assembly that
/// touches the piece only through ports that moved one way moves with them;
/// one that touches ports that stay, or ports that moved differently, gives
/// up seats on a path between them until it no longer does. When the
/// assembly's anchor would move, the piece and the rest move back instead,
/// so a stand or the flask stays where it is. A port that is gone, such as
/// the jaw of an opened clamp, lets go; one that moved or came back and now
/// fits a free one is seated on it. Returns the seats released.
pub fn set(doc: &mut Document, id: u64, param: &str, value: Value) -> Result<Vec<Seat>, String> {
    set_inner(doc, id, param, value, None)
}

/// Set the same Length parameter by moving one cut end of a straight tube.
/// The opposite end and any through-bore holders stay where they are.
/// This is deliberately separate from the Properties parameter operation.
pub fn set_end(
    doc: &mut Document,
    id: u64,
    param: &str,
    end: &str,
    value: Value,
) -> Result<Vec<Seat>, String> {
    let g = doc
        .graphics
        .iter()
        .find(|g| g.id == id)
        .ok_or("The tube is not in the drawing.")?;
    let (d, _) = definition(g).ok_or("The tube definition is missing.")?;
    let (length, _, ends) = straight_tube(d).ok_or("This tube has no editable straight ends.")?;
    if length != param
        || !ends.contains(&end)
        || !frame(g).0.iter().all(|v| v.is_finite())
        || frame(g).inverse().is_none()
    {
        return Err("This tube end cannot be adjusted.".into());
    }
    let opposite = ends
        .iter()
        .find(|e| **e != end)
        .ok_or("The opposite tube end is missing.")?
        .to_string();
    set_inner(doc, id, param, value, Some(&opposite))
}

/// Drag a cut end, seating to a nearby socket by changing only the tube's
/// length. A reducer is previewed as an explicit offer, never inserted here.
pub fn snap_end(
    doc: &mut Document,
    id: u64,
    param: &str,
    end: &str,
    value: Value,
    reach: f32,
) -> Result<(Vec<Seat>, Option<Snap>), String> {
    let joint = JointRef {
        piece: id,
        joint: end.into(),
    };
    let was_free = partner(doc, &joint).is_none();
    let released = set_end(doc, id, param, end, value)?;
    if !was_free || !reach.is_finite() || reach < 0. {
        return Ok((released, None));
    }
    let Some(m) = port(doc, &joint) else {
        return Ok((released, None));
    };
    // Exact native auto-seating has already done the same length-only edit.
    if let Some(other) = partner(doc, &joint)
        && let Some(s) = port(doc, &other)
    {
        return Ok((released, Some(end_snap(s, m, Offer::Seat, None))));
    }
    let mut nearby = Vec::new();
    for s in free(doc, &BTreeSet::from([id]), false) {
        if !matches!(s.kind, Kind::Plug(PlugFamily::Compression, _))
            || s.gender != Gender::F
            || misalign(&s, &m) > TOLERANCE_DEG
            || (s.mm / m.mm - 1.).abs() > 1e-3
        {
            continue;
        }
        let distance = gap(&s, &m);
        if distance > f64::from(reach).max(REACH_DIAMETERS * s.diameter()) {
            continue;
        }
        nearby.push((!mates(&s, &m), distance, s));
    }
    // Rank before copying a document. Reducer previews only need catalogue
    // metadata; their complete placement plan is prepared once on release.
    nearby.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    for (mismatch, _, s) in nearby {
        if !mismatch {
            if let Some((candidate, snap)) = plan_end(doc, id, param, end, &s.at) {
                *doc = candidate;
                return Ok((released, Some(snap)));
            }
        } else if crate::graphics::table::require_same_cell(doc, &[id, s.at.piece]).is_ok()
            && let Some(reducer) = compression_reducer(&s, &m)
        {
            return Ok((
                released,
                Some(end_snap(s, m, Offer::Reducer(reducer), None)),
            ));
        }
    }
    Ok((released, None))
}

fn end_snap(target: Port, moving: Port, offer: Offer, added: Option<u64>) -> Snap {
    Snap {
        at: target.point,
        target,
        moving,
        offer,
        scaled: false,
        behind: None,
        added,
    }
}

/// Explicitly accept a reducer offer at an editable free tube end. The
/// complete plan is checked on a copy, so a failed or stale action is atomic.
pub fn connect_end(
    doc: &mut Document,
    id: u64,
    param: &str,
    end: &str,
    target: &JointRef,
) -> Option<Snap> {
    let (candidate, snap) = plan_end(doc, id, param, end, target)?;
    *doc = candidate;
    Some(snap)
}

fn plan_end(
    doc: &Document,
    id: u64,
    param: &str,
    end: &str,
    target: &JointRef,
) -> Option<(Document, Snap)> {
    let moving = JointRef {
        piece: id,
        joint: end.into(),
    };
    if target.piece == id
        || partner(doc, &moving).is_some()
        || partner(doc, target).is_some()
        || crate::graphics::table::require_same_cell(doc, &[id, target.piece]).is_err()
    {
        return None;
    }
    let (m, s) = (port(doc, &moving)?, port(doc, target)?);
    if !matches!(m.kind, Kind::Plug(PlugFamily::Compression, _))
        || !matches!(s.kind, Kind::Plug(PlugFamily::Compression, _))
        || s.gender != Gender::F
        || m.gender != Gender::M
        || misalign(&s, &m) > TOLERANCE_DEG
        || (s.mm / m.mm - 1.).abs() > 1e-3
        || (!assembly(doc, id).contains(&target.piece)
            && fixed(doc, &assembly(doc, id))
            && fixed(doc, &assembly(doc, target.piece)))
    {
        return None;
    }
    let g = doc.graphics.iter().find(|g| g.id == id)?;
    let (d, values) = definition(g)?;
    let (length, _, ends) = straight_tube(d)?;
    if length != param || !ends.contains(&end) {
        return None;
    }
    let Value::Number(current) = values.get(param)? else {
        return None;
    };
    let opposite = ends.iter().find(|e| **e != end)?;
    let pinned = ports(g).into_iter().find(|p| p.at.joint == *opposite)?;
    let inverse = frame(g).inverse()?;
    let local = inverse.apply(m.point);
    let axis = normalize(sub(local, inverse.apply(pinned.point)));
    let mut candidate = doc.clone();
    let mut onto = s.clone();
    let mut added = None;
    let offer = if mates(&s, &m) {
        Offer::Seat
    } else {
        compression_reducer(&s, &m)?;
        let (d, near, far) = adapter(&s, &m)?;
        let adapter_id = candidate.next_id();
        let mut adapter = placed::graphic(adapter_id, d.clone(), Point::default(), s.mm as f32);
        adapter.layer = doc.graphics.iter().find(|g| g.id == target.piece)?.layer;
        let a = ports(&adapter).into_iter().find(|p| p.at.joint == near)?;
        candidate.graphics.push(adapter);
        apply(&mut candidate, &[adapter_id], &s, &a);
        candidate.seats.push(Seat::new(s.at.clone(), a.at.clone()));
        onto = port(
            &candidate,
            &JointRef {
                piece: adapter_id,
                joint: far,
            },
        )?;
        added = Some(adapter_id);
        Offer::Adapter(d)
    };
    // The length solver only seats joints within their current table cell.
    // Give the planned reducer its ownership before trying that attachment.
    if let Some(added) = added {
        crate::graphics::table::inherit_attachment(
            doc,
            &mut candidate,
            &[added],
            &[id, target.piece],
        )
        .ok()?;
    }
    let requested = current + dot(sub(inverse.apply(onto.contact()), local), axis);
    if !requested.is_finite() {
        return None;
    }
    set_end(&mut candidate, id, param, end, Value::Number(requested)).ok()?;
    // An already exact cut length has no moving ports for set_end's native
    // auto-seat pass. Explicit acceptance can still join its two free ends.
    if partner(&candidate, &moving).is_none()
        && partner(&candidate, &onto.at).is_none()
        && fits(&port(&candidate, &moving)?, &onto)
    {
        candidate
            .seats
            .push(Seat::new(onto.at.clone(), moving.clone()));
        mark(&mut candidate);
    }
    let (_, values) = definition(candidate.graphics.iter().find(|g| g.id == id)?)?;
    let Value::Number(actual) = values.get(param)? else {
        return None;
    };
    if (actual - requested).abs() > 1e-6
        || !fits(&port(&candidate, &moving)?, &onto)
        || !candidate
            .seats
            .iter()
            .any(|seat| seat.holds(&moving) && seat.holds(&onto.at))
        || !doc.seats.iter().all(|seat| candidate.seats.contains(seat))
        || candidate.graphics.len() != doc.graphics.len() + usize::from(added.is_some())
        || doc
            .graphics
            .iter()
            .zip(&candidate.graphics)
            .any(|(before, after)| {
                before.id != after.id
                    || (before.id != id
                        && (before.origin != after.origin
                            || before.axis_x != after.axis_x
                            || before.axis_y != after.axis_y
                            || before.piece.as_ref().map(|p| &p.params)
                                != after.piece.as_ref().map(|p| &p.params)))
            })
    {
        return None;
    }
    let now_pinned = port(&candidate, &pinned.at)?;
    if len(sub(pinned.point, now_pinned.point)) > TOLERANCE_MM * pinned.mm {
        return None;
    }
    crate::graphics::table::validate(&candidate).ok()?;
    let snap = end_snap(s, m, offer, added);
    Some((candidate, snap))
}

fn set_inner(
    doc: &mut Document,
    id: u64,
    param: &str,
    value: Value,
    pinned: Option<&str>,
) -> Result<Vec<Seat>, String> {
    let g = (doc.graphics.iter())
        .find(|g| g.id == id)
        .ok_or("The piece is not in the drawing.")?;
    let d = (g.piece.as_ref())
        .and_then(|p| p.linked.clone())
        .ok_or("Only an equipment piece has parameters.")?;
    let mut params = g
        .piece
        .as_ref()
        .map(|p| p.params.clone())
        .unwrap_or_default();
    params.insert(param.to_owned(), value);
    let resolved = resolve(&d, &params)?;
    if let Some(v) = resolved.get(param) {
        params.insert(param.to_owned(), v.clone());
    }
    let old = ports(g);
    let anchor = anchor(doc, &assembly(doc, id));
    let mut replacement = g.clone();
    if let Some(piece) = replacement.piece.as_mut() {
        piece.params = params;
    }
    if let Some(end) = pinned {
        let before = old
            .iter()
            .find(|p| p.at.joint == end)
            .ok_or("The opposite tube end is missing.")?;
        let after = ports(&replacement)
            .into_iter()
            .find(|p| p.at.joint == end)
            .ok_or("The opposite tube end is missing.")?;
        let delta = sub(before.point, after.point);
        replacement.origin.x += delta[0] as f32;
        replacement.origin.y += delta[1] as f32;
    }
    let new = ports(&replacement);
    if let Some(g) = doc.graphics.iter_mut().find(|g| g.id == id) {
        *g = replacement;
    }
    // How each port moved, by class.
    let mut motions = vec![Affine::IDENTITY];
    let mut class = BTreeMap::new();
    let mut changed = BTreeSet::new();
    for o in &old {
        let Some(n) = new.iter().find(|n| n.at == o.at) else {
            continue;
        };
        // A continuous span grows around its existing holders. They move
        // only when the whole tube moves, not when one cut end is edited.
        let m = if pinned.is_some_and(|end| o.at.joint == end)
            || (pinned.is_some() && o.hold() == Some(HoldKind::TubeSpan))
        {
            Affine::IDENTITY
        } else {
            motion(o, n)
        };
        let k = match motions.iter().position(|x| same(x, &m)) {
            Some(k) => k,
            None => {
                motions.push(m);
                motions.len() - 1
            }
        };
        if k != 0
            || (pinned.is_some()
                && o.hold() == Some(HoldKind::TubeSpan)
                && (len(sub(o.point, n.point)) > 1e-6 || (o.length - n.length).abs() > 1e-6))
        {
            changed.insert(n.at.clone());
        }
        class.insert(n.at.clone(), k);
    }
    changed.extend(
        new.iter()
            .filter(|n| !old.iter().any(|o| o.at == n.at))
            .map(|n| n.at.clone()),
    );
    let gone: Vec<Seat> = (doc.seats.iter())
        .filter(|s| {
            (s.a.piece == id && !class.contains_key(&s.a))
                || (s.b.piece == id && !class.contains_key(&s.b))
        })
        .cloned()
        .collect();
    release(doc, |s| gone.contains(s));
    let mut released = gone;
    while let Some(c) = components(doc, id).into_iter().find(|c| {
        let classes = touching(doc, id, c, &class);
        classes.len() > 1 || (pinned.is_some() && fixed(doc, c) && classes.iter().any(|k| *k != 0))
    }) {
        let Some(s) = choose(doc, id, &c, &class, pinned.is_some()) else {
            break;
        };
        release(doc, |x| *x == s);
        released.push(s);
    }
    let parts: Vec<(BTreeSet<u64>, usize)> = (components(doc, id).into_iter())
        .map(|c| {
            let k = touching(doc, id, &c, &class)
                .into_iter()
                .next()
                .unwrap_or(0);
            (c, k)
        })
        .collect();
    // The anchor stays: when it would move, everything moves back by its motion.
    let back = (parts.iter())
        .find(|(c, _)| pinned.is_none() && anchor.is_some_and(|a| c.contains(&a)))
        .and_then(|(_, k)| motions.get(*k))
        .and_then(Affine::inverse)
        .unwrap_or(Affine::IDENTITY);
    if !same(&back, &Affine::IDENTITY) {
        reframe(doc, &BTreeSet::from([id]), back);
        // Every port of the piece moved on the page.
        changed.extend(new.iter().map(|n| n.at.clone()));
    }
    for (c, k) in &parts {
        let Some(&motion) = motions.get(*k) else {
            continue;
        };
        let t = back.then(motion);
        if !same(&t, &Affine::IDENTITY) {
            reframe(doc, c, t);
        }
    }
    // A port that moved or came back takes hold of a free one it now fits.
    let me = BTreeSet::from([id]);
    let outer = free(doc, &me, false);
    for a in free(doc, &me, true)
        .iter()
        .filter(|a| changed.contains(&a.at))
    {
        let taken =
            |doc: &Document, p: &Port| !p.shared() && doc.seats.iter().any(|s| s.holds(&p.at));
        let Some(b) = outer.iter().find(|b| {
            fits(a, b)
                && (pinned.is_none()
                    || crate::graphics::table::require_same_cell(doc, &[a.at.piece, b.at.piece])
                        .is_ok())
                && !taken(doc, b)
                && !(!assembly(doc, id).contains(&b.at.piece)
                    && fixed(doc, &assembly(doc, id))
                    && fixed(doc, &assembly(doc, b.at.piece)))
        }) else {
            continue;
        };
        if !taken(doc, a) {
            doc.seats.push(Seat::new(b.at.clone(), a.at.clone()));
            fit(doc, a, b);
        }
    }
    // A seat that a part's motion could not keep, such as a slider on a
    // shortened rail, lets go as well, as the commit's check would.
    let loose: Vec<Seat> = (doc.seats.iter())
        .filter(|s| s.has(id))
        .filter(|s| {
            !port(doc, &s.a)
                .zip(port(doc, &s.b))
                .is_some_and(|(a, b)| fits(&a, &b))
        })
        .cloned()
        .collect();
    release(doc, |s| loose.contains(s));
    released.extend(loose);
    clips(doc, &[]);
    mark(doc);
    Ok(released)
}
