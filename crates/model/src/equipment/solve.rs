//! Seats: which joints of placed pieces touch. Snapping makes them, every edit
//! and every read checks them with one tolerance, and the pieces they join
//! form an assembly that moves as one. Stored poses are never re-solved: a
//! seat that no longer fits is released and both pieces stay where they are.
use super::builtin;
use super::eval::{joints, resolve};
use super::format::{Category, Definition, Gender, Joint, PlugFamily, taper};
use super::geometry::{Affine, add, dot, len, mul, normalize, sub};
use super::placed;
use crate::document::{Document, Point};
use crate::graphics::Graphic;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

/// One joint of one placed piece.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointRef {
    pub piece: u64,
    pub joint: String,
}

/// Two joints seated in each other.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seat {
    pub a: JointRef,
    pub b: JointRef,
}

impl Seat {
    fn has(&self, piece: u64) -> bool {
        self.a.piece == piece || self.b.piece == piece
    }
    fn holds(&self, j: &JointRef) -> bool {
        self.a == *j || self.b == *j
    }
}

/// How far a seat may be off, in millimetres and degrees, when it is made,
/// after every edit and when a drawing is read.
pub const TOLERANCE_MM: f64 = 0.05;
pub const TOLERANCE_DEG: f64 = 0.2;
/// Snapping reaches joints within this many screen pixels at least …
pub const REACH_PX: f32 = 14.;
/// … and within the joint's own diameter, so big joints are easy to hit.
const REACH_DIAMETERS: f64 = 1.;
/// A drag snaps only when the axes are this close to opposite.
const REACH_DEG: f64 = 40.;

/// What a joint fits.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Taper(String),
    Plug(PlugFamily, f64),
}

/// A joint of a placed piece, in world units.
#[derive(Debug, Clone)]
pub struct Port {
    pub at: JointRef,
    pub kind: Kind,
    pub gender: Gender,
    pub point: [f64; 2],
    pub dir: [f64; 2],
    /// World units per millimetre.
    pub mm: f64,
}

impl Port {
    /// "15/25", or a plug's family.
    pub fn size(&self) -> String {
        match &self.kind {
            Kind::Taper(size) => size.clone(),
            Kind::Plug(family, _) => format!("{family:?}").to_lowercase(),
        }
    }
    /// Where the seated pieces touch: the male shoulder sits 0.06 L outside
    /// the female mouth.
    fn contact(&self) -> [f64; 2] {
        let exposure = match (&self.kind, self.gender) {
            (Kind::Taper(size), Gender::F) => taper(size).map_or(0., |t| t.exposure()),
            _ => 0.,
        };
        add(self.point, mul(self.dir, exposure * self.mm))
    }
    /// The joint's diameter in world units, for the snap ring.
    pub fn diameter(&self) -> f64 {
        let mm = match &self.kind {
            Kind::Taper(size) => taper(size).map_or(10., |t| t.d),
            Kind::Plug(_, size) => *size,
        };
        mm * self.mm
    }
}

fn frame(g: &Graphic) -> Affine {
    let f = |v: f32| f64::from(v);
    Affine([
        f(g.axis_x.x),
        f(g.axis_x.y),
        f(g.axis_y.x),
        f(g.axis_y.y),
        f(g.origin.x),
        f(g.origin.y),
    ])
}

/// Every joint of a placed piece; none for any other graphic.
pub fn ports(g: &Graphic) -> Vec<Port> {
    let Some((piece, d)) = g.piece.as_ref().and_then(|p| Some((p, p.linked.as_ref()?))) else {
        return Vec::new();
    };
    let Ok(values) = resolve(d, &piece.params) else {
        return Vec::new();
    };
    let t = frame(g);
    joints(d, &values)
        .into_iter()
        .map(|j| Port {
            at: JointRef {
                piece: g.id,
                joint: j.joint.id().to_owned(),
            },
            kind: match j.joint {
                Joint::Taper { size, .. } => Kind::Taper(size.clone()),
                Joint::Plug {
                    family, size_mm, ..
                } => Kind::Plug(*family, *size_mm),
            },
            gender: j.joint.gender(),
            point: t.apply(j.at),
            dir: t.apply_dir(j.dir),
            mm: t.scale_factor(),
        })
        .collect()
}

/// Same kind, opposite gender; tapers of one size, plugs within 10 %.
pub fn mates(a: &Port, b: &Port) -> bool {
    a.gender != b.gender
        && match (&a.kind, &b.kind) {
            (Kind::Taper(x), Kind::Taper(y)) => x == y,
            (Kind::Plug(f, x), Kind::Plug(g, y)) => f == g && (x - y).abs() <= 0.1 * x.max(*y),
            _ => false,
        }
}

