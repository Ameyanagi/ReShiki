//! ORTEP displacement ellipsoids in Cartesian ångström and their exact silhouettes.
use super::{Adp, BLACK, Cell, View, WHITE, length, mix, rgb, rotate, sub};
use crate::{
    document::Point,
    scene::{EllipsoidSection, Outline, Primitive},
};
use serde::{Deserialize, Serialize};

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// The probability enclosed by a three-dimensional displacement ellipsoid.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Probability {
    Thirty,
    #[default]
    Fifty,
    Seventy,
    Ninety,
}
impl Probability {
    pub const ALL: [Self; 4] = [Self::Thirty, Self::Fifty, Self::Seventy, Self::Ninety];

    pub const fn percent(self) -> u8 {
        match self {
            Self::Thirty => 30,
            Self::Fifty => 50,
            Self::Seventy => 70,
            Self::Ninety => 90,
        }
    }

    /// Square root of the chi-square quantile with three degrees of freedom.
    pub const fn scale(self) -> f32 {
        match self {
            Self::Thirty => 1.193_169,
            Self::Fifty => 1.538_172_2,
            Self::Seventy => 1.914_385_2,
            Self::Ninety => 2.500_277_8,
        }
    }
}

/// Orthogonal principal semiaxis vectors, already scaled to the probability.
#[derive(Debug, Clone, Copy)]
pub(super) struct Ellipsoid {
    pub axes: [[f32; 3]; 3],
    pub anisotropic: bool,
}

impl Ellipsoid {
    /// A non-positive-definite ADP has no displacement ellipsoid.
    #[allow(clippy::indexing_slicing)] // All matrix indices are in 0..3.
    pub fn of(adp: Adp, cell: Option<&Cell>, probability: Probability) -> Option<Self> {
        let covariance = match adp {
            Adp::Iso(u) => {
                return (u.is_finite() && u > 0.)
                    .then(|| Self::sphere(u.sqrt() * probability.scale()));
            }
            Adp::Aniso(u) => covariance(u, cell?),
        };
        let (values, vectors) = eigen(covariance);
        if values.iter().any(|v| !v.is_finite() || *v <= 0.) {
            return None;
        }
        Some(Self {
            axes: std::array::from_fn(|i| {
                let radius = values[i].sqrt() * probability.scale() as f64;
                vectors[i].map(|v| (v * radius) as f32)
            }),
            anisotropic: matches!(adp, Adp::Aniso(_)),
        })
    }

    pub fn sphere(radius: f32) -> Self {
        Self {
            axes: [[radius, 0., 0.], [0., radius, 0.], [0., 0., radius]],
            anisotropic: false,
        }
    }

    pub fn rotated(self, rotation: [f32; 4]) -> Self {
        Self {
            axes: self.axes.map(|a| rotate(rotation, a)),
            ..self
        }
    }

    pub fn radius(&self) -> f32 {
        self.axes.map(length).into_iter().fold(0., f32::max)
    }

    /// Distance from the centre to the surface along a unit direction.
    pub fn radius_along(&self, direction: [f32; 3]) -> f32 {
        let magnitude = length(direction);
        let inverse = self
            .axes
            .iter()
            .map(|a| {
                let component = dot(*a, direction) / dot(*a, *a);
                component * component
            })
            .sum::<f32>();
        magnitude / inverse.sqrt()
    }

