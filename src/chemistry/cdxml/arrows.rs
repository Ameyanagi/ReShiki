//! Native-precision arrows from engine/arrows_exchange.py::read_arrow.
use super::{
    Error as XmlError, ImportPoint,
    abbreviations::python_number,
    numeric,
    presentation::{self, NativeColor},
    tree::{Element, Tree},
};
use crate::{
    arrows::{ArrowStyle, Head, HeadShape, NoGo, Preset},
    document::{Arrow, Point},
    graphics::LinePattern,
};
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum ArrowError {
    #[error(transparent)]
    Xml(#[from] XmlError),
    #[error(transparent)]
    Numeric(#[from] presentation::NumericError),
    #[error(transparent)]
    Presentation(#[from] presentation::Error),
    #[error("{0}")]
    Invalid(&'static str),
    #[error("'{0}'")]
    Missing(&'static str),
    #[error("Unsupported CDXML arrow {0}")]
    Range(&'static str),
    #[error("Arrow coordinates exceed the finite document range")]
    DocumentPoint,
}
type Result<T> = std::result::Result<T, ArrowError>;
#[derive(Clone, Debug, Serialize)]
pub struct NativeArrowStyle {
    pub head: Head,
    pub tail: Head,
    pub shape: HeadShape,
    pub color: NativeColor,
    pub width_pt: f64,
    pub pattern: LinePattern,
    pub head_length_pt: f64,
    pub head_width_pt: f64,
    pub head_notch: f64,
    pub equilibrium_ratio: f64,
    pub gap_pt: f64,
    pub no_go: NoGo,
    pub dipole: bool,
}
#[derive(Clone, Debug, Serialize)]
pub struct NativeArrow {
    pub id: u64,
    pub kind: Preset,
    pub style: NativeArrowStyle,
    pub start: ImportPoint,
    pub end: ImportPoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub control: Option<ImportPoint>,
}
impl NativeArrow {
    /// Explicit narrowing after native helper acceptance. Palette channels and
    /// coordinates may be accepted by the helper but not fit document types.
    pub fn into_document(self) -> Result<Arrow> {
        let s = self.style;
        Ok(Arrow {
            id: self.id,
            kind: self.kind.kind().into(),
            start: drawing_point(self.start)?,
            end: drawing_point(self.end)?,
            control: self.control.map(drawing_point).transpose()?,
            style: Some(ArrowStyle {
                head: s.head,
                tail: s.tail,
                shape: s.shape,
                color: s.color.into_document()?,
                width_pt: s.width_pt as f32,
                pattern: s.pattern,
                head_length_pt: s.head_length_pt as f32,
                head_width_pt: s.head_width_pt as f32,
                head_notch: s.head_notch as f32,
                equilibrium_ratio: s.equilibrium_ratio as f32,
                gap_pt: s.gap_pt as f32,
                no_go: s.no_go,
                dipole: s.dipole,
            }),
        })
    }
}
fn drawing_point(value: ImportPoint) -> Result<Point> {
    let point = Point {
        x: value.x as f32,
        y: value.y as f32,
    };
    if !point.x.is_finite() || !point.y.is_finite() {
        return Err(ArrowError::DocumentPoint);
    }
    Ok(point)
}
fn scalar(node: &Element, key: &str, default: &str) -> Result<f64> {
    Ok(numeric::float(node.attr(key).unwrap_or(default))?)
}
fn head(value: &str) -> Result<Head> {
    match value {
        "None" => Ok(Head::None),
        "Full" => Ok(Head::Full),
        "HalfLeft" => Ok(Head::Left),
        "HalfRight" => Ok(Head::Right),
        _ => Err(ArrowError::Invalid("Unsupported CDXML arrowhead")),
    }
}
struct Flags {
    bold: bool,
    dashed: bool,
    unsupported: bool,
}
fn flags(text: &str) -> Result<Flags> {
    let value = numeric::integer(text)?;
    let (low, unsupported) = if let Some(value) = value {
        ((value & 127) as u8, value & !126 != 0)
    } else {
        // Python integers are unbounded. Very large straight-arrow CurveType
        // values still contribute their low bold bit; curves reject high bits.
        let normalized =
            python_number(text.trim()).ok_or(ArrowError::Invalid("Invalid curve flags"))?;
        let digits = normalized.strip_prefix(['+', '-']).unwrap_or(&normalized);
        let mut low = 0u16;
        for digit in digits.bytes() {
            let digit = digit
                .checked_sub(b'0')
                .filter(|&n| n <= 9)
                .ok_or(ArrowError::Invalid("Invalid curve flags"))?;
            low = (low * 10 + u16::from(digit)) % 128;
        }
        if normalized.starts_with('-') {
            low = (128 - low) % 128;
        }
        (low as u8, true)
    };
    Ok(Flags {
        bold: low & 4 != 0,
        dashed: low & 2 != 0,
        unsupported,
    })
}
fn point(text: &str, scale: f64) -> Result<ImportPoint> {
    let values = text
        .split_whitespace()
        .map(numeric::float)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let (Some(&x), Some(&y)) = (values.first(), values.get(1)) else {
        return Err(ArrowError::Invalid("Invalid CDXML coordinates"));
    };
    if values.iter().any(|v| !v.is_finite()) {
        return Err(ArrowError::Invalid("Invalid CDXML coordinates"));
    }
    Ok(ImportPoint {
        x: x * scale,
        y: y * scale,
    })
}

/// Reuse one bounded detached source tree for every arrow. Source ordinals are
/// root-inclusive XML element ordinals, independent of missing/duplicate IDs.
/// Neither source XML nor the caller's palette or existing document is mutated.
pub struct ArrowReader {
    tree: Tree,
    order: Vec<usize>,
}
impl ArrowReader {
    pub fn new(xml: &str) -> Result<Self> {
        let tree = Tree::parse(xml)?;
        let order = tree.descendants(0)?;
        Ok(Self { tree, order })
    }
    pub fn read(
        &self,
        source: usize,
        source_scale: f64,
        colors: &[NativeColor],
        identifier: u64,
    ) -> Result<NativeArrow> {
        if colors.len() > 100_000 {
            return Err(XmlError::Limit.into());
        }
        if !source_scale.is_finite() {
            return Err(ArrowError::Invalid("Nonfinite source coordinate scale"));
        }
        let node = self
            .tree
            .node(*self.order.get(source).ok_or(ArrowError::Invalid(
                "Arrow source ordinal is outside the source document",
            ))?)?;
        let root = self.tree.node(0)?;
        for (key, value) in node.attributes.iter().chain(&root.attributes) {
            self.tree.spend(key.len().saturating_add(value.len()))?;
        }
        if scalar(node, "AngularSize", "0")? != 0. {
            return Err(ArrowError::Invalid(
                "Circular/elliptical CDXML arrows are not supported yet; use a Bézier arrow",
            ));
        }
        if !matches!(
            node.attr("FillType").unwrap_or("None"),
            "None" | "Unspecified"
        ) || scalar(node, "FadePercent", "100")? != 100.
        {
            return Err(ArrowError::Invalid(
                "Filled or faded CDXML arrows are not supported yet",
            ));
        }
        let line_type = node.attr("LineType").unwrap_or("Solid");
        if !matches!(line_type, "Solid" | "Dashed" | "Bold") {
            return Err(ArrowError::Invalid("Unsupported CDXML arrow stroke"));
        }
        let head = head(node.attr("ArrowheadHead").unwrap_or("None"))?;
        let tail = self::head(node.attr("ArrowheadTail").unwrap_or("None"))?;
        let shape = match node.attr("ArrowheadType").unwrap_or("Solid") {
            "Solid" => HeadShape::Solid,
            "Hollow" => HeadShape::Hollow,
            "Angle" => HeadShape::Open,
            _ => return Err(ArrowError::Invalid("Unsupported CDXML arrowhead")),
        };
        let color = numeric::integer(
            node.attr("color")
                .or_else(|| root.attr("color"))
                .unwrap_or("3"),
        )?
        .and_then(|v| usize::try_from(v).ok())
        .and_then(|i| colors.get(i))
        .copied()
        .ok_or(ArrowError::Invalid("Invalid arrow color"))?;
        let length = scalar(node, "HeadSize", "1000")? / 100.;
        let width = scalar(node, "ArrowheadWidth", "250")? / 100.;
        // Preserve the original default's multiply/string-roundtrip/divide order.
        let center = if let Some(value) = node.attr("ArrowheadCenterSize") {
            numeric::float(value)?
        } else {
            length * 100.
        } / 100.;
        let bold = line_type == "Bold" || flags(node.attr("CurveType").unwrap_or("0"))?.bold;
        let width_key = if bold { "BoldWidth" } else { "LineWidth" };
        let line = numeric::float(
            node.attr(width_key)
                .or_else(|| root.attr(width_key))
                .unwrap_or(if bold { "2" } else { "0.6" }),
        )?;
        let gap = scalar(node, "ArrowShaftSpacing", "0")? / 100.;
        let no_go = match node.attr("NoGo").unwrap_or("None").to_lowercase().as_str() {
            "none" => NoGo::None,
            "cross" => NoGo::Cross,
            "hash" => NoGo::Hash,
            _ => return Err(ArrowError::Invalid("Unsupported CDXML no-go mark")),
        };
        if length <= 0. || !(0. <= center && center <= length) {
            return Err(ArrowError::Invalid("Unsupported arrowhead center size"));
        }
        let mut style = NativeArrowStyle {
            head,
            tail,
            shape,
            color,
            width_pt: line,
            pattern: if line_type == "Dashed" {
                LinePattern::Dashed
            } else {
                LinePattern::Solid
            },
            head_length_pt: length,
            head_width_pt: width,
            head_notch: 1. - center / length,
            equilibrium_ratio: 1.,
            gap_pt: if gap == 0. { 2. } else { gap },
            no_go,
            dipole: node.attr("Dipole") == Some("yes"),
        };
        for (key, value, lo, hi) in [
            ("width_pt", style.width_pt, 0.1, 12.),
            ("head_length_pt", length, 0.5, 24.),
            ("head_width_pt", width, 0.25, 16.),
            ("head_notch", style.head_notch, 0., 0.9),
            ("gap_pt", style.gap_pt, 0.5, 16.),
        ] {
            if !value.is_finite() || !(lo..=hi).contains(&value) {
                return Err(ArrowError::Range(key));
            }
        }
        let kind = if gap != 0. {
            Preset::Equilibrium
        } else if head != Head::None && tail != Head::None {
            Preset::Resonance
        } else {
            Preset::Forward
        };
        if node.tag != "curve" {
            return Ok(NativeArrow {
                id: identifier,
                kind,
                style,
                start: point(
                    node.attr("Tail3D").ok_or(ArrowError::Missing("Tail3D"))?,
                    source_scale,
                )?,
                end: point(
                    node.attr("Head3D").ok_or(ArrowError::Missing("Head3D"))?,
                    source_scale,
                )?,
                control: None,
            });
        }
        let flags = flags(node.attr("CurveType").unwrap_or("0"))?;
        if flags.unsupported || node.attr("Closed") == Some("yes") || gap != 0. {
            return Err(ArrowError::Invalid(
                "This arrowed CDXML spline is not supported yet",
            ));
        }
        let values = node
            .attr("CurvePoints")
            .unwrap_or("")
            .split_whitespace()
            .collect::<Vec<_>>();
        if !matches!(values.len(), 12 | 18) {
            return Err(ArrowError::Invalid(
                "Only quadratic and single-elbow arrow curves are supported",
            ));
        }
        let points = values
            .chunks_exact(2)
            .map(|pair| point(&pair.join(" "), source_scale))
            .collect::<Result<Vec<_>>>()?;
        if flags.dashed {
            style.pattern = LinePattern::Dashed;
        }
        if let [_, start, p1, p2, corner, p3, p4, end, _] = points.as_slice() {
            if ![
                (p1, start, corner),
                (p2, start, corner),
                (p3, corner, end),
                (p4, corner, end),
            ]
            .into_iter()
            .all(|(p, a, b)| on_segment(*p, *a, *b))
            {
                return Err(ArrowError::Invalid(
                    "This multi-segment curved arrow cannot be represented by an elbow",
                ));
            }
            return Ok(NativeArrow {
                id: identifier,
                kind: Preset::Bent,
                style,
                start: *start,
                end: *end,
                control: Some(*corner),
            });
        }
        let [_, start, a, b, end, _] = points.as_slice() else {
            return Err(ArrowError::Invalid(
                "Only quadratic and single-elbow arrow curves are supported",
            ));
        };
        let c1 = ImportPoint {
            x: start.x + 1.5 * (a.x - start.x),
            y: start.y + 1.5 * (a.y - start.y),
        };
        let c2 = ImportPoint {
            x: end.x + 1.5 * (b.x - end.x),
            y: end.y + 1.5 * (b.y - end.y),
        };
        if native_hypot(c1.x - c2.x, c1.y - c2.y) > 0.15 {
            return Err(ArrowError::Invalid(
                "This cubic arrow cannot be represented by a single quadratic bend",
            ));
        }
        Ok(NativeArrow {
            id: identifier,
            kind: if matches!(head, Head::Left | Head::Right) {
                Preset::Fishhook
            } else {
                Preset::Curved
            },
            style,
            start: *start,
            end: *end,
            control: Some(ImportPoint {
                x: (c1.x + c2.x) / 2.,
                y: (c1.y + c2.y) / 2.,
            }),
        })
    }
}
fn on_segment(p: ImportPoint, a: ImportPoint, b: ImportPoint) -> bool {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    let length = native_hypot(dx, dy);
    if length < 1e-6 {
        return native_hypot(p.x - a.x, p.y - a.y) < 0.15;
    }
    let cross = ((p.x - a.x) * dy - (p.y - a.y) * dx).abs() / length;
    let t = ((p.x - a.x) * dx + (p.y - a.y) * dy) / (length * length);
    cross < 0.15 && (0.0..=1.0).contains(&t)
}

/// Two-coordinate CPython 3.12.12 vector_norm (Modules/mathmodule.c), including
/// infinity/NaN precedence and subnormal scaling. Copyright Python Software
/// Foundation, PSF license; see licenses/cpython. Multiplication error uses FMA.
fn native_hypot(x: f64, y: f64) -> f64 {
    let (mut x, mut y) = (x.abs(), y.abs());
    let mut maximum = x.max(y);
    if maximum.is_infinite() {
        return maximum;
    }
    if x.is_nan() || y.is_nan() {
        return f64::NAN;
    }
    if maximum == 0. {
        return maximum;
    }
    let restore = if maximum < f64::MIN_POSITIVE / 4. {
        x /= f64::MIN_POSITIVE;
        y /= f64::MIN_POSITIVE;
        maximum /= f64::MIN_POSITIVE;
        f64::MIN_POSITIVE
    } else {
        1.
    };
    let biased = ((maximum.to_bits() >> 52) & 0x7ff) as i32;
    let exponent = if biased == 0 {
        63 - maximum.to_bits().leading_zeros() as i32 - 1073
    } else {
        biased - 1022
    };
    // ldexp(1,-exponent), constructed exactly even when the scale is subnormal.
    let power = -exponent;
    let scale = f64::from_bits(match power {
        -1024 => 1 << 50,
        -1023 => 1 << 51,
        _ => ((power + 1023) as u64) << 52,
    });
    let (mut sum, mut products, mut additions) = (1., 0., 0.);
    for value in [x, y] {
        let value = value * scale;
        let square = value * value;
        let error = value.mul_add(value, -square);
        let combined = sum + square;
        additions += square - (combined - sum);
        products += error;
        sum = combined;
    }
    let mut norm = (sum - 1. + (products + additions)).sqrt();
    let square = -norm * norm;
    let error = (-norm).mul_add(norm, -square);
    let combined = sum + square;
    additions += square - (combined - sum);
    products += error;
    sum = combined;
    norm += (sum - 1. + (products + additions)) / (2. * norm);
    restore * (norm / scale)
}
