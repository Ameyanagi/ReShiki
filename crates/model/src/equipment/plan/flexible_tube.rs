//! Flexible tubing and optional rigid tube-adapter ends share the same route.
//! All geometry enters the ordinary paint plan, including picking and export.
use super::*;
use crate::equipment::geometry::len;

/// Existing rubber tubing overlaps a seated barb by this schematic depth.
const HOSE_CUFF_MM: f64 = 12.;

pub(super) fn paths(owner: usize, instance: &Instance, tube: &Tube) -> Vec<PlanPath> {
    let mut out = Vec::new();
    let mut push = |pass, depth, material, color, ink, path: Vec<Seg>| {
        if !path.is_empty() {
            out.push(PlanPath {
                owner,
                pass,
                depth,
                material,
                color,
                ink,
                style: None,
                flat_caps: false,
                path: instance.transform.map(&path),
            });
        }
    };
    let mut line = instance.centre_line.clone();
    // The cuff belongs to a female hose end. A metal tube-adapter tip seats
    // inside a compression fitting and must never acquire this extension.
    for (k, end) in tube.ends.iter().enumerate() {
        let hose_end = instance.definition.joints.iter().any(|j| {
            j.id() == end
                && matches!(
                    j,
                    Joint::Plug {
                        family: PlugFamily::Hose,
                        gender: Gender::F,
                        ..
                    }
                )
        });
        if hose_end && instance.seated.contains(end) {
            if k == 1 {
                line.reverse();
            }
            let a = line[0];
            if let Some(b) = line.iter().find(|p| len(sub(**p, a)) > 1e-9) {
                line.insert(0, add(a, mul(normalize(sub(a, *b)), HOSE_CUFF_MM)));
            }
            if k == 1 {
                line.reverse();
            }
        }
    }
    let route = Route::new(&line);
    let inset = tube.end_fittings.as_ref().map_or(0., |f| f.length_mm());
    let body = if inset == 0. {
        // Keep the legacy rubber-hose outline byte-for-byte equivalent.
        line.clone()
    } else {
        route.between(inset, route.length() - inset)
    };
    if body.len() >= 2 {
        push(
            Pass::Front,
            0,
            tube.material,
            tube.color,
            Ink::FillStroke(Weight::Outline),
            tube_outline(&body, tube.width_mm),
        );
    }
    if let Some(braid) = &tube.braid {
        push(
            Pass::Front,
            1,
            Material::Metal,
            None,
            Ink::Stroke(Weight::Hatch),
            braided(&Route::new(&body), tube.width_mm, braid.pitch_mm),
        );
    }
    if let Some(f) = &tube.end_fittings {
        for (k, end) in tube.ends.iter().enumerate() {
            let Some(Joint::Plug { size_mm, .. }) =
                instance.definition.joints.iter().find(|j| j.id() == end)
            else {
                continue;
            };
            let (p, facing) = crate::equipment::placed::end(&line, k);
            let into = mul(facing, -1.);
            let side = perp(into);
            let at = |x: f64, y: f64| add(p, add(mul(into, x), mul(side, y)));
            let tip = *size_mm / 2.;
            let collar = f.collar_width_mm / 2.;
            let shoulder = f.tip_length_mm;
            let back = f.length_mm();
            // The tip is behind the mating connector body. Its actual end
            // remains at the internal stop for checking and snapping.
            push(
                Pass::Back,
                0,
                Material::Metal,
                tube.color,
                Ink::FillStroke(Weight::Outline),
                polygon(
                    &[
                        at(0., -tip),
                        at(shoulder + 1., -tip),
                        at(shoulder + 1., tip),
                        at(0., tip),
                    ],
                    true,
                ),
            );
            let bevel = (f.collar_length_mm * 0.16).min(4.);
            push(
                Pass::Front,
                2,
                Material::Metal,
                tube.color,
                Ink::FillStroke(Weight::Outline),
                polygon(
                    &[
                        at(shoulder, -tip),
                        at(shoulder + bevel, -collar),
                        at(back - bevel, -collar),
                        at(back, -tube.width_mm / 2.),
                        at(back, tube.width_mm / 2.),
                        at(back - bevel, collar),
                        at(shoulder + bevel, collar),
                        at(shoulder, tip),
                    ],
                    true,
                ),
            );
            let mut seams = Vec::new();
            for x in [shoulder + bevel + 1.5, back - bevel - 1.5] {
                seams.extend([Seg::Move(at(x, -collar)), Seg::Line(at(x, collar))]);
            }
            push(
                Pass::Front,
                3,
                Material::Metal,
                None,
                Ink::Stroke(Weight::Hatch),
                seams,
            );
        }
    }
    out
}

/// Arc-length lookup on the already sampled centre line. The drawing's
/// geometric route controls decoration only, never a catalog assembly length.
struct Route<'a> {
    line: &'a [Xy],
    distances: Vec<f64>,
}

impl<'a> Route<'a> {
    fn new(line: &'a [Xy]) -> Self {
        let mut distances = vec![0.];
        for pair in line.windows(2) {
            distances.push(distances.last().unwrap() + len(sub(pair[1], pair[0])));
        }
        Self { line, distances }
    }

    fn length(&self) -> f64 {
        self.distances.last().copied().unwrap_or(0.)
    }

    fn at(&self, distance: f64, offset: f64) -> Xy {
        if self.line.len() < 2 {
            return self.line.first().copied().unwrap_or_default();
        }
        let distance = distance.clamp(0., self.length());
        let k = self
            .distances
            .partition_point(|d| *d < distance)
            .clamp(1, self.line.len() - 1);
        let (a, b) = (self.line[k - 1], self.line[k]);
        let delta = sub(b, a);
        let segment = len(delta);
        let along = if segment > 1e-9 {
            mul(delta, 1. / segment)
        } else {
            [1., 0.]
        };
        add(
            a,
            add(
                mul(along, distance - self.distances[k - 1]),
                mul(perp(along), offset),
            ),
        )
    }

    fn between(&self, start: f64, end: f64) -> Vec<Xy> {
        if self.line.len() < 2 || end <= start {
            return Vec::new();
        }
        let mut out = vec![self.at(start, 0.)];
        out.extend(
            self.line
                .iter()
                .zip(&self.distances)
                .filter_map(|(p, d)| (*d > start && *d < end).then_some(*p)),
        );
        out.push(self.at(end, 0.));
        out
    }
}

fn braided(route: &Route, width: f64, pitch: f64) -> Vec<Seg> {
    let length = route.length();
    if length <= 1e-9 {
        return Vec::new();
    }
    // Cap the work for extremely long or tightly pitched user definitions.
    // Each crossing follows the bent route with five short pieces.
    let pitch = pitch.max(length / 256.);
    let mut out = Vec::new();
    for i in 0..((length / pitch).ceil() as usize).min(256) {
        let start = i as f64 * pitch;
        for direction in [-1., 1.] {
            for k in 0..=5 {
                let t = k as f64 / 5.;
                let distance = start + t * pitch;
                if distance > length {
                    break;
                }
                let offset = direction * width * 0.45 * (2. * t - 1.);
                let p = route.at(distance, offset);
                out.push(if k == 0 { Seg::Move(p) } else { Seg::Line(p) });
            }
        }
    }
    out
}