    /// Project the dual quadric, then recover the centre and shape of its conic.
    /// This includes the silhouette's perspective shift away from the atom centre.
    #[allow(clippy::indexing_slicing)] // All matrix indices are in 0..3.
    pub fn project(&self, view: &View<'_>, center: [f32; 3]) -> Option<Projection> {
        let b: [[f64; 3]; 3] = std::array::from_fn(|i| {
            std::array::from_fn(|j| self.axes.iter().map(|a| a[i] as f64 * a[j] as f64).sum())
        });
        let [x, y, z] = center.map(f64::from);
        let (c, shape) = if view.perspective > 0. {
            let k = view.camera as f64;
            let h = k - z;
            let denominator = h * h - b[2][2];
            if denominator <= 0. {
                return None;
            }
            let c = [
                k * (b[0][2] + x * h) / denominator,
                k * (b[1][2] + y * h) / denominator,
            ];
            let shape = [
                c[0] * c[0] + k * k * (b[0][0] - x * x) / denominator,
                c[0] * c[1] + k * k * (b[0][1] - x * y) / denominator,
                c[1] * c[1] + k * k * (b[1][1] - y * y) / denominator,
            ];
            (c, shape)
        } else {
            ([x, y], [b[0][0], b[0][1], b[1][1]])
        };
        let scale = view.scale as f64;
        let [xx, xy, yy] = [
            shape[0] * scale * scale,
            -shape[1] * scale * scale,
            shape[2] * scale * scale,
        ];
        let theta = 0.5 * (2. * xy).atan2(xx - yy);
        let spread = ((xx - yy).powi(2) + 4. * xy * xy).sqrt();
        let radii = [(xx + yy + spread) * 0.5, (xx + yy - spread) * 0.5];
        if radii.iter().any(|r| !r.is_finite() || *r <= 0.) {
            return None;
        }
        let (s, ctheta) = theta.sin_cos();
        Some(Projection {
            center: view
                .origin
                .offset((c[0] * scale) as f32, (-c[1] * scale) as f32),
            axes: [
                Point::new(
                    (radii[0].sqrt() * ctheta) as f32,
                    (radii[0].sqrt() * s) as f32,
                ),
                Point::new(
                    (-radii[1].sqrt() * s) as f32,
                    (radii[1].sqrt() * ctheta) as f32,
                ),
            ],
        })
    }

    pub fn draw(
        &self,
        view: &View<'_>,
        center: [f32; 3],
        base: [f32; 3],
        glossy: bool,
        outline: Option<Outline>,
    ) -> Option<Primitive> {
        let projection = self.project(view, center)?;
        let shade = |stops: &[(f32, [f32; 3], f32)]| {
            stops
                .iter()
                .map(|&(offset, toward, t)| (offset, rgb(mix(base, toward, t))))
                .collect()
        };
        let stops = if glossy {
            shade(&[
                (0., WHITE, 0.85),
                (0.18, WHITE, 0.35),
                (0.75, base, 0.),
                (1., BLACK, 0.45),
            ])
        } else {
            shade(&[(0., WHITE, 0.45), (0.6, base, 0.), (1., BLACK, 0.18)])
        };
        let focal = projection
            .axes
            .map(|a| (-0.38 * a.x - 0.42 * a.y) / a.x.hypot(a.y));
        Some(Primitive::Ellipsoid {
            center: projection.center,
            axes: projection.axes,
            focal: Point::new(focal[0], focal[1]),
            stops,
            outline,
            section: self
                .anisotropic
                .then(|| self.sections(view, center, base, outline)),
        })
    }

    #[allow(clippy::indexing_slicing)] // Principal-axis pairs index the three fixed axes.
    fn sections(
        &self,
        view: &View<'_>,
        center: [f32; 3],
        base: [f32; 3],
        outline: Option<Outline>,
    ) -> EllipsoidSection {
        let eye = if view.perspective > 0. {
            sub([0., 0., view.camera], center)
        } else {
            [0., 0., 1.]
        };
        let pairs = [(0, 1), (1, 2), (2, 0)];
        let at = |a: [f32; 3], b: [f32; 3], t: f32| {
            view.project(std::array::from_fn(|i| {
                center[i] + a[i] * t.cos() + b[i] * t.sin()
            }))
            .0
        };
        let sample = |a, b, start: f32, end: f32, steps: usize| {
            (0..=steps)
                .map(|i| at(a, b, start + (end - start) * i as f32 / steps as f32))
                .collect::<Vec<_>>()
        };
        let arcs = pairs
            .into_iter()
            .filter_map(|(i, j)| {
                let [a, b] = [self.axes[i], self.axes[j]];
                let [x, y] = [a, b].map(|a| dot(a, eye) / dot(a, a));
                let radius = x.hypot(y);
                let threshold = if view.perspective > 0. { 1. } else { 0. };
                if radius <= threshold {
                    return None;
                }
                let middle = y.atan2(x);
                let half = (threshold / radius).acos();
                Some(sample(a, b, middle - half, middle + half, 64))
            })
            .collect();
        let facing = self
            .axes
            .map(|a| a.map(|v| v * if dot(a, eye) < 0. { -1. } else { 1. }));
        let octant = pairs
            .into_iter()
            .flat_map(|(i, j)| {
                sample(facing[i], facing[j], 0., std::f32::consts::FRAC_PI_2, 24)
                    .into_iter()
                    .take(24)
            })
            .collect();
        EllipsoidSection {
            octant,
            arcs,
            fill: rgb(mix(base, BLACK, 0.26)),
            color: rgb(mix(base, BLACK, 0.65)),
            width: outline.map_or(0.7, |o| o.width),
        }
    }
}

