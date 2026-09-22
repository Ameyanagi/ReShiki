use super::*;
use crate::scientific::{OrbitalKind, SymbolKind};

impl Reader<'_, '_> {
    pub(super) fn curve(&mut self, el: Node<'_, '_>) -> Result<NativeGraphic> {
        let flags = numeric::integer(el.attribute("CurveType").unwrap_or("0"))?;
        if flags.is_none_or(|f| f & !(1 | 2 | 4 | 128) != 0)
            || !matches!(
                el.attribute("FillType").unwrap_or("None"),
                "None" | "Solid" | "Unspecified"
            )
            || !matches!(
                el.attribute("LineType").unwrap_or("Solid"),
                "Solid" | "Dashed" | "Bold"
            )
            || ["ArrowheadHead", "ArrowheadTail"]
                .iter()
                .any(|key| !matches!(el.attribute(*key).unwrap_or("None"), "None" | "Unspecified"))
        {
            return Err(Error::Native(
                "Arrowed, shaded or doubled CDXML curves are not supported yet",
            ));
        }
        let flags = flags.unwrap_or(0);
        let text = required(el, "CurvePoints")?;
        self.spend(text.len())?;
        let count = text.split_whitespace().count();
        if count < 12 || !count.is_multiple_of(6) {
            return Err(Error::Native("Invalid CDXML Bézier control-point sequence"));
        }
        self.points = self.points.checked_add(count / 2).ok_or(Error::Limit)?;
        if self.points > PATH_LIMIT {
            return Err(Error::Limit);
        }
        let mut values = text.split_whitespace();
        let mut points = Vec::with_capacity(count / 2);
        while let Some(a) = values.next() {
            let b = values.next().ok_or(Error::Limit)?;
            points.push(self.point(&format!("{a} {b}"))?);
        }
        let at = |i| points.get(i).copied().ok_or(Error::Limit);
        let first = at(1)?;
        let mut path = vec![NativeCommand::Move(first)];
        for i in (3..points.len()).step_by(3) {
            path.push(NativeCommand::Cubic(at(i - 1)?, at(i)?, at(i + 1)?));
        }
        if flags & 1 != 0 || el.attribute("Closed") == Some("yes") {
            if at(points.len() - 2)? != first {
                path.push(NativeCommand::Cubic(at(points.len() - 1)?, at(0)?, first));
            }
            path.push(NativeCommand::Close);
        }
        let style = self.style(
            el,
            flags & 128 != 0 || el.attribute("FillType") == Some("Solid"),
            flags & 2 != 0 || el.attribute("LineType") == Some("Dashed"),
            flags & 4 != 0 || el.attribute("LineType") == Some("Bold"),
        )?;
        self.base(
            el,
            GraphicKind::Path,
            [
                NativePoint::new(0., 0.),
                NativePoint::new(1., 0.),
                NativePoint::new(0., 1.),
            ],
            style,
            path,
        )
    }
    pub(super) fn graphic(&mut self, el: Node<'_, '_>) -> Result<NativeGraphic> {
        let text = el.attribute("BoundingBox").unwrap_or("");
        self.spend(text.len())?;
        let values: Vec<_> = text.split_whitespace().take(5).collect();
        let [a, b, c, d] = values.as_slice() else {
            return Err(Error::Native("Graphic is missing its two defining points"));
        };
        let a = self.point(&format!("{a} {b}"))?;
        let b = self.point(&format!("{c} {d}"))?;
        // Python min preserves its first argument on equal/unordered operands.
        let mut origin = NativePoint::new(
            if b.x < a.x { b.x } else { a.x },
            if b.y < a.y { b.y } else { a.y },
        );
        let mut x = NativePoint::new((b.x - a.x).abs(), 0.);
        let mut y = NativePoint::new(0., (b.y - a.y).abs());
        if el.attribute("ArrowType").unwrap_or("NoHead") != "NoHead"
            || el.attribute("BracketUsage").unwrap_or("Unspecified") != "Unspecified"
        {
            return Err(Error::Native(
                "Chemical brackets and legacy graphic arrows need a dedicated import path",
            ));
        }
        if el.attribute("GraphicType") == Some("Orbital") {
            let label = el.attribute("OrbitalType").unwrap_or("");
            if label.contains("Shaded") {
                return Err(Error::Native(
                    "Gradient-shaded CDXML orbitals are not supported yet",
                ));
            }
            let key = label.replace("Shaded", "").replace("Solid", "");
            let key = if label.starts_with("dz2") {
                "dz2"
            } else {
                key.strip_suffix('2').unwrap_or(&key)
            };
            let kind = match key {
                "s" => OrbitalKind::S,
                "oval" | "sigma" => OrbitalKind::Sigma,
                "lobe" | "sp" => OrbitalKind::Lobe,
                "p" => OrbitalKind::P,
                "sp3" => OrbitalKind::Hybrid,
                "dxy" => OrbitalKind::Dxy,
                "dz2" => OrbitalKind::Dz2,
                _ => return Err(Error::Native("Unsupported CDXML orbital")),
            };
            let x = NativePoint::new(a.x - b.x, a.y - b.y);
            let mut value = self.base(
                el,
                GraphicKind::Orbital(kind),
                [b, x, NativePoint::new(-x.y, x.x)],
                self.style(el, false, false, false)?,
                vec![],
            )?;
            value.phase = Some(if label.contains("Solid") {
                Phase::Solid
            } else {
                Phase::Open
            });
            value.phase_flipped = Some(label.ends_with('2') && label != "dz2");
            return Ok(value);
        }
        if el.attribute("GraphicType") == Some("Symbol") {
            let kind = match el.attribute("SymbolType") {
                Some("CirclePlus") => SymbolKind::CirclePlus,
                Some("CircleMinus") => SymbolKind::CircleMinus,
                Some("Plus") => SymbolKind::Plus,
                Some("Minus") => SymbolKind::Minus,
                Some("Radical" | "Electron") => SymbolKind::Radical,
                Some("LonePair" | "ElectronPair") => SymbolKind::LonePair,
                Some("RadicalCation") => SymbolKind::RadicalCation,
                Some("RadicalAnion") => SymbolKind::RadicalAnion,
                Some("HDot") => SymbolKind::HydrogenDot,
                Some("HDash") => SymbolKind::HydrogenDash,
                Some("Attachment") => SymbolKind::Diamond,
                _ => return Err(Error::Native("Unsupported CDXML chemical symbol")),
            };
            if el.attribute("ChemicallySignificant") == Some("yes")
                || el.attribute("ObjectID").is_some_and(|s| !s.is_empty())
            {
                return Err(Error::Native(
                    "Attached CDXML symbols require a dedicated chemical attachment importer",
                ));
            }
            if symbol_too_small(b.x - a.x, b.y - a.y) {
                return Err(Error::Native("Invalid CDXML symbol size"));
            }
            let mut x = NativePoint::new(a.x - b.x, a.y - b.y);
            if matches!(
                el.attribute("SymbolType"),
                Some("Electron" | "Radical" | "LonePair" | "ElectronPair")
            ) {
                x.x *= 2.;
                x.y *= 2.;
            }
            return self.base(
                el,
                GraphicKind::Symbol(kind),
                [a, x, NativePoint::new(-x.y, x.x)],
                self.style(el, false, false, false)?,
                vec![],
            );
        }
        let (kind, filled, dashed, bold) = match el.attribute("GraphicType") {
            Some("Line") => {
                let line = el.attribute("LineType").unwrap_or("Solid");
                if !matches!(line, "Solid" | "Dashed" | "Bold") {
                    return Err(Error::Native("Unsupported graphic line style"));
                }
                origin = a;
                x = NativePoint::new(b.x - a.x, b.y - a.y);
                y = NativePoint::new(0., 0.);
                (GraphicKind::Line, false, line == "Dashed", line == "Bold")
            }
            Some("Rectangle") => {
                let flags: BTreeSet<_> = el
                    .attribute("RectangleType")
                    .unwrap_or("Plain")
                    .split_whitespace()
                    .collect();
                if flags
                    .iter()
                    .any(|f| !matches!(*f, "Plain" | "RoundEdge" | "Filled" | "Dashed" | "Bold"))
                {
                    return Err(Error::Native(
                        "Shadowed/shaded rectangle styles are not supported yet",
                    ));
                }
                (
                    if flags.contains("RoundEdge") {
                        GraphicKind::RoundedRectangle
                    } else {
                        GraphicKind::Rectangle
                    },
                    flags.contains("Filled"),
                    flags.contains("Dashed"),
                    flags.contains("Bold"),
                )
            }
            Some("Bracket") => {
                let kind = match el.attribute("BracketType").unwrap_or("RoundPair") {
                    "SquarePair" => GraphicKind::Brackets,
                    "RoundPair" => GraphicKind::Parentheses,
                    "CurlyPair" => GraphicKind::Braces,
                    _ => {
                        return Err(Error::Native(
                            "Single CDXML brackets need explicit orientation; import as curves",
                        ));
                    }
                };
                (kind, false, false, false)
            }
            Some("Oval")
                if ["Center3D", "MajorAxisEnd3D", "MinorAxisEnd3D"]
                    .iter()
                    .all(|k| el.has_attribute(*k)) =>
            {
                let center = self.point(required(el, "Center3D")?)?;
                let major = self.point(required(el, "MajorAxisEnd3D")?)?;
                let minor = self.point(required(el, "MinorAxisEnd3D")?)?;
                x = NativePoint::new(2. * (major.x - center.x), 2. * (major.y - center.y));
                y = NativePoint::new(2. * (minor.x - center.x), 2. * (minor.y - center.y));
                origin = NativePoint::new(center.x - (x.x + y.x) / 2., center.y - (x.y + y.y) / 2.);
                let flags: BTreeSet<_> = el
                    .attribute("OvalType")
                    .unwrap_or("Plain")
                    .split_whitespace()
                    .collect();
                if flags
                    .iter()
                    .any(|f| !matches!(*f, "Plain" | "Circle" | "Filled" | "Dashed" | "Bold"))
                {
                    return Err(Error::Native("Unsupported oval style"));
                }
                (
                    GraphicKind::Ellipse,
                    flags.contains("Filled"),
                    flags.contains("Dashed"),
                    flags.contains("Bold"),
                )
            }
            _ => {
                return Err(Error::Native(
                    "This CDXML graphic type is not supported yet; convert it to curves first",
                ));
            }
        };
        self.base(
            el,
            kind,
            [origin, x, y],
            self.style(el, filled, dashed, bold)?,
            vec![],
        )
    }
}

