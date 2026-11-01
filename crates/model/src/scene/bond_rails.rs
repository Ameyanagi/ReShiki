//! Screen-space offsets and per-end setbacks shared by every rail consumer.
use super::{FaceIndex, automatic_double_side, double_position};
use crate::{
    bonds::DoublePosition,
    document::{Bond, Point},
};

#[derive(Clone, Copy)]
pub(crate) struct BondRail {
    pub offset: (f32, f32),
    pub trim: (f32, f32),
}

impl BondRail {
    pub(crate) fn points(&self, a: Point, b: Point) -> (Point, Point) {
        let length = a.distance(b).max(0.001);
        let (ux, uy) = ((b.x - a.x) / length, (b.y - a.y) / length);
        (
            a.offset(
                -uy * self.offset.0 + ux * self.trim.0,
                ux * self.offset.0 + uy * self.trim.0,
            ),
            b.offset(
                -uy * self.offset.1 - ux * self.trim.1,
                ux * self.offset.1 - uy * self.trim.1,
            ),
        )
    }
}

pub(crate) fn bond_rails(faces: &FaceIndex<'_>, bond: &Bond) -> Vec<BondRail> {
    let style = &faces.doc().drawing_style;
    let spacing = style.bond_length_world * style.bond_spacing_ratio;
    let rail = |offset| BondRail {
        offset: (offset, offset),
        trim: (0., 0.),
    };
    match bond.order {
        2 if bond.display == "wavy" => [-spacing / 2., spacing / 2.]
            .map(|offset| BondRail {
                offset: (offset, -offset),
                trim: (0., 0.),
            })
            .to_vec(),
        2 | 7 => match double_position(faces, bond) {
            position @ (DoublePosition::Left | DoublePosition::Right) => {
                let side = if position == DoublePosition::Left {
                    -1.
                } else {
                    1.
                };
                let mut inner = rail(spacing * side);
                let face = (bond.double_position == DoublePosition::Auto)
                    .then(|| faces.face_atoms(bond))
                    .flatten();
                if let Some((a, b)) = faces.atom(bond.a).zip(faces.atom(bond.b)) {
                    let length = a.position.distance(b.position).max(0.001);
                    let (ux, uy) = (
                        (b.position.x - a.position.x) / length,
                        (b.position.y - a.position.y) / length,
                    );
                    let trim = |id, other, origin: Point, direction: f32| {
                        faces
                            .graph()
                            .bonds(id)
                            .filter_map(|(_, neighbor)| {
                                let next = if neighbor.a == id {
                                    neighbor.b
                                } else {
                                    neighbor.a
                                };
                                if !neighbor.is_covalent()
                                    || next == other
                                    || !faces.doc().bond_visible(id, next)
                                {
                                    return None;
                                }
                                let atom = faces.atom(next)?;
                                let (along, across, length) = if let Some(face) = &face {
                                    if !face.contains(&next) {
                                        return None;
                                    }
                                    // A retained face offsets in its own XYZ
                                    // plane; tilt must not change its angles.
                                    use crate::projection::vector::{cross, dot, sub, unit, xyz};
                                    let end = faces.atom(id)?;
                                    let axis = unit(sub(xyz(faces.atom(other)?), xyz(end)))?;
                                    let v = sub(xyz(atom), xyz(end));
                                    let perpendicular = cross(axis, v);
                                    (
                                        dot(axis, v),
                                        dot(perpendicular, perpendicular).sqrt(),
                                        dot(v, v).sqrt(),
                                    )
                                } else {
                                    let (dx, dy) =
                                        (atom.position.x - origin.x, atom.position.y - origin.y);
                                    (
                                        (ux * dx + uy * dy) * direction,
                                        (ux * dy - uy * dx) * side,
                                        dx.hypot(dy),
                                    )
                                };
                                // cot(theta/2) = (|v| + dot(u,v)) / |cross(u,v)|.
                                (across > 0.001).then(|| {
                                    (spacing * (length + along) / across)
                                        .clamp(spacing * 0.75, spacing * 2.)
                                })
                            })
                            .fold(0., f32::max)
                    };
                    inner.trim = (
                        trim(bond.a, bond.b, a.position, 1.),
                        trim(bond.b, bond.a, b.position, -1.),
                    );
                }
                vec![rail(0.), inner]
            }
            _ => vec![rail(-spacing / 2.), rail(spacing / 2.)],
        },
        3 => vec![rail(-spacing), rail(0.), rail(spacing)],
        4 => vec![
            rail(0.),
            rail(spacing * automatic_double_side(faces, bond).unwrap_or(1.)),
        ],
        6 => [-1.5, -0.5, 0.5, 1.5].map(|k| rail(spacing * k)).to_vec(),
        _ => vec![rail(0.)],
    }
}
