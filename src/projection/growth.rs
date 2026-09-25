//! Bond growth in a Cp/aromatic ring's retained plane. No conformer inference.
use crate::{
    bonds::BondPreset,
    chains::BondDrawing,
    document::{Atom, Document, Point},
};

type V = [f32; 3];
fn xyz(a: &Atom) -> V {
    [a.position.x, a.position.y, a.depth]
}
fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: V, b: V) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}
fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
fn unit(a: V) -> Option<V> {
    let length = dot(a, a).sqrt();
    (length.is_finite() && length > 0.001).then(|| a.map(|v| v / length))
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Endpoint {
    pub position: Point,
    pub depth: f32,
}

pub struct Plane {
    origin: V,
    u: V,
    v: V,
    normal: V,
}
impl Plane {
    pub fn at(doc: &Document, id: u64) -> Option<Self> {
        let atom = doc.atom(id)?;
        if !atom.centroid.is_empty() || doc.abbreviation(id).is_some() {
            return None;
        }
        let mut result: Option<Self> = None;
        for ring in crate::aromatic::ring_circles(doc, false) {
            if !ring.atoms.contains(&id) {
                continue;
            }
            let atoms: Option<Vec<_>> = ring.atoms.iter().map(|i| doc.atom(*i)).collect();
            let atoms = atoms?;
            let bonds: Vec<_> = doc
                .bonds
                .iter()
                .filter(|b| ring.contains_bond(b.a, b.b))
                .collect();
            let aromatic = atoms.iter().all(|a| a.aromatic)
                || bonds.iter().all(|b| b.order == 4)
                || (atoms.len() % 4 == 2
                    && atoms.iter().all(|a| {
                        bonds
                            .iter()
                            .filter(|b| b.order == 2 && (b.a == a.id || b.b == a.id))
                            .count()
                            == 1
                    }))
                || (atoms.len() == 5
                    && atoms.iter().all(|a| a.element == "C")
                    && atoms.iter().map(|a| a.charge).sum::<i32>() == -1
                    && bonds.iter().filter(|b| b.order == 2).count() == 2);
            if !aromatic {
                continue;
            }
            let mut center = [0.; 3];
            for atom in &atoms {
                for (value, coordinate) in center.iter_mut().zip(xyz(atom)) {
                    *value += coordinate / atoms.len() as f32;
                }
            }
            let origin = xyz(atom);
            let u = unit(sub(origin, center))?;
            let normal = atoms
                .iter()
                .find_map(|a| unit(cross(u, sub(xyz(a), center))))?;
            if atoms
                .iter()
                .any(|a| dot(sub(xyz(a), center), normal).abs() > 0.01)
            {
                return None;
            }
            // At a fused junction, never guess between incompatible planes.
            if result
                .as_ref()
                .is_some_and(|p| dot(p.normal, normal).abs() < 0.999)
            {
                return None;
            }
            if result.is_none() {
                result = Some(Self {
                    origin,
                    u,
                    v: cross(normal, u),
                    normal,
                });
            }
        }
        result
    }

    fn point(&self, length: f32, angle: f32) -> Option<Endpoint> {
        let (sin, cos) = angle.sin_cos();
        let mut p = self.origin;
        for ((value, u), v) in p.iter_mut().zip(self.u).zip(self.v) {
            *value += length * (u * cos + v * sin);
        }
        p.iter()
            .all(|v| v.is_finite() && v.abs() <= 1_000_000.)
            .then_some(Endpoint {
                position: Point::new(p[0], p[1]),
                depth: p[2],
            })
    }

    pub fn outward(&self, length: f32) -> Option<Endpoint> {
        self.point(length, 0.)
    }

    pub fn endpoint(&self, cursor: Point, drawing: BondDrawing) -> Option<Endpoint> {
        if !cursor.x.is_finite()
            || !cursor.y.is_finite()
            || !drawing.length.is_finite()
            || drawing.length <= 0.
        {
            return None;
        }
        let x = cursor.x - self.origin[0];
        let y = cursor.y - self.origin[1];
        let determinant = self.u[0] * self.v[1] - self.u[1] * self.v[0];
        if determinant.abs() > 0.02 {
            let u = (x * self.v[1] - y * self.v[0]) / determinant;
            let v = (self.u[0] * y - self.u[1] * x) / determinant;
            let length = if drawing.fixed_length {
                drawing.length
            } else {
                u.hypot(v)
            };
            return self.point(length, drawing.angle(v.atan2(u)));
        }
        // A nearly edge-on plane cannot be inverted reliably. Choose a bounded
        // projected candidate, preferring the outward direction on a tie.
        let steps = if drawing.fixed_angles { 24 } else { 720 };
        let mut best = None;
        let mut score = f32::INFINITY;
        for i in 0..steps {
            let angle = i as f32 * std::f32::consts::TAU / steps as f32;
            let (sin, cos) = angle.sin_cos();
            let dx = self.u[0] * cos + self.v[0] * sin;
            let dy = self.u[1] * cos + self.v[1] * sin;
            let norm = dx * dx + dy * dy;
            if norm < 0.0025 {
                continue;
            }
            let length = if drawing.fixed_length {
                drawing.length
            } else {
                ((x * dx + y * dy) / norm).max(0.)
            };
            let distance = (length * dx - x).hypot(length * dy - y);
            if distance < score - 0.001 {
                score = distance;
                best = self.point(length, angle);
            }
        }
        best
    }
}

/// Explicit ring C–H is replaced by the new single substituent bond.
pub fn replaces_hydrogen(doc: &Document, id: u64, preset: BondPreset) -> bool {
    preset.parts().0 == 1
        && doc
            .atom(id)
            .is_some_and(|a| a.element == "C" && a.explicit_h == 1)
        && doc.bonds.iter().filter(|b| b.a == id || b.b == id).count() == 2
        && Plane::at(doc, id).is_some()
}

/// Shared by preview, mouse placement and keyboard growth.
pub fn place(
    source: &Document,
    start: u64,
    end: Endpoint,
    element: &str,
    preset: BondPreset,
) -> Result<(Document, u64), String> {
    source.validate()?;
    let plane = Plane::at(source, start).ok_or("No unambiguous aromatic ring plane")?;
    let offset = sub([end.position.x, end.position.y, end.depth], plane.origin);
    if offset.iter().any(|v| !v.is_finite())
        || dot(offset, plane.normal).abs() > 0.02
        || dot(offset, offset) < 0.000001
    {
        return Err("Invalid bond endpoint in the ring plane".into());
    }
    if element != "*" && !crate::editing::ELEMENTS.contains(&element) {
        return Err("Choose an element first".into());
    }
    if source.atoms.len() >= 100_000
        || source
            .next_id()
            .checked_add(1)
            .is_none_or(|n| n == u64::MAX)
    {
        return Err("Drawing atom limit reached".into());
    }
    let mut doc = source.clone();
    if replaces_hydrogen(source, start, preset) {
        let atom = doc.atom_mut(start).ok_or("Missing starting atom")?;
        atom.explicit_h = 0;
    }
    let id = doc.add_atom(element, end.position);
    doc.atom_mut(id).ok_or("Missing new atom")?.depth = end.depth;
    doc.invalidate_chemistry(&[start, id]);
    preset.place(&mut doc, start, id);
    doc.validate()?;
    Ok((doc, id))
}