fn angle_between(a: [f64; 2], b: [f64; 2]) -> f64 {
    dot(normalize(a), normalize(b))
        .clamp(-1., 1.)
        .acos()
        .to_degrees()
}

/// How far a seated pair is off: millimetres between the male shoulder and
/// where it belongs, and degrees between the male axis and the reversed
/// female axis. Pieces seated together share one scale.
pub fn residual(a: &Port, b: &Port) -> (f64, f64) {
    let (f, m) = if a.gender == Gender::F {
        (a, b)
    } else {
        (b, a)
    };
    let off = len(sub(m.point, f.contact())) / f.mm;
    let turn = angle_between(m.dir, mul(f.dir, -1.));
    let scale = (m.mm / f.mm - 1.).abs();
    (if scale > 1e-3 { f64::INFINITY } else { off }, turn)
}

fn fits(a: &Port, b: &Port) -> bool {
    let (off, turn) = residual(a, b);
    mates(a, b) && off <= TOLERANCE_MM && turn <= TOLERANCE_DEG
}

fn port(doc: &Document, j: &JointRef) -> Option<Port> {
    let g = doc.graphics.iter().find(|g| g.id == j.piece)?;
    ports(g).into_iter().find(|p| p.at == *j)
}

/// Drop seats that name a missing piece or joint, that a joint shares, or
/// that no longer fit; mark the seated joints so seated cones are drawn cut.
/// An assembly keeps one layer and one place in the stacking order: a piece
/// whose layer changed takes its assembly along.
pub fn check(doc: &mut Document, before: Option<&Document>) {
    let mut taken = BTreeSet::new();
    let seats = std::mem::take(&mut doc.seats);
    for seat in seats {
        let ok = match (port(doc, &seat.a), port(doc, &seat.b)) {
            (Some(a), Some(b)) => fits(&a, &b),
            _ => false,
        };
        if ok && !taken.contains(&seat.a) && !taken.contains(&seat.b) {
            taken.extend([seat.a.clone(), seat.b.clone()]);
            doc.seats.push(seat);
        }
    }
    let layer_before =
        |id: u64| before.and_then(|b| b.graphics.iter().find(|g| g.id == id).map(|g| g.layer));
    for members in assemblies(doc) {
        let layers: BTreeMap<u64, i32> = (doc.graphics.iter())
            .filter(|g| members.contains(&g.id))
            .map(|g| (g.id, g.layer))
            .collect();
        let first = doc.graphics.iter().find(|g| members.contains(&g.id));
        let Some(first_layer) = first.map(|g| g.layer) else {
            continue;
        };
        let layer = (layers.iter())
            .find(|(id, l)| layer_before(**id).is_some_and(|b| b != **l))
            .map_or(first_layer, |(_, l)| *l);
        for g in doc.graphics.iter_mut().filter(|g| members.contains(&g.id)) {
            g.layer = layer;
        }
        gather(doc, &members);
    }
    mark(doc);
}

/// Mark each piece's seated joints.
fn mark(doc: &mut Document) {
    let mut seated: BTreeMap<u64, BTreeSet<String>> = BTreeMap::new();
    for j in doc.seats.iter().flat_map(|s| [&s.a, &s.b]) {
        seated.entry(j.piece).or_default().insert(j.joint.clone());
    }
    for g in &mut doc.graphics {
        if let Some(piece) = g.piece.as_mut() {
            piece.seated = seated.remove(&g.id).unwrap_or_default();
        }
    }
}

/// Move `members` together in the drawing order, where the first of them is.
fn gather(doc: &mut Document, members: &BTreeSet<u64>) {
    let at: Vec<usize> = (doc.graphics.iter().enumerate())
        .filter(|(_, g)| members.contains(&g.id))
        .map(|(i, _)| i)
        .collect();
    let (Some(first), Some(last)) = (at.first(), at.last()) else {
        return;
    };
    if last - first + 1 == at.len() {
        return;
    }
    let ids: Vec<u64> = (doc.graphics.iter())
        .filter(|g| members.contains(&g.id))
        .map(|g| g.id)
        .collect();
    place(doc, &ids, *first);
}

