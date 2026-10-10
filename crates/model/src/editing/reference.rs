//! Exact drawing orientation and single-bond stretching. These operations change
//! coordinates, never bond identity, endpoint order, projection depth or chemistry.
use crate::{
    document::{Document, Point},
    graphics::PathCommand,
};
use std::collections::{HashMap, HashSet};

/// A retained, ordered bond or a straight command in an editable graphic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Edge {
    Bond(u64, u64),
    Graphic(u64, usize),
}

impl Edge {
    pub fn endpoints(self, doc: &Document) -> Result<(Point, Point), String> {
        let pair = match self {
            Self::Bond(a, b) => {
                if !doc.bonds.iter().any(|bond| bond.a == a && bond.b == b) {
                    return Err("The reference bond is no longer available".into());
                }
                doc.atom(a)
                    .zip(doc.atom(b))
                    .map(|(a, b)| (a.position, b.position))
            }
            Self::Graphic(id, index) => doc
                .graphics
                .iter()
                .find(|g| g.id == id)
                .and_then(|g| {
                    straight_edges(&g.commands())
                        .into_iter()
                        .find(|(i, _, _)| *i == index)
                })
                .map(|(_, a, b)| (a, b)),
        }
        .ok_or("Choose an existing reference bond or straight edge")?;
        if !finite(pair.0) || !finite(pair.1) || pair.0.distance(pair.1) < 0.001 {
            return Err("The reference edge must have a positive length".into());
        }
        Ok(pair)
    }
}

fn finite(p: Point) -> bool {
    p.x.is_finite() && p.y.is_finite()
}

fn straight_edges(commands: &[PathCommand]) -> Vec<(usize, Point, Point)> {
    let mut anchor = None;
    let mut start = None;
    let mut out = Vec::new();
    for (index, command) in commands.iter().enumerate() {
        match command {
            PathCommand::Move(p) => {
                anchor = Some(*p);
                start = Some(*p);
            }
            PathCommand::Line(p) => {
                if let Some(a) = anchor {
                    out.push((index, a, *p));
                }
                anchor = Some(*p);
            }
            PathCommand::Cubic(_, _, p) => anchor = Some(*p),
            PathCommand::Close => {
                if let Some((a, b)) = anchor.zip(start) {
                    out.push((index, a, b));
                }
                anchor = start;
            }
        }
    }
    out
}

/// References available in this explicit selection, in document order.
pub fn edges(doc: &Document, selected: &[u64]) -> Vec<Edge> {
    let ids: HashSet<_> = doc
        .expand_abbreviation_selection(selected)
        .into_iter()
        .collect();
    let mut out: Vec<_> = doc
        .bonds
        .iter()
        .filter(|b| ids.contains(&b.a) && ids.contains(&b.b) && doc.bond_visible(b.a, b.b))
        .map(|b| Edge::Bond(b.a, b.b))
        .collect();
    for graphic in doc.graphics.iter().filter(|g| ids.contains(&g.id)) {
        out.extend(
            straight_edges(&graphic.commands())
                .into_iter()
                .filter(|(_, a, b)| a.distance(*b) >= 0.001)
                .map(|(i, _, _)| Edge::Graphic(graphic.id, i)),
        );
    }
    out
}

/// Expand connected molecules when requested. An explicitly partial selection is
/// rejected rather than silently stretching exterior bonds or clearing stereo.
pub fn scope(doc: &Document, selected: &[u64], connected: bool) -> Result<Vec<u64>, String> {
    let all: HashSet<_> = doc.object_ids().collect();
    if selected.is_empty() || selected.iter().any(|id| !all.contains(id)) {
        return Err("Select the objects to rotate".into());
    }
    let mut ids: HashSet<_> = doc
        .expand_abbreviation_selection(selected)
        .into_iter()
        .collect();
    if connected {
        loop {
            let before = ids.len();
            let selection: Vec<_> = ids.iter().copied().collect();
            ids.extend(doc.expand_integral_groups(&selection));
            ids.extend(crate::attachments::selection(doc, &selection));
            for atom in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
                if ids.contains(&atom.id) || atom.centroid.iter().any(|id| ids.contains(id)) {
                    ids.extend(atom.centroid.iter().copied());
                    ids.insert(atom.id);
                }
            }
            for b in &doc.bonds {
                if ids.contains(&b.a) || ids.contains(&b.b) {
                    ids.extend([b.a, b.b]);
                }
            }
            if ids.len() == before {
                break;
            }
        }
    } else {
        for atom in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
            let any = atom.centroid.iter().any(|id| ids.contains(id));
            let all = atom.centroid.iter().all(|id| ids.contains(id));
            if any && !all || ids.contains(&atom.id) && !all {
                return Err(
                    "Select the complete fragment and its derived attachment targets".into(),
                );
            }
            if all {
                ids.insert(atom.id);
            }
        }
    }
    if doc
        .bonds
        .iter()
        .any(|b| ids.contains(&b.a) != ids.contains(&b.b))
    {
        return Err("Select the complete connected fragment to keep its exterior bonds and stereochemistry intact".into());
    }
    Ok(doc
        .all_ids()
        .into_iter()
        .filter(|id| ids.contains(id))
        .collect())
}

