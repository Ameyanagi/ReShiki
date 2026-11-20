//! Hoses. A hose is a tube piece drawn along a centre line stored on each
//! placed copy (see [`placed::centre_line`]). Its ends seat on hose barbs:
//! an end pushed onto a barb leaves it along the barb, and the hose bends
//! smoothly through its middle to the other end. Dragging an end or the
//! middle reshapes the hose; nothing else moves.
use super::*;
use crate::equipment::format::Xy;

/// One end of a hose, or its middle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grip {
    End(usize),
    Middle,
}

/// A click places a hose this long, in millimetres: out along the barb, then down.
const CLICK_OUT_MM: f64 = 30.;
const CLICK_DOWN_MM: f64 = 50.;

fn tube(g: &Graphic) -> Option<(&Definition, Vec<Xy>)> {
    let piece = g.piece.as_ref()?;
    let d = piece.linked.as_ref()?;
    Some((d, placed::centre_line(d, piece)?))
}

/// The point halfway along a centre line's chain of Béziers.
fn middle(line: &[Xy]) -> Xy {
    let n = line.len().saturating_sub(1) / 3;
    let rest = line.get(3 * (n / 2)..).unwrap_or_default();
    if n % 2 == 1 {
        placed::bezier(rest, 0.5)
    } else {
        rest.first().copied().unwrap_or_default()
    }
}

/// A hose's two ends and its middle, in world units; none for any other graphic.
pub fn grips(g: &Graphic) -> Vec<(Grip, [f64; 2])> {
    let Some((_, line)) = tube(g) else {
        return Vec::new();
    };
    let t = frame(g);
    let ends = [0, 1].map(|k| t.apply(placed::end(&line, k).0));
    vec![
        (Grip::End(0), ends[0]),
        (Grip::Middle, t.apply(middle(&line))),
        (Grip::End(1), ends[1]),
    ]
}

/// A unit vector along `v`, or `fallback` when `v` is too short to have one.
fn unit(v: Xy, fallback: Xy) -> Xy {
    if len(v) > 1e-9 {
        normalize(v)
    } else {
        fallback
    }
}

/// How far a hose runs straight on from each end, in millimetres, before it
/// bends: room to turn when a barb faces away from the other end.
const LEAD_MM: f64 = 20.;
/// How far a new hose sags below the straight line between its leads, as a
/// share of that line's length.
const SAG: f64 = 0.15;

/// One end of a hose as [`shape`] lays it: where it is, the unit vector
/// into the hose, and how far it runs straight on (world units).
type End = (Xy, Xy, f64);

/// The three Béziers from end `e` towards `m`, `m` excluded: straight on
/// for the lead, then a turn of radius `r`. A lead that faces away from
/// `m` turns a quarter circle towards it first, so a hose doubling back
/// to a barb loops instead of pinching; otherwise it runs straight on.
fn leg((p, out, lead): End, m: Xy, r: f64) -> (Vec<Xy>, Xy) {
    let l = add(p, mul(out, lead));
    let side = perp(out);
    let n = if dot(side, sub(m, l)) < 0. {
        mul(side, -1.)
    } else {
        side
    };
    let (t, dir) = if dot(out, sub(m, l)) < 0. {
        (add(l, add(mul(out, r), mul(n, r))), n)
    } else {
        (add(l, mul(out, r / 2.)), out)
    };
    // A quarter circle's control points sit 0.552 r along its tangents.
    let k = if dir == out { r / 6. } else { 0.552 * r };
    let d = len(sub(m, t)).max(1.);
    let points = vec![
        p,
        add(p, mul(out, lead / 3.)),
        add(p, mul(out, lead * 2. / 3.)),
        l,
        add(l, mul(out, k)),
        sub(t, mul(dir, k)),
        t,
        add(t, mul(dir, d / 3.)),
    ];
    (points, t)
}

/// A smooth line from end `a` through `m` to end `b` as six Béziers: each
/// end's [`leg`], meeting at `m`, the chain's middle point.
fn shape(a: End, m: Xy, b: End, r: f64) -> Vec<Xy> {
    let (mut line, ta) = leg(a, m, r);
    let (mut back, tb) = leg(b, m, r);
    let along = unit(sub(tb, ta), perp(a.1));
    let tangent = unit(add(unit(sub(m, ta), along), unit(sub(tb, m), along)), along);
    let (da, db) = (len(sub(m, ta)).max(1.), len(sub(tb, m)).max(1.));
    line.extend([
        sub(m, mul(tangent, da / 3.)),
        m,
        add(m, mul(tangent, db / 3.)),
    ]);
    back.reverse();
    line.extend(back);
    line
}