/// Take the graphics `ids` out and put them back in this order at `index`
/// (counted before they were taken out).
fn place(doc: &mut Document, ids: &[u64], index: usize) {
    let before = (doc.graphics.iter().take(index))
        .filter(|g| ids.contains(&g.id))
        .count();
    let (mut taken, kept): (Vec<Graphic>, Vec<Graphic>) = std::mem::take(&mut doc.graphics)
        .into_iter()
        .partition(|g| ids.contains(&g.id));
    taken.sort_by_key(|g| ids.iter().position(|id| *id == g.id));
    doc.graphics = kept;
    let at = (index - before).min(doc.graphics.len());
    doc.graphics.splice(at..at, taken);
}

/// The pieces joined to `id` through seats, `id` included.
pub fn assembly(doc: &Document, id: u64) -> BTreeSet<u64> {
    let mut found = BTreeSet::from([id]);
    let mut pending = vec![id];
    while let Some(p) = pending.pop() {
        for s in doc.seats.iter().filter(|s| s.has(p)) {
            for q in [s.a.piece, s.b.piece] {
                if found.insert(q) {
                    pending.push(q);
                }
            }
        }
    }
    found
}

/// Every assembly of two or more pieces.
fn assemblies(doc: &Document) -> Vec<BTreeSet<u64>> {
    let mut out: Vec<BTreeSet<u64>> = Vec::new();
    for s in &doc.seats {
        if !out.iter().any(|a| a.contains(&s.a.piece)) {
            out.push(assembly(doc, s.a.piece));
        }
    }
    out
}

/// `ids` and every piece seated to one of them, in drawing order.
pub fn closure(doc: &Document, ids: &[u64]) -> Vec<u64> {
    if doc.seats.is_empty() {
        return ids.to_vec();
    }
    let mut all: BTreeSet<u64> = ids.iter().copied().collect();
    for id in ids {
        all.extend(assembly(doc, *id));
    }
    let mut out = ids.to_vec();
    out.extend(
        doc.graphics
            .iter()
            .filter(|g| all.contains(&g.id) && !ids.contains(&g.id))
            .map(|g| g.id),
    );
    out
}

/// The piece that stays when an assembly is pulled apart: the biggest vessel
/// (a piece that holds liquid), else the biggest piece.
fn anchor(doc: &Document, members: &BTreeSet<u64>) -> Option<u64> {
    let rank = |g: &Graphic| {
        let (lo, hi) = g.shape_bounds();
        let vessel = (g.piece.as_ref()?.linked.as_ref()).is_some_and(|d| !d.liquids.is_empty());
        Some((vessel, (hi.x - lo.x) * (hi.y - lo.y)))
    };
    doc.graphics
        .iter()
        .filter(|g| members.contains(&g.id))
        .max_by(|a, b| {
            let (a, b) = (rank(a), rank(b));
            a.map(|a| a.0)
                .cmp(&b.map(|b| b.0))
                .then(a.map_or(0., |a| a.1).total_cmp(&b.map_or(0., |b| b.1)))
        })
        .map(|g| g.id)
}

/// What an Alt-drag on the pieces `ids` pulls out: each piece, plus every
/// part of its assembly that reaches the anchor only through it. A drying
/// tube comes away with its condenser; the flask stays.
pub fn detached(doc: &Document, ids: &[u64]) -> Vec<u64> {
    let mut out = ids.to_vec();
    for &id in ids {
        let members = assembly(doc, id);
        if members.len() < 2 {
            continue;
        }
        let without = |doc: &Document, from: u64| -> BTreeSet<u64> {
            let mut found = BTreeSet::from([from]);
            let mut pending = vec![from];
            while let Some(p) = pending.pop() {
                for s in doc.seats.iter().filter(|s| s.has(p) && !s.has(id)) {
                    for q in [s.a.piece, s.b.piece] {
                        if found.insert(q) {
                            pending.push(q);
                        }
                    }
                }
            }
            found
        };
        let mut rest: Vec<BTreeSet<u64>> = Vec::new();
        for &m in members.iter().filter(|m| **m != id) {
            if !rest.iter().any(|c| c.contains(&m)) {
                rest.push(without(doc, m));
            }
        }
        let anchor = anchor(doc, &members).filter(|a| *a != id).or_else(|| {
            let largest = rest.iter().max_by_key(|c| c.len())?;
            anchor(doc, largest)
        });
        for m in rest
            .iter()
            .filter(|c| anchor.is_none_or(|a| !c.contains(&a)))
            .flatten()
        {
            if !out.contains(m) {
                out.push(*m);
            }
        }
    }
    out
}