/// Clockwise drawing angle. Axis alignment uses the nearest equivalent modulo
/// 180°, directed alignment modulo 360°; neither uses the 15° drawing grid.
pub fn alignment_degrees(
    doc: &Document,
    edge: Edge,
    target: f64,
    directed: bool,
) -> Result<f32, String> {
    if !target.is_finite() {
        return Err("Enter a finite target angle".into());
    }
    let (a, b) = edge.endpoints(doc)?;
    let angle = (f64::from(b.y) - f64::from(a.y))
        .atan2(f64::from(b.x) - f64::from(a.x))
        .to_degrees();
    let period = if directed { 360. } else { 180. };
    let target = target.rem_euclid(period);
    let delta = (target - angle + period / 2.).rem_euclid(period) - period / 2.;
    // Repeating an alignment already reached to f32 coordinate precision is a no-op.
    let length = f64::from(a.distance(b));
    let magnitude = [a.x, a.y, b.x, b.y]
        .into_iter()
        .map(|v| f64::from(v.abs()))
        .fold(length, f64::max);
    let slack = (4. * f64::from(f32::EPSILON) * magnitude / length).to_degrees();
    Ok(if delta.abs() < slack {
        0.
    } else {
        delta as f32
    })
}

/// Prepare an atomic rotation/copy. A shared pivot is a fixed world point, even
/// when copying changes which fragment is selected.
pub fn rotate(
    doc: &Document,
    ids: &[u64],
    pivot: Point,
    degrees: f32,
    duplicate: bool,
) -> Result<(Document, Vec<u64>), String> {
    if !finite(pivot) || !degrees.is_finite() {
        return Err("Enter a finite pivot and angle".into());
    }
    let ids = scope(doc, ids, false)?;
    let mut candidate = doc.clone();
    let ids = if duplicate {
        let part = super::selection(doc, &ids);
        let copied = super::append(&mut candidate, &part, Point::default());
        if copied.is_empty() {
            return Err("The selected fragment could not be copied".into());
        }
        copied
    } else {
        ids
    };
    let degrees = degrees % 360.;
    if degrees != 0. {
        // Keep world-coordinate subtraction and trig in f64. Round only the
        // stored result, especially for a small fragment far from the origin.
        let (s, c) = f64::from(degrees).to_radians().sin_cos();
        let vector = |p: Point| {
            let (x, y) = (f64::from(p.x), f64::from(p.y));
            Point::new((x * c - y * s) as f32, (x * s + y * c) as f32)
        };
        super::map_positions(
            &mut candidate,
            &ids,
            |p| {
                let (x, y) = (
                    f64::from(p.x) - f64::from(pivot.x),
                    f64::from(p.y) - f64::from(pivot.y),
                );
                Point::new(
                    (f64::from(pivot.x) + x * c - y * s) as f32,
                    (f64::from(pivot.y) + x * s + y * c) as f32,
                )
            },
            vector,
        );
        crate::projection::sync_centroids(&mut candidate);
    }
    candidate.validate()?;
    Ok((candidate, ids))
}

/// A movable component on one side of a bridge bond. Removing a ring edge does
/// not split the graph, so it cannot change only that edge's length rigidly.
#[derive(Debug, Clone)]
pub struct Stretch {
    pub fixed: u64,
    pub moving: u64,
    pub ids: Vec<u64>,
    pub length: f32,
    pub minimum: f32,
    axis: (f64, f64),
}