/// The free hose barb nearest `p` within `reach` that end `k` of hose `id`
/// can be pushed onto: at the hose's scale unless `any_scale` (a new hose
/// takes its first barb's), and never joining two stands' assemblies.
fn barb(
    doc: &Document,
    id: u64,
    k: usize,
    p: [f64; 2],
    reach: f64,
    any_scale: bool,
) -> Option<Port> {
    let g = doc.graphics.iter().find(|g| g.id == id)?;
    let joint = g
        .piece
        .as_ref()?
        .linked
        .as_ref()?
        .tube
        .as_ref()?
        .ends
        .get(k)?;
    let end = ports(g).into_iter().find(|q| q.at.joint == *joint)?;
    let others = BTreeSet::from([id]);
    let mine = assembly(doc, id);
    let apart = |b: &Port| {
        !mine.contains(&b.at.piece) && fixed(doc, &mine) && fixed(doc, &assembly(doc, b.at.piece))
    };
    free(doc, &others, false)
        .into_iter()
        .filter(|b| mates(&end, b) && (any_scale || (b.mm / end.mm - 1.).abs() <= 1e-3))
        .filter(|b| !apart(b))
        .map(|b| (len(sub(b.point, p)), b))
        .filter(|(d, b)| *d <= reach.max(b.diameter()))
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, b)| b)
}

/// Store a hose's centre line from world points, ends first and last.
fn store(doc: &mut Document, id: u64, world: &[Xy]) {
    let Some(g) = doc.graphics.iter_mut().find(|g| g.id == id) else {
        return;
    };
    let Some(inverse) = frame(g).inverse() else {
        return;
    };
    if let Some(piece) = g.piece.as_mut() {
        piece.centre_line = world.iter().map(|p| inverse.apply(*p)).collect();
    }
}

/// Seat end `k` of hose `id` on `barb`: the hose and what it already holds
/// join the barb's assembly, on its layer and after it in the drawing order.
fn push_on(doc: &mut Document, id: u64, k: usize, barb: &Port) {
    let joint = (doc.graphics.iter()).find(|g| g.id == id).and_then(|g| {
        g.piece
            .as_ref()?
            .linked
            .as_ref()?
            .tube
            .as_ref()?
            .ends
            .get(k)
            .cloned()
    });
    let Some(joint) = joint else {
        return;
    };
    let target = assembly(doc, barb.at.piece);
    let joining: Vec<u64> = (doc.graphics.iter())
        .filter(|g| !target.contains(&g.id) && assembly(doc, id).contains(&g.id))
        .map(|g| g.id)
        .collect();
    doc.seats
        .push(Seat::new(barb.at.clone(), JointRef { piece: id, joint }));
    let layer = (doc.graphics.iter())
        .find(|g| g.id == barb.at.piece)
        .map(|g| g.layer);
    for g in doc.graphics.iter_mut().filter(|g| joining.contains(&g.id)) {
        g.layer = layer.unwrap_or(g.layer);
    }
    let end = (doc.graphics.iter())
        .rposition(|g| target.contains(&g.id))
        .map_or(doc.graphics.len(), |i| i + 1);
    place(doc, &joining, end);
    mark(doc);
}

/// Where each end of hose `id` goes and the way it leaves into the hose:
/// along the barb it is on, else towards `m`.
fn reshape(doc: &mut Document, id: u64, ends: [(Xy, Option<Port>); 2], m: Xy) {
    let Some((mm, width)) = (doc.graphics.iter()).find(|g| g.id == id).and_then(|g| {
        let tube = g.piece.as_ref()?.linked.as_ref()?.tube.as_ref()?;
        Some((frame(g).scale_factor(), tube.width_mm))
    }) else {
        return;
    };
    let lead = LEAD_MM * mm;
    // On a barb the hose runs straight on along it; a free end aims at the
    // middle, with a lead short enough to stay short of it.
    let end = |(p, barb): &(Xy, Option<Port>)| match barb {
        Some(b) => (*p, b.dir, lead),
        None => (
            *p,
            unit(sub(m, *p), [1., 0.]),
            lead.min(len(sub(m, *p)) / 3.).max(1e-3),
        ),
    };
    let [a, b] = &ends;
    // Turns are a width and a half across the centre line: open, not kinked.
    let line = shape(end(a), m, end(b), 1.5 * width * mm);
    store(doc, id, &line);
}