/// What snapping offers at a joint.
#[derive(Debug, Clone)]
pub enum Offer {
    /// The joints fit.
    Seat,
    /// They fit through this adapter, inserted between them.
    Adapter(Arc<Definition>),
    /// Nothing fits.
    Mismatch,
}

/// A snap found while dragging or placing, and what it changed.
#[derive(Debug, Clone)]
pub struct Snap {
    /// The joint snapped to, on the piece that stays.
    pub target: Port,
    /// The moving joint.
    pub moving: Port,
    pub offer: Offer,
    /// The moving pieces changed size to match.
    pub scaled: bool,
    /// The moving pieces changed sides of the structures: `Some(true)` when
    /// they now paint behind them.
    pub behind: Option<bool>,
    /// The inserted adapter.
    pub added: Option<u64>,
}

/// The turn in degrees and the scale that seat `m` on `s`: axes opposite,
/// one scale, and the two contact points on each other.
fn seating(s: &Port, m: &Port) -> (f64, f64) {
    let want = mul(s.dir, -1.);
    let turn = (want[1].atan2(want[0]) - m.dir[1].atan2(m.dir[0])).to_degrees();
    (turn, s.mm / m.mm)
}

/// Turn and scale `ids` about `m`'s contact point and move it onto `s`'s:
/// pieces exactly, other objects (a grouped caption) as a turn would.
fn apply(doc: &mut Document, ids: &[u64], s: &Port, m: &Port) {
    let (turn, k) = seating(s, m);
    let from = m.contact();
    let shift = sub(s.contact(), from);
    let t = Affine::translate(s.contact())
        .then(Affine::rotate(turn, [0., 0.]))
        .then(Affine::scale(k, [0., 0.]))
        .then(Affine::translate(mul(from, -1.)));
    let [a, b, c, d, ..] = t.0;
    let p = |v: [f64; 2]| Point::new(v[0] as f32, v[1] as f32);
    let linear = |v: Point| {
        let (x, y) = (f64::from(v.x), f64::from(v.y));
        p([a * x + c * y, b * x + d * y])
    };
    for g in doc.graphics.iter_mut().filter(|g| ids.contains(&g.id)) {
        if g.piece.is_some() {
            g.origin = p(t.apply([f64::from(g.origin.x), f64::from(g.origin.y)]));
            g.axis_x = linear(g.axis_x);
            g.axis_y = linear(g.axis_y);
        }
    }
    let others: Vec<u64> = ids
        .iter()
        .copied()
        .filter(|id| {
            doc.graphics
                .iter()
                .all(|g| g.id != *id || g.piece.is_none())
        })
        .collect();
    if !others.is_empty() {
        let pivot = Point::new(from[0] as f32, from[1] as f32);
        crate::editing::transform_about(doc, &others, pivot, k as f32, turn as f32);
        doc.translate(&others, shift[0] as f32, shift[1] as f32);
    }
}

/// Free joints of the pieces in (or not in) `moving`.
fn free(doc: &Document, moving: &BTreeSet<u64>, inside: bool) -> Vec<Port> {
    let seated: BTreeSet<&JointRef> = doc.seats.iter().flat_map(|s| [&s.a, &s.b]).collect();
    doc.graphics
        .iter()
        .filter(|g| g.piece.is_some() && moving.contains(&g.id) == inside)
        .flat_map(ports)
        .filter(|p| !seated.contains(&p.at))
        .collect()
}

/// The first catalogue adapter with one joint that mates `s` and another
/// that mates `m`: which joint goes where.
fn adapter(s: &Port, m: &Port) -> Option<(Arc<Definition>, String, String)> {
    let probe = |j: &Joint, like: &Port| Port {
        kind: match j {
            Joint::Taper { size, .. } => Kind::Taper(size.clone()),
            Joint::Plug {
                family, size_mm, ..
            } => Kind::Plug(*family, *size_mm),
        },
        gender: j.gender(),
        ..like.clone()
    };
    builtin::catalogue()
        .iter()
        .filter(|d| d.category == Category::Adapter)
        .find_map(|d| {
            d.joints.iter().find_map(|a| {
                let b = d
                    .joints
                    .iter()
                    .find(|b| b.id() != a.id() && mates(&probe(b, m), m))?;
                mates(&probe(a, s), s).then(|| (d.clone(), a.id().to_owned(), b.id().to_owned()))
            })
        })
}