/// An ellipse in drawing units; its two axes are orthogonal semiaxis vectors.
#[derive(Debug, Clone, Copy)]
pub(super) struct Projection {
    pub center: Point,
    pub axes: [Point; 2],
}
impl Projection {
    #[cfg(test)]
    pub fn contains(&self, point: Point, margin: f32) -> bool {
        let d = Point::new(point.x - self.center.x, point.y - self.center.y);
        self.axes
            .iter()
            .map(|a| {
                let radius = a.x.hypot(a.y);
                ((d.x * a.x + d.y * a.y) / (radius * (radius + margin))).powi(2)
            })
            .sum::<f32>()
            <= 1.
    }
}

/// CIF U uses reciprocal lengths, not fractional-coordinate covariance.
/// Cell vectors are rows in storage and columns in A: U_cart = A N U N Aᵀ.
#[allow(clippy::indexing_slicing)] // All matrix indices are in 0..3.
fn covariance(u: [f32; 6], cell: &Cell) -> [[f64; 3]; 3] {
    let vectors = cell.vectors.map(|a| a.map(f64::from));
    let cross = |a: [f64; 3], b: [f64; 3]| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let length = |a: [f64; 3]| a.iter().map(|v| v * v).sum::<f64>().sqrt();
    let volume = cell.volume() as f64;
    let n = [
        cross(vectors[1], vectors[2]),
        cross(vectors[2], vectors[0]),
        cross(vectors[0], vectors[1]),
    ]
    .map(|a| length(a) / volume);
    let [xx, yy, zz, xy, xz, yz] = u.map(f64::from);
    let u = [[xx, xy, xz], [xy, yy, yz], [xz, yz, zz]];
    std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            (0..3)
                .flat_map(|a| {
                    (0..3).map(move |b| vectors[a][i] * n[a] * u[a][b] * n[b] * vectors[b][j])
                })
                .sum()
        })
    })
}

/// Symmetric 3×3 Jacobi eigendecomposition; eigenvectors are returned as rows.
#[allow(clippy::indexing_slicing)] // Jacobi pivots are fixed pairs within a 3×3 matrix.
fn eigen(mut a: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut v = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    for _ in 0..32 {
        let mut pair = (0, 1);
        for (i, j) in [(0, 2), (1, 2)] {
            if a[i][j].abs() > a[pair.0][pair.1].abs() {
                pair = (i, j);
            }
        }
        let (p, q) = pair;
        if a[p][q].abs()
            <= f64::EPSILON
                * a.iter()
                    .enumerate()
                    .map(|(i, row)| row[i].abs())
                    .fold(0., f64::max)
        {
            break;
        }
        let angle = 0.5 * (2. * a[p][q]).atan2(a[q][q] - a[p][p]);
        let (s, c) = angle.sin_cos();
        let [pp, qq, pq] = [a[p][p], a[q][q], a[p][q]];
        a[p][p] = c * c * pp - 2. * s * c * pq + s * s * qq;
        a[q][q] = s * s * pp + 2. * s * c * pq + c * c * qq;
        a[p][q] = 0.;
        a[q][p] = 0.;
        for k in 0..3 {
            if k != p && k != q {
                let [kp, kq] = [a[k][p], a[k][q]];
                a[k][p] = c * kp - s * kq;
                a[p][k] = a[k][p];
                a[k][q] = s * kp + c * kq;
                a[q][k] = a[k][q];
            }
            let [kp, kq] = [v[k][p], v[k][q]];
            v[k][p] = c * kp - s * kq;
            v[k][q] = s * kp + c * kq;
        }
    }
    (
        [a[0][0], a[1][1], a[2][2]],
        std::array::from_fn(|i| std::array::from_fn(|j| v[j][i])),
    )
}

#[cfg(test)]
mod tests;