/// Place a new hose from `from` to `to` (world units). An end within
/// `reach` of a free hose barb is pushed onto it. A click (`from` = `to`)
/// on a barb hangs a hose from it; elsewhere it lays one to the right.
/// Returns the hose's ID and the barbs it was pushed onto.
pub fn place_hose(
    doc: &mut Document,
    definition: Arc<Definition>,
    from: [f64; 2],
    to: [f64; 2],
    reach: f64,
) -> (u64, Vec<Port>) {
    let id = doc.next_id();
    let scale = placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM);
    let mut g = placed::graphic(
        id,
        definition,
        Point::new(from[0] as f32, from[1] as f32),
        scale,
    );
    g.origin = Point::new(from[0] as f32, from[1] as f32);
    doc.graphics.push(g);
    let first = barb(doc, id, 0, from, reach, true);
    // The hose takes the scale of the barb it starts on.
    if let (Some(b), Some(g)) = (&first, doc.graphics.iter_mut().find(|g| g.id == id)) {
        let k = (b.mm / f64::from(scale)) as f32;
        g.axis_x = Point::new(g.axis_x.x * k, g.axis_x.y * k);
        g.axis_y = Point::new(g.axis_y.x * k, g.axis_y.y * k);
    }
    let mm = first.as_ref().map_or(f64::from(scale), |b| b.mm);
    let a = first.as_ref().map_or(from, |b| b.point);
    let clicked = len(sub(to, from)) < 1e-9;
    let b = if clicked {
        let out = first.as_ref().map_or([1., 0.], |b| b.dir);
        let down = if first.is_some() { CLICK_DOWN_MM } else { 0. };
        add(a, add(mul(out, CLICK_OUT_MM * mm), [0., down * mm]))
    } else {
        to
    };
    // Seat the first end so the second cannot take the same barb.
    if let Some(barb) = &first {
        push_on(doc, id, 0, barb);
    }
    let second = (!clicked)
        .then(|| barb(doc, id, 1, b, reach, false))
        .flatten();
    let b = second.as_ref().map_or(b, |s| s.point);
    // The middle sags below the leads' midpoint, as a hose hangs.
    let lead = |p: Xy, barb: &Option<Port>| {
        barb.as_ref()
            .map_or(p, |b| add(p, mul(b.dir, LEAD_MM * mm)))
    };
    let (la, lb) = (lead(a, &first), lead(b, &second));
    let m = add(mul(add(la, lb), 0.5), [0., SAG * len(sub(lb, la))]);
    reshape(doc, id, [(a, first.clone()), (b, second.clone())], m);
    if let Some(barb) = &second {
        push_on(doc, id, 1, barb);
    }
    (id, first.into_iter().chain(second).collect())
}

/// Drag `grip` of hose `id` to `to` (world units). An end lets go of its
/// barb and is pushed onto a free one within `reach`; the middle bends the
/// hose through `to`. The other end stays where it is. Returns the barb
/// the dragged end is now on.
pub fn drag(doc: &mut Document, id: u64, grip: Grip, to: [f64; 2], reach: f64) -> Option<Port> {
    let g = doc.graphics.iter().find(|g| g.id == id)?;
    let (d, line) = tube(g)?;
    let refs = d
        .tube
        .as_ref()?
        .ends
        .clone()
        .map(|joint| JointRef { piece: id, joint });
    let t = frame(g);
    let m = t.apply(middle(&line));
    // Each end where it is, with the barb it is on.
    let [ra, rb] = &refs;
    let mut now = [(0, ra), (1, rb)].map(|(k, j)| {
        let p = t.apply(placed::end(&line, k).0);
        (p, partner(doc, j).and_then(|b| port(doc, &b)))
    });
    match grip {
        Grip::Middle => {
            reshape(doc, id, now, to);
            None
        }
        Grip::End(k) => {
            let j = refs.get(k)?.clone();
            release(doc, |s| s.holds(&j));
            let barb = barb(doc, id, k, to, reach, false);
            let p = barb.as_ref().map_or(to, |b| b.point);
            let slot = now.get_mut(k)?;
            // The middle follows the end halfway, so the hose keeps its sag.
            let m = add(m, mul(sub(p, slot.0), 0.5));
            *slot = (p, barb.clone());
            reshape(doc, id, now, m);
            if let Some(barb) = &barb {
                push_on(doc, id, k, barb);
            }
            barb
        }
    }
}
