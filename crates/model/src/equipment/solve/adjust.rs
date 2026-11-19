//! Movable parts. Setting a parameter moves a piece's parts, and with them
//! what is seated on those parts: a clamp's head tilts the flask it holds, an
//! opened jaw lets go. Handles on the canvas set parameters by dragging a
//! part or a liquid's surface, or by clicking a part that has states.
use super::*;
use crate::equipment::eval::{Values, part_transforms};
use crate::equipment::format::{Gesture, Param, Unit};
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
            })
        })
        .collect();
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
            });
        }
    }
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
    let (d, mut values) = definition(g)?;
    let (lo, hi, step) = range(d.params.iter().find(|p| p.id() == handle.param)?)?;
    let round = |v: f64| ((v / step).round() * step).clamp(lo, hi);
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

/// The rigid motion that takes the pose (`point`, `dir`) of `from` to that of `to`.
fn motion(from: &Port, to: &Port) -> Affine {
    let angle = |v: [f64; 2]| v[1].atan2(v[0]).to_degrees();
    Affine::translate(to.point)
        .then(Affine::rotate(angle(to.dir) - angle(from.dir), [0., 0.]))
        .then(Affine::translate(mul(from.point, -1.)))
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
) -> Option<Seat> {
    // Nodes: the component's pieces, then one per class of `id`'s ports.
    let pieces: Vec<u64> = c.iter().copied().collect();
    let classes: Vec<usize> = touching(doc, id, c, class).into_iter().collect();
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
    let virtual_edges = (piece_count..n).flat_map(|u| (u + 1..n).map(move |v| (u, v)));
    let edges = seats.iter().map(|&(edge, _)| edge).chain(virtual_edges);
    let comp = biconnected(n, edges);
    let between: BTreeSet<usize> = comp
        .iter()
        .skip(seats.len())
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
    if let Some(piece) = (doc.graphics.iter_mut())
        .find(|g| g.id == id)
        .and_then(|g| g.piece.as_mut())
    {
        piece.params = params;
    }
    let new = (doc.graphics.iter())
        .find(|g| g.id == id)
        .map(ports)
        .unwrap_or_default();
    // How each port moved, by class.
    let mut motions = vec![Affine::IDENTITY];
    let mut class = BTreeMap::new();
    let mut changed = BTreeSet::new();
    for o in &old {
        let Some(n) = new.iter().find(|n| n.at == o.at) else {
            continue;
        };
        let m = motion(o, n);
        let k = match motions.iter().position(|x| same(x, &m)) {
            Some(k) => k,
            None => {
                motions.push(m);
                motions.len() - 1
            }
        };
        if k != 0 {
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
    while let Some(c) = components(doc, id)
        .into_iter()
        .find(|c| touching(doc, id, c, &class).len() > 1)
    {
        let Some(s) = choose(doc, id, &c, &class) else {
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
        .find(|(c, _)| anchor.is_some_and(|a| c.contains(&a)))
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