/// Only the 0.001 decision needs hypot here. Far from that boundary the native
/// result cannot change acceptance; near it use CPython's compensated norm.
/// Adapted from CPython Modules/mathmodule.c vector_norm (PSF license).
fn symbol_too_small(x: f64, y: f64) -> bool {
    let maximum = x.abs().max(y.abs());
    if maximum >= 0.001 || !x.is_finite() || !y.is_finite() {
        return false;
    }
    if maximum < 0.0005 {
        return true;
    }
    let exponent = ((maximum.to_bits() >> 52) & 0x7ff) as i32;
    let scale = 2.0_f64.powi(1022 - exponent);
    let (mut sum, mut products, mut additions) = (1.0, 0.0, 0.0);
    for value in [x.abs(), y.abs()] {
        let value = value * scale;
        let square = value * value;
        let error = value.mul_add(value, -square);
        let combined = sum + square;
        additions += square - (combined - sum);
        products += error;
        sum = combined;
    }
    let mut norm = (sum - 1.0 + (products + additions)).sqrt();
    let square = -norm * norm;
    let error = (-norm).mul_add(norm, -square);
    let combined = sum + square;
    additions += square - (combined - sum);
    products += error;
    sum = combined;
    norm += (sum - 1.0 + (products + additions)) / (2.0 * norm);
    norm / scale < 0.001
}