/// Seat the moving pieces on `s` through their joint `m`: directly, or
/// through an inserted adapter. Further joints that now fit are seated too,
/// and the moving pieces join the target's place in the stacking order.
fn seat(doc: &mut Document, moving: &[u64], s: &Port, m: &Port, offer: &Offer) -> Snap {
    let mut ids: Vec<u64> = moving.to_vec();
    let mut snap = Snap {
        target: s.clone(),
        moving: m.clone(),
        offer: offer.clone(),
        scaled: (m.mm / s.mm - 1.).abs() > 1e-3,
        behind: None,
        added: None,
    };
    let static_set = assembly(doc, s.at.piece);
    let target_layer = (doc.graphics.iter())
        .find(|g| g.id == s.at.piece)
        .map_or(-1, |g| g.layer);
    let mut onto = s.clone();
    if let Offer::Adapter(d) = offer
        && let Some((_, near, far)) = adapter(s, m)
    {
        let id = doc.next_id();
        let mut g = placed::graphic(id, d.clone(), Point::default(), s.mm as f32);
        g.layer = target_layer;
        let joints = ports(&g);
        let (Some(a), Some(b)) = (
            joints.iter().find(|p| p.at.joint == near),
            joints.iter().find(|p| p.at.joint == far),
        ) else {
            return snap;
        };
        doc.graphics.push(g);
        apply(doc, &[id], s, a);
        doc.seats.push(Seat {
            a: s.at.clone(),
            b: a.at.clone(),
        });
        onto = port(doc, &b.at).unwrap_or_else(|| b.clone());
        snap.added = Some(id);
        ids.insert(0, id);
    }
    apply(doc, moving, &onto, m);
    doc.seats.push(Seat {
        a: onto.at.clone(),
        b: m.at.clone(),
    });
    // Close every other joint that now fits, such as a second neck.
    let moved: BTreeSet<u64> = ids.iter().copied().collect();
    let inner = free(doc, &moved, true);
    let outer = free(doc, &moved, false);
    for a in &inner {
        if let Some(b) = outer
            .iter()
            .find(|b| fits(a, b) && !doc.seats.iter().any(|s| s.holds(&b.at)))
            && !doc.seats.iter().any(|s| s.holds(&a.at))
        {
            doc.seats.push(Seat {
                a: b.at.clone(),
                b: a.at.clone(),
            });
        }
    }
    // The merged assembly takes the target's layer, and the pieces that
    // joined it (the adapter first) follow it in the drawing order.
    let joined = assembly(doc, s.at.piece);
    let mut order: Vec<u64> = (doc.graphics.iter())
        .filter(|g| joined.contains(&g.id) && !static_set.contains(&g.id))
        .filter(|g| Some(g.id) != snap.added)
        .map(|g| g.id)
        .collect();
    order.splice(0..0, snap.added);
    for g in doc.graphics.iter_mut().filter(|g| order.contains(&g.id)) {
        if (g.layer < 0) != (target_layer < 0) {
            snap.behind = Some(target_layer < 0);
        }
        g.layer = target_layer;
    }
    let end = (doc.graphics.iter())
        .rposition(|g| static_set.contains(&g.id))
        .map_or(doc.graphics.len(), |i| i + 1);
    place(doc, &order, end);
    mark(doc);
    snap
}

/// After the pieces `ids` moved: release their seats to pieces that stayed,
/// then snap them to the best joint within `reach` world units of one of
/// their free joints. A compatible joint seats; a mismatch inserts the
/// adapter that fits, or only says why nothing fits. `any_angle` lets a new
/// piece turn to fit; a dragged one must already point within 40°.
pub fn snap(doc: &mut Document, ids: &[u64], reach: f32, any_angle: bool) -> Option<Snap> {
    let moving: BTreeSet<u64> = ids.iter().copied().collect();
    if !doc
        .graphics
        .iter()
        .any(|g| g.piece.is_some() && moving.contains(&g.id))
    {
        return None;
    }
    let before = doc.seats.len();
    doc.seats
        .retain(|s| moving.contains(&s.a.piece) == moving.contains(&s.b.piece));
    if doc.seats.len() != before {
        mark(doc);
    }
    let inner = free(doc, &moving, true);
    let outer = free(doc, &moving, false);
    let mut best: Option<(bool, f64, &Port, &Port)> = None;
    for m in &inner {
        for s in &outer {
            let reach = f64::from(reach).max(REACH_DIAMETERS * s.diameter());
            let off = len(sub(m.point, s.contact()));
            let turn = angle_between(m.dir, mul(s.dir, -1.));
            if off > reach || (!any_angle && turn > REACH_DEG) {
                continue;
            }
            let fit = mates(m, s);
            let score = off / reach * 12. + 0.3 * turn;
            // A joint that fits wins over a closer one that does not.
            if best.is_none_or(|(f, b, ..)| (fit, -score) > (f, -b)) {
                best = Some((fit, score, s, m));
            }
        }
    }
    let (fit, _, s, m) = best?;
    let (s, m) = (s.clone(), m.clone());
    let offer = if fit {
        Offer::Seat
    } else if let Some((d, ..)) = adapter(&s, &m) {
        Offer::Adapter(d)
    } else {
        return Some(Snap {
            target: s,
            moving: m,
            offer: Offer::Mismatch,
            scaled: false,
            behind: None,
            added: None,
        });
    };
    Some(seat(doc, ids, &s, &m, &offer))
}

