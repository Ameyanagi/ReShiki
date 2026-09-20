//! Molecular ring drawings. Chair presets are 2D depictions, not 3D conformers
//! or stereochemical assignments. Every edge uses the chosen bond length.
use crate::{
    document::{Document, Point},
    editing, templates,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Preset {
    #[default]
    Regular,
    ChairUp,
    ChairDown,
    Cyclopentadiene,
}
impl Preset {
    pub const ALL: &'static [Self] = &[
        Self::Regular,
        Self::ChairUp,
        Self::ChairDown,
        Self::Cyclopentadiene,
    ];
    pub fn document(self, length: f32, alternate: bool) -> Document {
        let mut doc = Document::default();
        let points = match self {
            Self::ChairUp | Self::ChairDown => {
                // Three unit vectors and their opposites close a centrosymmetric
                // chair. Its two shallow edges form the familiar inward corners.
                let mut points = vec![Point::default()];
                let mut current = Point::default();
                for angle in [60_f32, 195., 165., 240., 15.] {
                    let angle = angle.to_radians();
                    current = current.offset(length * angle.cos(), length * angle.sin());
                    points.push(current);
                }
                if self == Self::ChairDown {
                    for p in &mut points {
                        p.y = -p.y;
                    }
                }
                let center = Point::new(
                    points.iter().map(|p| p.x).sum::<f32>() / 6.,
                    points.iter().map(|p| p.y).sum::<f32>() / 6.,
                );
                points
                    .into_iter()
                    .map(|p| p.offset(-center.x, -center.y))
                    .collect::<Vec<_>>()
            }
            _ => {
                let n = if self == Self::Cyclopentadiene { 5 } else { 6 };
                let radius = length / (2. * (std::f32::consts::PI / n as f32).sin());
                (0..n)
                    .map(|i| {
                        let angle = -std::f32::consts::FRAC_PI_2
                            + i as f32 * std::f32::consts::TAU / n as f32;
                        Point::new(radius * angle.cos(), radius * angle.sin())
                    })
                    .collect()
            }
        };
        let ids: Vec<_> = points.into_iter().map(|p| doc.add_atom("C", p)).collect();
        for (i, (&a, &b)) in ids
            .iter()
            .zip(ids.iter().cycle().skip(1))
            .take(ids.len())
            .enumerate()
        {
            let phase = if alternate { 1 } else { 0 };
            let order = if self == Self::Cyclopentadiene && [phase, phase + 2].contains(&i) {
                2
            } else {
                1
            };
            doc.add_bond(a, b, order, "plain");
        }
        doc
    }
}
impl std::fmt::Display for Preset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Regular => "Regular ring",
            Self::ChairUp => "Chair A",
            Self::ChairDown => "Chair B",
            Self::Cyclopentadiene => "Cyclopentadiene",
        })
    }
}

#[derive(Clone, Copy)]
pub struct Drawing {
    pub preset: Preset,
    pub length: f32,
    pub alternate: bool,
    pub connect: bool,
}
impl Drawing {
    pub fn place(
        self,
        doc: &Document,
        point: Point,
        direction: Option<Point>,
        radius: f32,
    ) -> Result<(Document, Vec<u64>), &'static str> {
        if !self.length.is_finite() || self.length <= 0. {
            return Err("Ring bond length must be positive and finite.");
        }
        if doc.validate().is_err() {
            return Err("The drawing is invalid.");
        }
        let part = self.preset.document(self.length, self.alternate);
        if !self.connect {
            return templates::place(doc, &part, point, direction, radius);
        }
        let Some(target_id) = doc.nearest(point, radius) else {
            if editing::nearest_bond(doc, point, radius).is_some() {
                return Err("Start on an atom to connect a ring with a new bond.");
            }
            return templates::place(doc, &part, point, direction, radius);
        };
        let target = doc
            .atom(target_id)
            .ok_or("The attachment atom is no longer available")?;
        if target.stereo.is_some()
            || target.map_num != 0
            || target.explicit_h > 0
            || target.no_implicit
            || target.radical_electrons > 0
            || !target.marks.is_empty()
            || templates::valence(doc, target_id) + 2 > templates::capacity(target)
        {
            return Err("This atom cannot accept a new ring bond without changing its chemistry.");
        }
        let neighbors: Vec<_> = doc
            .bonds
            .iter()
            .filter_map(|b| {
                if b.a == target_id {
                    doc.atom(b.b)
                } else if b.b == target_id {
                    doc.atom(b.a)
                } else {
                    None
                }
            })
            .map(|a| a.position)
            .collect();
        let angle = direction
            .filter(|p| p.distance(target.position) > radius)
            .map(|p| (p.y - target.position.y).atan2(p.x - target.position.x))
            .unwrap_or_else(|| editing::open_angle(target.position, &neighbors));
        let length = if neighbors.is_empty() {
            self.length
        } else {
            neighbors
                .iter()
                .map(|p| p.distance(target.position))
                .sum::<f32>()
                / neighbors.len() as f32
        };
        let source = part
            .atoms
            .iter()
            .filter(|a| templates::valence(&part, a.id) + 2 <= templates::capacity(a))
            .min_by(|a, b| a.position.x.total_cmp(&b.position.x))
            .ok_or("This ring has no available connection atom.")?;
        let source_id = source.id;
        let origin = source.position;
        let inward = (-origin.y).atan2(-origin.x);
        let mut positioned = part.clone();
        let all = positioned.all_ids();
        editing::transform_about(
            &mut positioned,
            &all,
            origin,
            length / self.length,
            (angle - inward).to_degrees(),
        );
        let dest = target
            .position
            .offset(length * angle.cos(), length * angle.sin());
        let mut result = doc.clone();
        let ids = editing::append(
            &mut result,
            &positioned,
            Point::new(dest.x - origin.x, dest.y - origin.y),
        );
        let source_index = part
            .all_ids()
            .iter()
            .position(|id| *id == source_id)
            .ok_or("The source ring atom is unavailable")?;
        let new_id = ids
            .get(source_index)
            .copied()
            .ok_or("The ring could not be inserted")?;
        result.add_bond(target_id, new_id, 1, "plain");
        result.reconcile_molecule_groups();
        let mut selected = ids;
        selected.push(target_id);
        Ok((result, selected))
    }
}
