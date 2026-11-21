//! Seats join the joints and holds of placed pieces. Snapping makes them,
//! every edit and every read checks them with one tolerance, and the pieces
//! they join form an assembly that moves as one. Stored poses are never
//! re-solved: a seat that no longer fits is released and both pieces stay
//! where they are. Only a seat's clip follows it, and a grounded piece such
//! as a stand stays put while what it holds slides along its rod.
use super::builtin;
use super::eval::{Value, holds, joints, resolve};
use super::format::{Category, Definition, Gender, HoldKind, Joint, PlugFamily, taper};
use super::geometry::{Affine, add, dot, len, mul, normalize, perp, sub};
use super::placed;
use crate::document::{Document, Point};
use crate::graphics::Graphic;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

pub mod adjust;
pub mod hose;

/// One joint or hold of one placed piece.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointRef {
    pub piece: u64,
    pub joint: String,
}

/// Two joints seated in each other, or a hold on what it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Seat {
    pub a: JointRef,
    pub b: JointRef,
    /// The joint clip on a ground-glass seat: a placed clip piece.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clip: Option<u64>,
}

impl Seat {
    fn new(a: JointRef, b: JointRef) -> Self {
        Self { a, b, clip: None }
    }
    fn has(&self, piece: u64) -> bool {
        self.a.piece == piece || self.b.piece == piece
    }
    /// The seat joins `piece`, or `piece` is its clip.
    fn touches(&self, piece: u64) -> bool {
        self.has(piece) || self.clip == Some(piece)
    }
    fn pieces(&self) -> impl Iterator<Item = u64> + use<> {
        [self.a.piece, self.b.piece].into_iter().chain(self.clip)
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

/// What a joint or hold fits.
#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Taper(String),
    Plug(PlugFamily, f64),
    /// A segment to grip or slide along, and the diameters it takes in mm.
    Hold(HoldKind, [f64; 2]),
}

/// A joint or hold of a placed piece, in world units. A female port receives:
/// a socket, a rail, a neck to grip. A hold runs from `point` along the unit
/// `dir` for `length`.
#[derive(Debug, Clone)]
pub struct Port {
    pub at: JointRef,
    pub kind: Kind,
    pub gender: Gender,
    pub point: [f64; 2],
    pub dir: [f64; 2],
    /// World units per millimetre.
    pub mm: f64,
    /// A hold's length in world units; 0 for a joint.
    pub length: f64,
}

impl Port {
    /// "15/25", a plug's family, or a hold's diameter.
    pub fn size(&self) -> String {
        match &self.kind {
            Kind::Taper(size) => size.clone(),
            Kind::Plug(family, _) => format!("{family:?}").to_lowercase(),
            Kind::Hold(_, [lo, hi]) if lo == hi => format!("Ø{lo} mm"),
            Kind::Hold(_, [lo, hi]) => format!("Ø{lo}–{hi} mm"),
        }
    }
    pub fn hold(&self) -> Option<HoldKind> {
        match self.kind {
            Kind::Hold(kind, _) => Some(kind),
            _ => None,
        }
    }
    /// A rail takes any number of sliders; every other port one partner.
    pub fn shared(&self) -> bool {
        self.hold() == Some(HoldKind::Rail)
    }
    /// The middle of a hold.
    fn centre(&self) -> [f64; 2] {
        add(self.point, mul(self.dir, self.length / 2.))
    }
    /// The point of a hold nearest `p`.
    fn nearest(&self, p: [f64; 2]) -> [f64; 2] {
        let t = dot(sub(p, self.point), self.dir).clamp(0., self.length);
        add(self.point, mul(self.dir, t))
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
            Kind::Hold(_, [_, hi]) => *hi,
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

/// Every joint and hold of a placed piece; none for any other graphic.
pub fn ports(g: &Graphic) -> Vec<Port> {
    let Some((piece, d)) = g.piece.as_ref().and_then(|p| Some((p, p.linked.as_ref()?))) else {
        return Vec::new();
    };
    let Ok(values) = resolve(d, &piece.params) else {
        return Vec::new();
    };
    let t = frame(g);
    let at = |joint: &str| JointRef {
        piece: g.id,
        joint: joint.to_owned(),
    };
    let mut out: Vec<Port> = joints(d, &values)
        .into_iter()
        .map(|j| Port {
            at: at(j.joint.id()),
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
            length: 0.,
        })
        .collect();
    // A hose's ends are where its centre line ends.
    if let (Some(tube), Some(line)) = (&d.tube, placed::centre_line(d, piece)) {
        for (k, end) in tube.ends.iter().enumerate() {
            if let Some(p) = out.iter_mut().find(|p| p.at.joint == *end) {
                let (at, dir) = placed::end(&line, k);
                (p.point, p.dir) = (t.apply(at), t.apply_dir(dir));
            }
        }
    }
    out.extend(holds(d, &values).into_iter().filter_map(|h| {
        let (from, to) = (t.apply(h.from), t.apply(h.to));
        let length = len(sub(to, from));
        // A part's motion can squash a hold to nothing; it holds nothing then.
        let d = sub(to, from);
        (length.is_finite()
            && length > 0.
            && (d[0] / length).is_finite()
            && (d[1] / length).is_finite())
        .then(|| Port {
            at: at(&h.hold.id),
            kind: Kind::Hold(h.hold.kind, h.hold.diameter_mm.range()),
            gender: if h.hold.kind.held() {
                Gender::F
            } else {
                Gender::M
            },
            point: from,
            dir: [d[0] / length, d[1] / length],
            mm: t.scale_factor(),
            length,
        })
    }));
    out
}

/// Same kind, opposite gender; tapers of one size, plugs within 10 %; a
/// slider on a rail and a jaw on a grip whose diameter is in the jaw's range.
pub fn mates(a: &Port, b: &Port) -> bool {
    a.gender != b.gender
        && match (&a.kind, &b.kind) {
            (Kind::Taper(x), Kind::Taper(y)) => x == y,
            (Kind::Plug(f, x), Kind::Plug(g, y)) => f == g && (x - y).abs() <= 0.1 * x.max(*y),
            (Kind::Hold(x, dx), Kind::Hold(y, dy)) if x.mate() == *y => {
                let (held, holder) = if x.held() { (dx, dy) } else { (dy, dx) };
                let d = (held[0] + held[1]) / 2.;
                (holder[0]..=holder[1]).contains(&d)
            }
            _ => false,
        }
}

/// Both are holds, or both are joints: only these can ever be paired.
fn alike(a: &Port, b: &Port) -> bool {
    a.hold().is_some() == b.hold().is_some()
}

/// The female and male of a pair.
fn sides<'p>(a: &'p Port, b: &'p Port) -> (&'p Port, &'p Port) {
    if a.gender == Gender::F {
        (a, b)
    } else {
        (b, a)
    }
}

fn angle_between(a: [f64; 2], b: [f64; 2]) -> f64 {
    dot(normalize(a), normalize(b))
        .clamp(-1., 1.)
        .acos()
        .to_degrees()
}

/// Degrees between the axes of a pair as seated: a cone points into its
/// socket, while a hold lies along what it holds, either way round.
fn misalign(a: &Port, b: &Port) -> f64 {
    if a.hold().is_some() {
        let turn = angle_between(a.dir, b.dir);
        turn.min(180. - turn)
    } else {
        angle_between(a.dir, mul(b.dir, -1.))
    }
}

/// How far a seated pair is off: millimetres between the male shoulder and
/// where it belongs, or between a jaw or slider and what it holds, and
/// degrees between the axes as seated. Pieces seated together share one scale.
pub fn residual(a: &Port, b: &Port) -> (f64, f64) {
    let (f, m) = sides(a, b);
    let off = if f.hold().is_some() {
        len(sub(m.centre(), f.nearest(m.centre())))
    } else {
        len(sub(m.point, f.contact()))
    } / f.mm;
    let scale = (m.mm / f.mm - 1.).abs();
    (
        if scale > 1e-3 { f64::INFINITY } else { off },
        misalign(a, b),
    )
}

/// How far apart two ports are for snapping: from the female contact point
/// to the male cone, shoulder to tip, or from a jaw or slider to what it
/// holds. A cone whose tip reaches the mouth, or is already inside it,
/// counts as close.
fn gap(a: &Port, b: &Port) -> f64 {
    let (f, m) = sides(a, b);
    if f.hold().is_some() {
        return len(sub(m.centre(), f.nearest(m.centre())));
    }
    let length = match (&m.kind, m.gender) {
        (Kind::Taper(size), Gender::M) => taper(size).map_or(0., |t| t.l) * m.mm,
        _ => 0.,
    };
    let p = f.contact();
    let along = dot(sub(p, m.point), m.dir).clamp(0., length);
    len(sub(p, add(m.point, mul(m.dir, along))))
}

fn fits(a: &Port, b: &Port) -> bool {
    let (off, turn) = residual(a, b);
    mates(a, b) && off <= TOLERANCE_MM && turn <= TOLERANCE_DEG
}

fn grounded(doc: &Document, id: u64) -> bool {
    (doc.graphics.iter())
        .find(|g| g.id == id)
        .and_then(|g| g.piece.as_ref()?.linked.as_ref())
        .is_some_and(|d| d.grounded)
}

fn port(doc: &Document, j: &JointRef) -> Option<Port> {
    let g = doc.graphics.iter().find(|g| g.id == j.piece)?;
    ports(g).into_iter().find(|p| p.at == *j)
}

/// Drop seats that name a missing piece or joint, that a joint shares, or
/// that no longer fit, with their clips; put every other clip on its seat;
/// mark the seated joints so seated cones are drawn cut. An assembly keeps
/// one layer and one place in the stacking order: a piece whose layer
/// changed takes its assembly along.
pub fn check(doc: &mut Document, before: Option<&Document>) {
    let mut taken = BTreeSet::new();
    let mut dropped = Vec::new();
    for seat in std::mem::take(&mut doc.seats) {
        let ok = port(doc, &seat.a)
            .zip(port(doc, &seat.b))
            .is_some_and(|(a, b)| {
                fits(&a, &b)
                    && [&a, &b]
                        .iter()
                        .all(|p| p.shared() || !taken.contains(&p.at))
            });
        if ok {
            taken.extend([seat.a.clone(), seat.b.clone()]);
            doc.seats.push(seat);
        } else {
            dropped.extend(seat.clip.filter(|c| owns(doc, &seat, *c)));
        }
    }
    clips(doc, &dropped);
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

/// Where a clip sits on a ground-glass seat, as a piece frame: the female
/// mouth at the origin and +y into the socket, a millimetre long; and the size.
fn clip_frame(doc: &Document, seat: &Seat) -> Option<(String, [Point; 3])> {
    let (a, b) = (port(doc, &seat.a)?, port(doc, &seat.b)?);
    let (f, _) = sides(&a, &b);
    let Kind::Taper(size) = &f.kind else {
        return None;
    };
    let y = mul(normalize(f.dir), -f.mm);
    let p = |v: [f64; 2]| Point::new(v[0] as f32, v[1] as f32);
    Some((size.clone(), [p(f.point), p([y[1], -y[0]]), p(y)]))
}

/// The clip `c` is a clip of the size of the seat's joints, so it goes with it.
fn owns(doc: &Document, seat: &Seat, c: u64) -> bool {
    let size = [&seat.a, &seat.b]
        .into_iter()
        .filter_map(|j| port(doc, j))
        .find_map(|p| match p.kind {
            Kind::Taper(size) => Some(size),
            _ => None,
        });
    let clip = doc.graphics.iter().find(|g| g.id == c).and_then(clip_size);
    size.is_some_and(|s| clip == Some(s.as_str()))
}

fn clip_size(g: &Graphic) -> Option<&str> {
    Some(
        g.piece
            .as_ref()?
            .linked
            .as_ref()?
            .clip
            .as_ref()?
            .size
            .as_str(),
    )
}

/// Remove the clips `dropped` with their seats, and put every other clip on
/// its seat; a clip that is gone, taken or of another size comes off it.
fn clips(doc: &mut Document, dropped: &[u64]) {
    let frames: Vec<_> = (doc.seats.iter())
        .map(|s| s.clip.and_then(|_| clip_frame(doc, s)))
        .collect();
    let mut used = BTreeSet::new();
    for (seat, frame) in doc.seats.iter_mut().zip(frames) {
        let Some(c) = seat.clip else {
            continue;
        };
        let g = doc.graphics.iter_mut().find(|g| g.id == c);
        match (frame, g) {
            (Some((size, [origin, x, y])), Some(g))
                if clip_size(g) == Some(&size) && used.insert(c) =>
            {
                (g.origin, g.axis_x, g.axis_y) = (origin, x, y);
            }
            _ => seat.clip = None,
        }
    }
    let gone: Vec<u64> = (dropped.iter())
        .filter(|c| !used.contains(*c))
        .copied()
        .collect();
    if (doc.graphics.iter()).any(|g| gone.contains(&g.id) && clip_size(g).is_some()) {
        doc.graphics
            .retain(|g| !gone.contains(&g.id) || clip_size(g).is_none());
        doc.prune_groups();
    }
}

/// Release the seats that `release` picks, and remove their clips.
fn release(doc: &mut Document, release: impl Fn(&Seat) -> bool) -> bool {
    let (gone, kept): (Vec<Seat>, Vec<Seat>) = std::mem::take(&mut doc.seats)
        .into_iter()
        .partition(|s| release(s));
    doc.seats = kept;
    if gone.is_empty() {
        return false;
    }
    let dropped: Vec<u64> = gone.iter().filter_map(|s| s.clip).collect();
    clips(doc, &dropped);
    mark(doc);
    true
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

/// The pieces joined to `id` through seats, with the seats' clips, `id` included.
pub fn assembly(doc: &Document, id: u64) -> BTreeSet<u64> {
    reach(doc, id, |_| true)
}

/// The pieces joined to `from` through the seats `through` allows.
fn reach(doc: &Document, from: u64, through: impl Fn(&Seat) -> bool) -> BTreeSet<u64> {
    let mut found = BTreeSet::from([from]);
    let mut pending = vec![from];
    while let Some(p) = pending.pop() {
        for s in doc.seats.iter().filter(|s| s.touches(p) && through(s)) {
            for q in s.pieces() {
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

/// The piece that stays when an assembly is pulled apart: a grounded piece
/// such as the stand, else the biggest vessel (a piece that holds liquid),
/// else the biggest piece.
fn anchor(doc: &Document, members: &BTreeSet<u64>) -> Option<u64> {
    let rank = |g: &Graphic| {
        let (lo, hi) = g.shape_bounds();
        let d = g.piece.as_ref()?.linked.as_ref()?;
        Some((
            d.grounded,
            !d.liquids.is_empty(),
            (hi.x - lo.x) * (hi.y - lo.y),
        ))
    };
    doc.graphics
        .iter()
        .filter(|g| members.contains(&g.id))
        .max_by(|a, b| {
            let (a, b) = (rank(a), rank(b));
            a.map(|a| (a.0, a.1))
                .cmp(&b.map(|b| (b.0, b.1)))
                .then(a.map_or(0., |a| a.2).total_cmp(&b.map_or(0., |b| b.2)))
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
        let mut rest: Vec<BTreeSet<u64>> = Vec::new();
        for &m in members.iter().filter(|m| **m != id) {
            if !rest.iter().any(|c| c.contains(&m)) {
                rest.push(reach(doc, m, |s| !s.has(id)));
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
    /// A joint clip goes on the seat.
    Clip,
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
    /// Where the seat is, in world units.
    pub at: [f64; 2],
}

/// The turn in degrees and the scale that seat `m` on `s`, and the point of
/// `m` that goes onto the point of `s`. Joints oppose their axes and meet at
/// their contact points; a hold lines its axis up with what it holds, the
/// shorter way round, and its middle goes onto it.
fn seating(s: &Port, m: &Port) -> (f64, f64, [f64; 2], [f64; 2]) {
    let angle = |v: [f64; 2]| v[1].atan2(v[0]).to_degrees();
    let k = s.mm / m.mm;
    if s.hold().is_none() {
        let turn = angle(mul(s.dir, -1.)) - angle(m.dir);
        return (turn, k, m.contact(), s.contact());
    }
    let turn = (angle(s.dir) - angle(m.dir) + 90.).rem_euclid(180.) - 90.;
    if m.gender == Gender::F {
        (turn, k, m.nearest(s.centre()), s.centre())
    } else {
        (turn, k, m.centre(), s.nearest(m.centre()))
    }
}

/// Turn and scale `ids` about the point of `m` that seats and move it onto
/// `s`: pieces exactly, other objects (a grouped caption) as a turn would.
fn apply(doc: &mut Document, ids: &[u64], s: &Port, m: &Port) {
    let (turn, k, from, to) = seating(s, m);
    let shift = sub(to, from);
    let t = Affine::translate(to)
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

/// A jaw or slider with a `fit` parameter closes on what it now holds: a
/// clamp's fingers meet the neck.
fn fit(doc: &mut Document, a: &Port, b: &Port) {
    let (f, m) = sides(a, b);
    let Kind::Hold(_, [lo, hi]) = f.kind else {
        return;
    };
    let Some(piece) = (doc.graphics.iter_mut())
        .find(|g| g.id == m.at.piece)
        .and_then(|g| g.piece.as_mut())
    else {
        return;
    };
    let Some(k) = (piece.linked.as_ref())
        .and_then(|d| d.holds.iter().find(|h| h.id == m.at.joint))
        .and_then(|h| h.fit.clone())
    else {
        return;
    };
    piece.params.insert(k, Value::Number((lo + hi) / 2.));
}

/// Free joints and holds of the pieces in (or not in) `moving`. A rail
/// stays free however many sliders it holds.
fn free(doc: &Document, moving: &BTreeSet<u64>, inside: bool) -> Vec<Port> {
    let seated: BTreeSet<&JointRef> = doc.seats.iter().flat_map(|s| [&s.a, &s.b]).collect();
    doc.graphics
        .iter()
        .filter(|g| g.piece.is_some() && moving.contains(&g.id) == inside)
        .flat_map(ports)
        .filter(|p| p.shared() || !seated.contains(&p.at))
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
        at: seating(s, m).3,
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
        doc.seats.push(Seat::new(s.at.clone(), a.at.clone()));
        onto = port(doc, &b.at).unwrap_or_else(|| b.clone());
        snap.added = Some(id);
        ids.insert(0, id);
    }
    apply(doc, moving, &onto, m);
    doc.seats.push(Seat::new(onto.at.clone(), m.at.clone()));
    fit(doc, &onto, m);
    // Close every other joint of the joined assembly that now fits, such as
    // a second neck; other selected pieces that happen to line up stay free.
    let moved: BTreeSet<u64> = ids.iter().copied().collect();
    let joined = assembly(doc, m.at.piece);
    let inner: Vec<Port> = free(doc, &moved, true)
        .into_iter()
        .filter(|p| joined.contains(&p.at.piece))
        .collect();
    let outer = free(doc, &moved, false);
    let taken = |doc: &Document, p: &Port| !p.shared() && doc.seats.iter().any(|s| s.holds(&p.at));
    // Two stands never become one assembly.
    let apart = |doc: &Document, a: &Port, b: &Port| {
        let (x, y) = (assembly(doc, a.at.piece), assembly(doc, b.at.piece));
        !x.contains(&b.at.piece) && fixed(doc, &x) && fixed(doc, &y)
    };
    for a in &inner {
        if let Some(b) = outer
            .iter()
            .find(|b| fits(a, b) && !taken(doc, b) && !apart(doc, a, b))
            && !taken(doc, a)
        {
            doc.seats.push(Seat::new(b.at.clone(), a.at.clone()));
            fit(doc, a, b);
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

/// The ground lock: when the moving pieces cannot move freely, because
/// they include a grounded piece such as a stand or are `held` on one's rod,
/// the target's side moves to them instead, so a stand stays where it is put
/// and a clamp stays on its rod. The pieces that move and the roles of the
/// two ports; `None` when the target is fixed too, or cannot take the scale.
fn ground_lock(
    doc: &Document,
    ids: &[u64],
    held: bool,
    s: Port,
    m: Port,
) -> Option<(Vec<u64>, Port, Port)> {
    let fixed = assembly(doc, s.at.piece);
    if !held && !ids.iter().any(|id| grounded(doc, *id)) {
        return Some((ids.to_vec(), s, m));
    }
    if self::fixed(doc, &fixed) || !scalable(doc, &fixed, m.mm / s.mm) {
        return None;
    }
    // A group can tie the target to the stand or to the moving pieces.
    let target = doc.expand_groups(&fixed.into_iter().collect::<Vec<_>>());
    let tied = (target.iter()).any(|id| grounded(doc, *id) || ids.contains(id));
    (!tied).then_some((target, m, s))
}

/// The pieces include a grounded one, such as a stand.
fn fixed(doc: &Document, pieces: &BTreeSet<u64>) -> bool {
    pieces.iter().any(|id| grounded(doc, *id))
}

/// After the pieces `ids` moved: release their seats to pieces that stayed,
/// unless they still fit (a clamp slid along a rod), then snap them to the
/// best joint or hold within `reach` world units of one of their own. A
/// compatible one seats; a joint mismatch inserts the adapter that fits, or
/// only says why nothing fits. `any_angle` lets a new piece turn to fit; a
/// dragged one must already point within 40°. A clip moved on its own goes
/// onto the nearest seat of its size.
pub fn snap(doc: &mut Document, ids: &[u64], reach: f32, any_angle: bool) -> Option<Snap> {
    let moving: BTreeSet<u64> = ids.iter().copied().collect();
    let pieces: Vec<&Graphic> = (doc.graphics.iter())
        .filter(|g| g.piece.is_some() && moving.contains(&g.id))
        .collect();
    if let [g] = pieces[..]
        && let Some(size) = clip_size(g)
    {
        let (id, size, at) = (g.id, size.to_owned(), placed::middle(g));
        return clip_on(doc, id, &size, at, reach);
    }
    if pieces.is_empty() {
        return None;
    }
    let cut: Vec<Seat> = (doc.seats.iter())
        .filter(|s| moving.contains(&s.a.piece) != moving.contains(&s.b.piece))
        .filter(|s| {
            !port(doc, &s.a)
                .zip(port(doc, &s.b))
                .is_some_and(|(a, b)| fits(&a, &b))
        })
        .cloned()
        .collect();
    release(doc, |s| cut.contains(s));
    // Seats that crossed and still fit hold the moving pieces in place.
    let held =
        (doc.seats.iter()).any(|s| moving.contains(&s.a.piece) != moving.contains(&s.b.piece));
    let inner = free(doc, &moving, true);
    // Pieces that cannot move freely only meet pieces that can come to them.
    let stuck = held || fixed(doc, &moving);
    let outer: Vec<Port> = (free(doc, &moving, false).into_iter())
        .filter(|s| !stuck || !fixed(doc, &assembly(doc, s.at.piece)))
        .collect();
    // A joint that fits wins, then one an adapter joins, then the closest.
    let mut best: Option<(u8, f64, &Port, &Port)> = None;
    for m in &inner {
        for s in outer.iter().filter(|s| alike(m, s)) {
            // Holds pair only with what they hold; a joint mismatch says why.
            if m.hold().is_some() && !mates(m, s) {
                continue;
            }
            let reach = f64::from(reach).max(REACH_DIAMETERS * s.diameter());
            let off = gap(m, s);
            let turn = misalign(m, s);
            // A pair whose scaling a moving piece cannot take is no choice.
            if off > reach
                || (!any_angle && turn > REACH_DEG)
                || !scalable(doc, &moving, s.mm / m.mm)
            {
                continue;
            }
            let tier = if mates(m, s) {
                2
            } else {
                u8::from(adapter(s, m).is_some())
            };
            let score = off / reach * 12. + 0.3 * turn;
            if best.is_none_or(|(t, b, ..)| (tier, -score) > (t, -b)) {
                best = Some((tier, score, s, m));
            }
        }
    }
    let (_, _, s, m) = best?;
    let (ids, s, m) = ground_lock(doc, ids, held, s.clone(), m.clone())?;
    let offer = if mates(&s, &m) {
        Offer::Seat
    } else if let Some((d, ..)) = adapter(&s, &m) {
        Offer::Adapter(d)
    } else {
        return Some(Snap {
            at: s.point,
            target: s,
            moving: m,
            offer: Offer::Mismatch,
            scaled: false,
            behind: None,
            added: None,
        });
    };
    Some(seat(doc, &ids, &s, &m, &offer))
}

/// Put the clip `id` on the nearest seat of its size without a clip whose
/// socket mouth is within reach of `at`; off any seat it was on before.
fn clip_on(doc: &mut Document, id: u64, size: &str, at: Point, reach: f32) -> Option<Snap> {
    for s in doc.seats.iter_mut().filter(|s| s.clip == Some(id)) {
        s.clip = None;
    }
    let p = [f64::from(at.x), f64::from(at.y)];
    let size = Kind::Taper(size.to_owned());
    let (i, f, m) = (doc.seats.iter().enumerate())
        .filter(|(_, s)| s.clip.is_none())
        .filter_map(|(i, s)| {
            let (a, b) = (port(doc, &s.a)?, port(doc, &s.b)?);
            let (f, m) = sides(&a, &b);
            (f.kind == size).then(|| (i, f.clone(), m.clone()))
        })
        .filter(|(_, f, _)| len(sub(f.point, p)) <= f64::from(reach).max(f.diameter()))
        .min_by(|a, b| len(sub(a.1.point, p)).total_cmp(&len(sub(b.1.point, p))))?;
    doc.seats.get_mut(i)?.clip = Some(id);
    clips(doc, &[]);
    // The clip joins its assembly's layer and place in the drawing order.
    let members = assembly(doc, f.at.piece);
    let layer = (doc.graphics.iter())
        .find(|g| g.id == f.at.piece)
        .map_or(-1, |g| g.layer);
    if let Some(g) = doc.graphics.iter_mut().find(|g| g.id == id) {
        g.layer = layer;
    }
    let end = (doc.graphics.iter())
        .rposition(|g| members.contains(&g.id) && g.id != id)
        .map_or(doc.graphics.len(), |i| i + 1);
    place(doc, &[id], end);
    Some(Snap {
        at: f.point,
        target: f,
        moving: m,
        offer: Offer::Clip,
        scaled: false,
        behind: None,
        added: None,
    })
}

/// The built-in clip a seat between the pieces `ids` lacks.
fn missing_clip(doc: &Document, ids: &[u64], s: &Seat) -> Option<Arc<Definition>> {
    if s.clip.is_some() || !ids.contains(&s.a.piece) || !ids.contains(&s.b.piece) {
        return None;
    }
    let (size, _) = clip_frame(doc, s)?;
    (builtin::catalogue().iter())
        .find(|d| d.clip.as_ref().is_some_and(|c| c.size == size))
        .cloned()
}

/// How many ground-glass seats between the pieces `ids` could take a clip.
pub fn unclipped(doc: &Document, ids: &[u64]) -> usize {
    (doc.seats.iter())
        .filter(|s| missing_clip(doc, ids, s).is_some())
        .count()
}

/// Put a clip of the right size on every ground-glass seat between the
/// pieces `ids` that has none; how many went on.
pub fn add_clips(doc: &mut Document, ids: &[u64]) -> usize {
    let wanted: Vec<(usize, Arc<Definition>, i32)> = (doc.seats.iter().enumerate())
        .filter_map(|(i, s)| {
            let layer = (doc.graphics.iter())
                .find(|g| g.id == s.a.piece)
                .map_or(-1, |g| g.layer);
            Some((i, missing_clip(doc, ids, s)?, layer))
        })
        .collect();
    for (i, d, layer) in &wanted {
        let id = doc.next_id();
        let mut g = placed::graphic(id, d.clone(), Point::default(), 1.);
        g.layer = *layer;
        doc.graphics.push(g);
        if let Some(s) = doc.seats.get_mut(*i) {
            s.clip = Some(id);
        }
    }
    clips(doc, &[]);
    wanted.len()
}

/// Joints the selected pieces could be connected to, nearest first: each a
/// free joint or hold of the selection and a free one elsewhere that fits
/// it, directly or through an adapter.
pub fn options(doc: &Document, ids: &[u64]) -> Vec<(Port, Port, Offer)> {
    let moving: BTreeSet<u64> = closure(doc, ids).into_iter().collect();
    let inner = free(doc, &moving, true);
    let stuck = fixed(doc, &moving);
    let outer: Vec<Port> = (free(doc, &moving, false).into_iter())
        .filter(|s| !stuck || !fixed(doc, &assembly(doc, s.at.piece)))
        .collect();
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
            if !scalable(doc, &moving, s.mm / m.mm) {
                continue;
            }
            out.push((gap(m, s), s.clone(), m.clone(), offer));
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
    let taken = |p: &Port| !p.shared() && doc.seats.iter().any(|x| x.holds(&p.at));
    if taken(&s) || taken(&m) {
        return None;
    }
    let pieces: BTreeSet<u64> = moving.iter().copied().collect();
    if !scalable(doc, &pieces, s.mm / m.mm) {
        return None;
    }
    let (moving, s, m) = ground_lock(doc, &moving, false, s, m)?;
    let offer = if mates(&s, &m) {
        Offer::Seat
    } else {
        Offer::Adapter(adapter(&s, &m)?.0)
    };
    Some(seat(doc, &moving, &s, &m, &offer))
}

/// Every moving piece stays within the scale limits when scaled by `k`.
fn scalable(doc: &Document, moving: &BTreeSet<u64>, k: f64) -> bool {
    let pt = f64::from(crate::style::DEFAULT.points_per_world());
    let range = f64::from(placed::MIN_PT_PER_MM) * 0.999..=f64::from(placed::MAX_PT_PER_MM) * 1.001;
    (doc.graphics.iter())
        .filter(|g| g.piece.is_some() && moving.contains(&g.id))
        .all(|g| range.contains(&(frame(g).scale_factor() * k * pt)))
}

/// What a drag that starts on `clicked` moves when the selection is `ids`:
/// the selection with every assembly in it, except that pieces held on a
/// grounded one, such as a clamp on a stand, slide along without it and
/// without what only it holds. Dragging the grounded piece itself moves
/// everything; other selected objects move as usual.
pub fn carried(doc: &Document, ids: &[u64], clicked: &[u64]) -> Vec<u64> {
    let all = closure(doc, ids);
    // What a clicked piece carries, and what its stand leaves behind.
    let (mut left, mut kept) = (BTreeSet::new(), BTreeSet::new());
    for &c in clicked.iter().filter(|c| all.contains(c)) {
        let members = assembly(doc, c);
        let ground: BTreeSet<u64> = (members.iter().copied())
            .filter(|id| grounded(doc, *id))
            .collect();
        if ground.is_empty() || ground.contains(&c) {
            kept.extend(members);
            continue;
        }
        let held = reach(doc, c, |s| {
            !ground.contains(&s.a.piece) && !ground.contains(&s.b.piece)
        });
        left.extend(members.difference(&held).copied());
        kept.extend(held);
    }
    (all.into_iter())
        .filter(|id| kept.contains(id) || !left.contains(id))
        .collect()
}

/// A drag of the pieces `ids` by `delta`, along the rails that hold them on
/// pieces that stay and stopped at the rails' ends; `None` when nothing that
/// stays holds them. Pieces held any other way do not move.
pub fn constrain(doc: &Document, ids: &[u64], delta: [f64; 2]) -> Option<[f64; 2]> {
    let mut axis: Option<[f64; 2]> = None;
    let (mut lo, mut hi) = (f64::NEG_INFINITY, f64::INFINITY);
    for s in (doc.seats.iter()).filter(|s| ids.contains(&s.a.piece) != ids.contains(&s.b.piece)) {
        let (Some(a), Some(b)) = (port(doc, &s.a), port(doc, &s.b)) else {
            return Some([0., 0.]);
        };
        let (rail, slider) = if ids.contains(&s.a.piece) {
            (b, a)
        } else {
            (a, b)
        };
        let v = *axis.get_or_insert(rail.dir);
        if !rail.shared() || (rail.dir[0] * v[1] - rail.dir[1] * v[0]).abs() > 1e-6 {
            return Some([0., 0.]);
        }
        // How far along `v` the slider may go before it leaves either end.
        let way = dot(rail.dir, v);
        let t = dot(sub(slider.centre(), rail.point), rail.dir);
        let (x, y) = (-t * way, (rail.length - t) * way);
        lo = lo.max(x.min(y));
        hi = hi.min(x.max(y));
    }
    let v = axis?;
    Some(mul(v, dot(delta, v).clamp(lo, hi.max(lo))))
}

/// Release the seat at joint `j` and pull the side without the anchor out,
/// along a joint's axis or away from a hold, far enough to leave a gap
/// outside snap reach.
pub fn detach(doc: &mut Document, j: &JointRef) -> bool {
    let Some(seat) = doc.seats.iter().find(|s| s.holds(j)).cloned() else {
        return false;
    };
    release(doc, |s| *s == seat);
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
    // Pieces still joined through another seat stay where they are.
    let apart = !assembly(doc, stay.piece).contains(&go.piece);
    if apart && let (Some(s), Some(g)) = (port(doc, stay), port(doc, go)) {
        let (f, _) = sides(&s, &g);
        // A joint length and a diameter, or two diameters off a hold: a
        // clear gap, and out of snap reach.
        let away = match &f.kind {
            Kind::Taper(size) => taper(size).map_or(10., |t| t.l + t.d),
            Kind::Plug(_, size) => 2. * size,
            Kind::Hold(_, [lo, hi]) => lo + hi,
        } * s.mm;
        // Off a hold, towards the side the pulled piece is on.
        let out = if s.hold().is_some() {
            let n = perp(s.dir);
            let middle =
                (doc.graphics.iter())
                    .find(|x| x.id == g.at.piece)
                    .map_or(g.centre(), |x| {
                        let m = placed::middle(x);
                        [f64::from(m.x), f64::from(m.y)]
                    });
            if dot(sub(middle, s.centre()), n) < 0. {
                mul(n, -1.)
            } else {
                n
            }
        } else {
            normalize(s.dir)
        };
        let shift = mul(out, away);
        let pulled: Vec<u64> = assembly(doc, g.at.piece).into_iter().collect();
        doc.translate(&pulled, shift[0] as f32, shift[1] as f32);
    }
    mark(doc);
    true
}

/// A seated joint's partner; the first of a rail's.
pub fn partner(doc: &Document, j: &JointRef) -> Option<JointRef> {
    partners(doc, j).into_iter().next()
}

/// Every joint or hold seated on `j`.
pub fn partners(doc: &Document, j: &JointRef) -> Vec<JointRef> {
    (doc.seats.iter())
        .filter_map(|s| {
            if s.a == *j {
                Some(s.b.clone())
            } else if s.b == *j {
                Some(s.a.clone())
            } else {
                None
            }
        })
        .collect()
}

/// The clip on the seat at joint `j`.
pub fn clip_at(doc: &Document, j: &JointRef) -> Option<u64> {
    doc.seats.iter().find(|s| s.holds(j))?.clip
}

/// Drop the seats of deleted pieces with their clips, and deleted clips.
pub(crate) fn prune(doc: &mut Document) {
    let ids: BTreeSet<u64> = doc.graphics.iter().map(|g| g.id).collect();
    for s in &mut doc.seats {
        if s.clip.is_some_and(|c| !ids.contains(&c)) {
            s.clip = None;
        }
    }
    release(doc, |s| {
        !ids.contains(&s.a.piece) || !ids.contains(&s.b.piece)
    });
}