/// Joints the selected pieces could be connected to, nearest first: each a
/// free joint of the selection and a free joint elsewhere that fits it,
/// directly or through an adapter.
pub fn options(doc: &Document, ids: &[u64]) -> Vec<(Port, Port, Offer)> {
    let moving: BTreeSet<u64> = closure(doc, ids).into_iter().collect();
    let inner = free(doc, &moving, true);
    let outer = free(doc, &moving, false);
    let mut out: Vec<(f64, Port, Port, Offer)> = Vec::new();
    for m in &inner {
        for s in &outer {
            let offer = if mates(m, s) {
                Offer::Seat
            } else if let Some((d, ..)) = adapter(s, m) {
                Offer::Adapter(d)
            } else {
                continue;
            };
            out.push((len(sub(m.point, s.contact())), s.clone(), m.clone(), offer));
        }
    }
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    out.into_iter().map(|(_, s, m, o)| (s, m, o)).collect()
}

/// Connect the pieces `ids` through joint `m` to the fixed joint `s`, as a
/// snap would. `None` when either joint is gone or taken.
pub fn connect(doc: &mut Document, ids: &[u64], s: &JointRef, m: &JointRef) -> Option<Snap> {
    let moving = closure(doc, ids);
    let (s, m) = (port(doc, s)?, port(doc, m)?);
    if doc.seats.iter().any(|x| x.holds(&s.at) || x.holds(&m.at)) {
        return None;
    }
    let offer = if mates(&s, &m) {
        Offer::Seat
    } else {
        Offer::Adapter(adapter(&s, &m)?.0)
    };
    Some(seat(doc, &moving, &s, &m, &offer))
}

/// Release the seat at joint `j` and pull the side without the anchor out
/// along the joint's axis, by the joint's length, so the two read as apart.
pub fn detach(doc: &mut Document, j: &JointRef) -> bool {
    let Some(i) = doc.seats.iter().position(|s| s.holds(j)) else {
        return false;
    };
    let seat = doc.seats.remove(i);
    let members: BTreeSet<u64> = assembly(doc, seat.a.piece)
        .union(&assembly(doc, seat.b.piece))
        .copied()
        .collect();
    let anchor = anchor(doc, &members);
    let (stay, go) = if anchor.is_some_and(|a| assembly(doc, seat.a.piece).contains(&a)) {
        (&seat.a, &seat.b)
    } else {
        (&seat.b, &seat.a)
    };
    if let (Some(s), Some(g)) = (port(doc, stay), port(doc, go)) {
        let away = match &s.kind {
            Kind::Taper(size) => taper(size).map_or(10., |t| t.l),
            Kind::Plug(_, size) => *size,
        } * s.mm;
        let shift = mul(normalize(s.dir), away);
        let pulled: Vec<u64> = assembly(doc, g.at.piece).into_iter().collect();
        doc.translate(&pulled, shift[0] as f32, shift[1] as f32);
    }
    mark(doc);
    true
}

/// A seated joint's partner.
pub fn partner(doc: &Document, j: &JointRef) -> Option<JointRef> {
    doc.seats.iter().find_map(|s| {
        if s.a == *j {
            Some(s.b.clone())
        } else if s.b == *j {
            Some(s.a.clone())
        } else {
            None
        }
    })
}

/// Drop the seats of deleted pieces.
pub(crate) fn prune(doc: &mut Document) {
    let before = doc.seats.len();
    let ids: BTreeSet<u64> = doc.graphics.iter().map(|g| g.id).collect();
    doc.seats
        .retain(|s| ids.contains(&s.a.piece) && ids.contains(&s.b.piece));
    if doc.seats.len() != before {
        mark(doc);
    }
}