impl Stretch {
    pub fn new(doc: &Document, fixed: u64, moving: u64) -> Result<Self, String> {
        let bond = doc
            .bonds
            .iter()
            .find(|b| (b.a == fixed && b.b == moving) || (b.a == moving && b.b == fixed))
            .ok_or("Choose an existing bond to stretch")?;
        if bond.order == 0
            || [fixed, moving].iter().any(|id| {
                doc.atom(*id)
                    .is_none_or(|a| !a.centroid.is_empty() || a.attachment.is_some())
            })
        {
            return Err("Stretch an ordinary bond between two atom sites".into());
        }
        let (a, b) = doc
            .atom(fixed)
            .zip(doc.atom(moving))
            .ok_or("The bond endpoints are unavailable")?;
        let x = f64::from(b.position.x) - f64::from(a.position.x);
        let y = f64::from(b.position.y) - f64::from(a.position.y);
        let length = x.hypot(y);
        if !length.is_finite() || length < 0.001 {
            return Err("The bond must have a positive length".into());
        }
        let mut adjacent: HashMap<u64, Vec<u64>> = HashMap::new();
        for b in &doc.bonds {
            if b.order == 0 || (b.a == fixed && b.b == moving) || (b.a == moving && b.b == fixed) {
                continue;
            }
            adjacent.entry(b.a).or_default().push(b.b);
            adjacent.entry(b.b).or_default().push(b.a);
        }
        let mut ids = HashSet::new();
        let mut pending = vec![moving];
        while let Some(id) = pending.pop() {
            if !ids.insert(id) {
                continue;
            }
            pending.extend(adjacent.get(&id).into_iter().flatten().copied());
        }
        if ids.contains(&fixed) {
            return Err(
                "A ring bond cannot change only its length while the rest of the ring stays rigid"
                    .into(),
            );
        }
        let selected: Vec<_> = ids.iter().copied().collect();
        ids.extend(doc.expand_abbreviation_selection(&selected));
        ids.extend(crate::attachments::selection(doc, &selected));
        for atom in doc.atoms.iter().filter(|a| !a.centroid.is_empty()) {
            let any = atom.centroid.iter().any(|id| ids.contains(id));
            let all = atom.centroid.iter().all(|id| ids.contains(id));
            if any && !all || ids.contains(&atom.id) && !all {
                return Err("This branch shares a derived attachment with fixed atoms".into());
            }
            if all {
                ids.insert(atom.id);
            }
        }
        if ids.contains(&fixed)
            || doc.bonds.iter().any(|b| {
                let boundary = ids.contains(&b.a) != ids.contains(&b.b);
                boundary && !((b.a == fixed && b.b == moving) || (b.a == moving && b.b == fixed))
            })
        {
            return Err("This fragment has more than one fixed attachment".into());
        }
        Ok(Self {
            fixed,
            moving,
            ids: doc
                .all_ids()
                .into_iter()
                .filter(|id| ids.contains(id))
                .collect(),
            length: length as f32,
            minimum: doc.drawing_style.world(0.1),
            axis: (x / length, y / length),
        })
    }
    /// Project a drag displacement onto the original (possibly non-grid) axis.
    pub fn dragged_length(&self, delta: Point) -> f32 {
        let change = f64::from(delta.x) * self.axis.0 + f64::from(delta.y) * self.axis.1;
        if !change.is_finite() {
            return self.length;
        }
        (f64::from(self.length) + change).max(f64::from(self.minimum)) as f32
    }
    pub fn delta(&self, length: f32) -> Result<Point, String> {
        if !length.is_finite() || length < self.minimum || length > 1_000_000. {
            return Err("Enter a positive bond length of at least 0.1 pt".into());
        }
        let difference = f64::from(length) - f64::from(self.length);
        Ok(Point::new(
            (difference * self.axis.0) as f32,
            (difference * self.axis.1) as f32,
        ))
    }
    pub fn apply(&self, doc: &Document, length: f32) -> Result<Document, String> {
        // Resolve fresh IDs and topology at commit, never apply a stale branch.
        let current = Self::new(doc, self.fixed, self.moving)?;
        let delta = current.delta(length)?;
        let mut candidate = doc.clone();
        candidate.translate(&current.ids, delta.x, delta.y);
        let (a, b) = candidate
            .atom(current.fixed)
            .zip(candidate.atom(current.moving))
            .ok_or("The bond endpoints are unavailable")?;
        let x = f64::from(b.position.x) - f64::from(a.position.x);
        let y = f64::from(b.position.y) - f64::from(a.position.y);
        if x * current.axis.0 + y * current.axis.1 <= 0.
            || x.hypot(y) < f64::from(current.minimum) * 0.5
        {
            return Err(
                "This length is too small for these drawing coordinates; choose a larger length"
                    .into(),
            );
        }
        candidate.validate()?;
        Ok(candidate)
    }
}

#[cfg(test)]
mod tests;
