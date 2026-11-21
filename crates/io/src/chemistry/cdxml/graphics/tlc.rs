//! Native TLC geometry and the import-only boundary for unsupported spot data.
use super::super::tree::Tree;
use super::*;
use crate::{
    document::Point,
    graphics::{MAX_TLC_LANES, MAX_TLC_SPOTS, TlcLane, TlcPlate, TlcRfLabel, TlcSpot},
    typography::{Script, TextFormat, TextSpan, TextStyle},
};

const SCALE: f64 = 42. / 14.4;

#[cfg(test)]
mod tests;

struct Frame {
    origin: NativePoint,
    x: NativePoint,
    y: NativePoint,
    br: Point,
}
fn number(value: Option<&str>, default: &str) -> Result<f64> {
    let value = numeric::float(value.unwrap_or(default))?;
    value
        .is_finite()
        .then_some(value)
        .ok_or(Error::Native("Invalid TLC number"))
}
fn pair(text: &str, scale: f64) -> Result<NativePoint> {
    let values = text
        .split_whitespace()
        .map(numeric::float)
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let [x, y] = values.as_slice() else {
        return Err(Error::Native("Invalid TLC corner"));
    };
    let (x, y) = (x * scale, y * scale);
    if !x.is_finite() || !y.is_finite() || x.abs() > 1_000_000. || y.abs() > 1_000_000. {
        return Err(Error::Native("Invalid TLC corner"));
    }
    Ok(NativePoint::new(x, y))
}
impl Frame {
    fn read<'a>(attr: impl Fn(&str) -> Option<&'a str>, scale: f64) -> Result<Self> {
        let corner = |key| pair(attr(key).ok_or(Error::Native("Missing TLC corner"))?, scale);
        let (o, tr, bl, br) = (
            corner("TopLeft")?,
            corner("TopRight")?,
            corner("BottomLeft")?,
            corner("BottomRight")?,
        );
        let x = NativePoint::new(tr.x - o.x, tr.y - o.y);
        let y = NativePoint::new(bl.x - o.x, bl.y - o.y);
        let (lx, ly) = (x.x.hypot(x.y), y.x.hypot(y.y));
        let det = x.x * y.y - x.y * y.x;
        if !(0.01..=1_000_000.).contains(&lx)
            || !(0.01..=1_000_000.).contains(&ly)
            || !det.is_finite()
            || det.abs() <= lx * ly * 1e-6
        {
            return Err(Error::Native("Invalid TLC frame"));
        }
        let (dx, dy) = (br.x - o.x, br.y - o.y);
        let br = Point::new(
            ((dx * y.y - dy * y.x) / det) as f32,
            ((x.x * dy - x.y * dx) / det) as f32,
        );
        if !br.is_finite() || br.x <= 1e-6 || br.y <= 1e-6 || br.x + br.y <= 1.000001 {
            return Err(Error::Native(
                "TLC corners must form a convex quadrilateral",
            ));
        }
        let fourth = NativePoint::new(
            x.x * f64::from(br.x) + y.x * f64::from(br.y),
            x.y * f64::from(br.x) + y.y * f64::from(br.y),
        );
        if [
            (fourth.x - x.x).hypot(fourth.y - x.y),
            (fourth.x - y.x).hypot(fourth.y - y.y),
        ]
        .into_iter()
        .any(|n| !(0.01..=1_000_000.).contains(&n))
        {
            return Err(Error::Native("Invalid TLC frame"));
        }
        let frame = Self {
            origin: o,
            x,
            y,
            br,
        };
        // Validate after the same narrowing the editable document uses, so a
        // nearly singular f64 frame cannot pass import and then fail as f32.
        let mut graphic = Graphic::dragged(
            0,
            GraphicKind::Rectangle,
            Point::default(),
            Point::new(1., 1.),
            GraphicStyle::default(),
            BracketSides::Both,
            false,
        );
        graphic.kind = GraphicKind::TlcPlate;
        graphic.origin = o.into_document();
        graphic.axis_x = x.into_document();
        graphic.axis_y = y.into_document();
        graphic.tlc = Some(Box::new(TlcPlate {
            bottom_right: br,
            ..Default::default()
        }));
        graphic.validate().map_err(Error::Document)?;
        Ok(frame)
    }
    fn at(&self, u: f64, v: f64) -> NativePoint {
        let (a, b) = (
            u + (f64::from(self.br.x) - 1.) * u * v,
            v + (f64::from(self.br.y) - 1.) * u * v,
        );
        NativePoint::new(
            self.origin.x + self.x.x * a + self.y.x * b,
            self.origin.y + self.x.y * a + self.y.y * b,
        )
    }
    fn lane_at(&self, lane: usize, count: usize, v: f64, scale: f64) -> NativePoint {
        let left = self.at(0., v);
        let bl = self.at(0., 1.);
        let br = self.at(1., 1.);
        let (x, y) = (br.x - bl.x, br.y - bl.y);
        let u = TlcPlate::lane_fraction(x.hypot(y) / scale, lane, count);
        NativePoint::new(left.x + x * u, left.y + y * u)
    }
    fn offset(&self, from: NativePoint, to: NativePoint) -> Point {
        let (lx, ly) = (self.x.x.hypot(self.x.y), self.y.x.hypot(self.y.y));
        let (x, y) = (
            NativePoint::new(self.x.x / lx, self.x.y / lx),
            NativePoint::new(self.y.x / ly, self.y.y / ly),
        );
        let det = x.x * y.y - x.y * y.x;
        let (dx, dy) = (to.x - from.x, to.y - from.y);
        Point::new(
            ((dx * y.y - dy * y.x) / det) as f32,
            ((x.x * dy - x.y * dx) / det) as f32,
        )
    }
}
fn fractions<'a>(attr: impl Fn(&str) -> Option<&'a str>) -> Result<(f64, f64)> {
    let origin = number(attr("OriginFraction"), "0.1")?;
    let front = number(attr("SolventFrontFraction"), "0.1")?;
    if !(0. ..=1.).contains(&origin) || !(0. ..=1.).contains(&front) || origin + front >= 1. {
        return Err(Error::Native("Invalid TLC origin or solvent front"));
    }
    Ok((origin, front))
}

fn label_position(
    node: Node<'_, '_>,
    text: &str,
    format: &TextFormat,
    scale: f64,
    angle: f32,
) -> Result<NativePoint> {
    let layout = crate::typography::layout(text, format);
    let (w, h) = (layout.width, layout.height);
    let position = if let Some(p) = node.attribute("p") {
        let p = pair(p, scale)?.into_document();
        let baseline = layout
            .fragments
            .first()
            .map(|f| f.position.y + crate::style::text_ascent(&f.style))
            .unwrap_or_else(|| crate::style::text_ascent(&format.style));
        let factor = match format.alignment {
            crate::typography::TextAlign::Center => 0.5,
            crate::typography::TextAlign::Right => 1.,
            _ => 0.,
        };
        p.offset(w / 2. - factor * w, h / 2. - baseline)
            .turned(p, angle)
            .offset(-w / 2., -h / 2.)
    } else {
        let bounds = node
            .attribute("BoundingBox")
            .ok_or(Error::Native("Missing TLC Rf label position"))?;
        let values = bounds.split_whitespace().collect::<Vec<_>>();
        let [left, top, right, bottom] = values.as_slice() else {
            return Err(Error::Native("Invalid TLC Rf label bounds"));
        };
        let a = pair(&format!("{left} {top}"), scale)?.into_document();
        let b = pair(&format!("{right} {bottom}"), scale)?.into_document();
        if angle == 0. {
            a
        } else {
            Point::new((a.x + b.x - w) / 2., (a.y + b.y - h) / 2.)
        }
    };
    if !position.is_finite() {
        return Err(Error::Native("Invalid TLC Rf label position"));
    }
    Ok(NativePoint::new(
        f64::from(position.x),
        f64::from(position.y),
    ))
}
fn dimension(value: Option<&str>) -> Result<f64> {
    let raw = value
        .ok_or(Error::Native("Missing TLC spot dimension"))?
        .trim()
        .parse::<i32>()
        .map_err(|_| Error::Native("Invalid TLC spot dimension"))?;
    let pt = f64::from(raw) / 65536.;
    (0. < pt && pt <= 32767.)
        .then_some(pt)
        .ok_or(Error::Native("Invalid TLC spot dimension"))
}
fn flags(value: Option<&str>) -> Option<i64> {
    let value = value.unwrap_or("0");
    if let Ok(value) = value.parse::<i64>() {
        return Some(value);
    }
    value.split_whitespace().try_fold(0, |bits, part| {
        Some(
            bits | match part {
                "Closed" => 1,
                "Dashed" => 2,
                "Bold" => 4,
                "Filled" => 128,
                "Shaded" => 256,
                _ => return None,
            },
        )
    })
}

impl Reader<'_, '_> {
    fn tlc_style(
        &self,
        el: Node<'_, '_>,
        plate: Node<'_, '_>,
        filled: bool,
        dashed: bool,
        bold: bool,
    ) -> Result<NativeStyle> {
        let attr = |key| {
            el.attribute(key)
                .or_else(|| plate.attribute(key))
                .or_else(|| {
                    plate
                        .ancestors()
                        .find(|n| named(*n, "page"))
                        .and_then(|n| n.attribute(key))
                })
                .or_else(|| self.root.attribute(key))
        };
        let color = attr("color").unwrap_or("3");
        self.spend(color.len())?;
        let color = numeric::integer(color)?
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| self.colors.colors.get(n))
            .copied()
            .ok_or(Error::Native("Invalid TLC color"))?;
        let width = attr(if bold { "BoldWidth" } else { "LineWidth" }).unwrap_or(if bold {
            "2"
        } else {
            "0.6"
        });
        self.spend(width.len())?;
        let width = number(Some(width), "0.6")?;
        if !(0.1..=12.).contains(&width) {
            return Err(Error::Native("Invalid TLC line width"));
        }
        Ok(NativeStyle {
            stroke: color,
            fill: filled.then_some(color),
            width_pt: width,
            pattern: if dashed {
                LinePattern::Dashed
            } else {
                LinePattern::Solid
            },
            shaded: false,
            shadow_pt: None,
            corner_pt: None,
            opacity: None,
        })
    }
    pub(super) fn tlc(&mut self, el: Node<'_, '_>) -> Result<NativeGraphic> {
        self.spend(el.range().len())?;
        if el.attributes().any(|a| !PLATE.contains(&a.name()))
            || el
                .children()
                .filter(Node::is_element)
                .any(|n| !named(n, "tlclane"))
        {
            return Err(Error::Native("Unsupported TLC plate data"));
        }
        let frame = Frame::read(|key| el.attribute(key), self.scale)?;
        let (origin_fraction, solvent_front_fraction) = fractions(|key| el.attribute(key))?;
        let style = self.tlc_style(el, el, false, false, false)?;
        let reader = self
            .tlc_text
            .as_ref()
            .ok_or(Error::Native("Missing TLC text reader"))?;
        let lanes: Vec<_> = el.children().filter(|n| named(*n, "tlclane")).collect();
        if lanes.len() > MAX_TLC_LANES {
            return Err(Error::Limit);
        }
        let mut spots = 0usize;
        let mut text_bytes = 0usize;
        let mut result = Vec::with_capacity(lanes.len());
        for (index, lane) in lanes.iter().enumerate() {
            if lane.attributes().any(|a| a.name() != "id")
                || lane
                    .children()
                    .filter(Node::is_element)
                    .any(|n| !named(n, "tlcspot"))
            {
                return Err(Error::Native("Unsupported TLC lane data"));
            }
            let mut values = Vec::new();
            for spot in lane.children().filter(|n| named(*n, "tlcspot")) {
                if spot.attribute("Visible") == Some("no")
                    || spot.attributes().any(|a| !SPOT.contains(&a.name()))
                    || spot.children().filter(Node::is_element).any(|tag| {
                        !named(tag, "objecttag")
                            || tag.attribute("Name") != Some("rf")
                            || tag.children().filter(Node::is_element).any(|text| {
                                !named(text, "t")
                                    || text.children().filter(Node::is_element).any(|run| {
                                        !named(run, "s")
                                            || run.children().any(|node| node.is_element())
                                    })
                            })
                    })
                {
                    return Err(Error::Native("Unsupported TLC spot data"));
                }
                spots += 1;
                if spots > MAX_TLC_SPOTS {
                    return Err(Error::Limit);
                }
                let rf = number(spot.attribute("Rf"), "0")?;
                if !(0. ..=1.).contains(&rf) {
                    return Err(Error::Native("Invalid TLC Rf"));
                }
                let (width_pt, height_pt) = (
                    dimension(spot.attribute("Width"))?,
                    dimension(spot.attribute("Height"))?,
                );
                if number(spot.attribute("Tail"), "0")? != 0. {
                    return Err(Error::Native("Unsupported TLC tail"));
                }
                let bits = flags(spot.attribute("CurveType"))
                    .ok_or(Error::Native("Unsupported TLC spot style"))?;
                if bits & !(1 | 2 | 4 | 128) != 0 {
                    return Err(Error::Native("Unsupported TLC spot style"));
                }
                if !matches!(
                    spot.attribute("FillType"),
                    None | Some("None" | "Unspecified" | "Solid")
                ) || !matches!(
                    spot.attribute("LineType"),
                    None | Some("Solid" | "Dashed" | "Bold")
                ) {
                    return Err(Error::Native("Unsupported TLC spot style"));
                }
                let style = self
                    .tlc_style(
                        spot,
                        el,
                        bits & 128 != 0 || spot.attribute("FillType") == Some("Solid"),
                        bits & 2 != 0 || spot.attribute("LineType") == Some("Dashed"),
                        bits & 4 != 0 || spot.attribute("LineType") == Some("Bold"),
                    )?
                    .into_document()?;
                let show_rf = spot.attribute("ShowRf") == Some("yes");
                let center = frame.lane_at(
                    index,
                    lanes.len(),
                    1. - origin_fraction - rf * (1. - origin_fraction - solvent_front_fraction),
                    self.scale,
                );
                let tag = spot
                    .children()
                    .find(|n| named(*n, "objecttag") && n.attribute("Name") == Some("rf"));
                let label = if let Some(text) =
                    tag.and_then(|tag| tag.children().find(|n| named(*n, "t")))
                {
                    let defaults = text.ancestors().find(|n| named(*n, "page")).map(|page| {
                        page.attributes()
                            .map(|a| (a.name().into(), a.value().into()))
                            .collect()
                    });
                    let (label, format) = reader
                        .read(text, defaults.as_ref(), false)?
                        .into_document()?;
                    let angle = ((number(text.attribute("RotationAngle"), "0")? / 65536.)
                        .rem_euclid(360.) as f32)
                        .rem_euclid(360.);
                    let position = label_position(text, &label, &format, self.scale, angle)?;
                    Some(TlcRfLabel {
                        text: label,
                        format,
                        offset: frame.offset(center, position),
                        angle,
                    })
                } else if show_rf {
                    let mut format = TextFormat {
                        style: TextStyle {
                            size_pt: 7.5,
                            color: style.stroke,
                            ..Default::default()
                        },
                        ..Default::default()
                    };
                    let mut subscript = format.style.clone();
                    subscript.script = Script::Subscript;
                    format.spans.push(TextSpan {
                        start: 1,
                        end: 2,
                        style: subscript,
                    });
                    Some(TlcRfLabel {
                        text: format!("Rf = {rf:.2}"),
                        format,
                        offset: Point::new(
                            ((width_pt + f64::from(style.width_pt)) * 0.5 * self.scale) as f32,
                            (-7.5 * self.scale) as f32,
                        ),
                        angle: 0.,
                    })
                } else {
                    None
                };
                if let Some(label) = &label {
                    text_bytes = text_bytes
                        .checked_add(label.text.len())
                        .ok_or(Error::Limit)?;
                    if label.text.len() > crate::document::MAX_ANNOTATION_TEXT
                        || text_bytes > crate::document::MAX_DRAWING_TEXT
                    {
                        return Err(Error::Limit);
                    }
                }
                values.push(TlcSpot {
                    rf,
                    width_pt,
                    height_pt,
                    style,
                    show_rf,
                    label,
                });
            }
            result.push(TlcLane { spots: values });
        }
        let mut graphic = self.base(
            el,
            GraphicKind::TlcPlate,
            [frame.origin, frame.x, frame.y],
            style,
            vec![],
        )?;
        graphic.tlc = Some(Box::new(TlcPlate {
            bottom_right: frame.br,
            origin_fraction,
            solvent_front_fraction,
            // Native CDX implied booleans are false when absent. The authored
            // reference explicitly stores all three true values.
            show_origin: el.attribute("ShowOrigin") == Some("yes"),
            show_solvent_front: el.attribute("ShowSolventFront") == Some("yes"),
            show_borders: el.attribute("ShowBorders") == Some("yes"),
            lanes: result,
        }));
        Ok(graphic)
    }
}

/// Rf text is owned by a native spot, never an unattached chemical indicator.
pub(in crate::chemistry::cdxml) fn is_rf_tag(
    tree: &Tree,
    index: usize,
) -> super::super::Result<bool> {
    let mut current = Some(index);
    for expected in ["objecttag", "tlcspot", "tlclane", "tlcplate"] {
        let Some(i) = current else { return Ok(false) };
        let node = tree.node(i)?;
        if node.tag != expected || expected == "objecttag" && node.attr("Name") != Some("rf") {
            return Ok(false);
        }
        current = node.parent;
    }
    Ok(true)
}

const PLATE: &[&str] = &[
    "id",
    "BoundingBox",
    "Z",
    "Visible",
    "color",
    "LineWidth",
    "BoldWidth",
    "TopLeft",
    "TopRight",
    "BottomRight",
    "BottomLeft",
    "OriginFraction",
    "SolventFrontFraction",
    "ShowOrigin",
    "ShowSolventFront",
    "ShowBorders",
];
const SPOT: &[&str] = &[
    "id",
    "BoundingBox",
    "Z",
    "Visible",
    "color",
    "LineWidth",
    "BoldWidth",
    "Rf",
    "Tail",
    "Width",
    "Height",
    "CurveType",
    "LineType",
    "FillType",
    "ShowRf",
];
fn strip(tree: &mut Tree, index: usize, allowed: &[&str]) -> super::super::Result<bool> {
    let node = tree.node_mut(index)?;
    let before = node.attributes.len();
    node.attributes
        .retain(|(key, _)| allowed.contains(&key.as_str()));
    Ok(node.attributes.len() != before)
}

/// Keep only modelled TLC data. Returned indices are counted once by the
/// common approximation notice; malformed geometry drops this plate alone.
pub(in crate::chemistry::cdxml) fn normalize(
    tree: &mut Tree,
) -> super::super::Result<Vec<(usize, bool)>> {
    let order = tree.descendants(0)?;
    if !order
        .iter()
        .any(|i| tree.node(*i).is_ok_and(|n| n.tag == "tlcplate"))
    {
        return Ok(vec![]);
    }
    let xml = tree.serialize()?;
    let document =
        presentation::parse(&xml).map_err(|e| super::super::Error::Invalid(e.to_string()))?;
    let root = document.root_element();
    let text_reader = presentation::TextReader::new(root)
        .map_err(|e| super::super::Error::Invalid(e.to_string()))?;
    let source: HashMap<_, _> = order
        .iter()
        .copied()
        .zip(document.descendants().filter(Node::is_element))
        .collect();
    let mut notices = Vec::new();
    for index in order {
        if tree.node(index)?.tag != "tlcplate" {
            continue;
        }
        let node = tree.node(index)?;
        let frame = Frame::read(|key| node.attr(key), SCALE);
        if frame.is_err() || fractions(|key| node.attr(key)).is_err() {
            tree.detach(index)?;
            notices.push((index, true));
            continue;
        }
        // The supplied native references spell these numerical fields out.
        // Keep useful geometry for omitted values, but name the approximation
        // instead of claiming these are proven ChemDraw implicit defaults.
        let assumed =
            node.attr("OriginFraction").is_none() || node.attr("SolventFrontFraction").is_none();
        let mut changed = strip(tree, index, PLATE)? || assumed;
        let mut invalid = false;
        let mut count = 0usize;
        let mut labels = 0usize;
        let children = tree.node(index)?.children.clone();
        let lanes: Vec<_> = children
            .iter()
            .copied()
            .filter(|i| tree.node(*i).is_ok_and(|n| n.tag == "tlclane"))
            .collect();
        if lanes.len() > MAX_TLC_LANES {
            invalid = true;
        }
        for child in children {
            if tree.node(child)?.tag != "tlclane" {
                tree.detach(child)?;
                changed = true;
            }
        }
        for lane in lanes {
            if tree.node(lane)?.attr("Visible") == Some("no") {
                for spot in tree.node(lane)?.children.clone() {
                    tree.detach(spot)?;
                }
                changed = true;
            }
            changed |= strip(tree, lane, &["id"])?;
            for spot in tree.node(lane)?.children.clone() {
                if tree.node(spot)?.tag != "tlcspot" {
                    tree.detach(spot)?;
                    changed = true;
                    continue;
                }
                count += 1;
                let node = tree.node(spot)?;
                if node.attr("Visible") == Some("no") {
                    tree.detach(spot)?;
                    changed = true;
                    continue;
                }
                if !number(node.attr("Rf"), "0").is_ok_and(|rf| (0. ..=1.).contains(&rf))
                    || dimension(node.attr("Width")).is_err()
                    || dimension(node.attr("Height")).is_err()
                {
                    invalid = true;
                    break;
                }
                changed |= node.attr("Rf").is_none();
                let tail = number(node.attr("Tail"), "0");
                let bits = flags(node.attr("CurveType"));
                let fill = node.attr("FillType").map(str::to_owned);
                let line = node.attr("LineType").map(str::to_owned);
                changed |= strip(tree, spot, SPOT)?;
                if tail.is_err() || tail.is_ok_and(|n| n != 0.) {
                    tree.node_mut(spot)?.set("Tail", "0".into());
                    changed = true;
                }
                if bits.is_none_or(|n| n < 0 || n & !(1 | 2 | 4 | 128) != 0) {
                    let original = bits.unwrap_or(0).max(0);
                    let ordinary =
                        (original & (1 | 2 | 4 | 128)) | if original & 256 != 0 { 128 } else { 0 };
                    tree.node_mut(spot)?.set("CurveType", ordinary.to_string());
                    changed = true;
                }
                if !matches!(
                    fill.as_deref(),
                    None | Some("None" | "Unspecified" | "Solid")
                ) {
                    tree.node_mut(spot)?.set(
                        "FillType",
                        if fill.as_deref() == Some("Shaded") {
                            "Solid"
                        } else {
                            "None"
                        }
                        .into(),
                    );
                    changed = true;
                }
                if !matches!(line.as_deref(), None | Some("Solid" | "Dashed" | "Bold")) {
                    tree.node_mut(spot)?.set("LineType", "Solid".into());
                    changed = true;
                }
                let mut retained = false;
                for tag in tree.node(spot)?.children.clone() {
                    let node = tree.node(tag)?;
                    let hidden_rf = !retained
                        && node.tag == "objecttag"
                        && node.attr("Name") == Some("rf")
                        && (node.attr("Visible") == Some("no")
                            || node.children.iter().any(|i| {
                                tree.node(*i)
                                    .is_ok_and(|n| n.tag == "t" && n.attr("Visible") == Some("no"))
                            }));
                    if hidden_rf {
                        tree.node_mut(spot)?.set("ShowRf", "no".into());
                        tree.node_mut(tag)?.set("Visible", "yes".into());
                        for text in tree.node(tag)?.children.clone() {
                            if tree.node(text)?.tag == "t"
                                && tree.node(text)?.attr("Visible") == Some("no")
                            {
                                tree.node_mut(text)?.set("Visible", "yes".into());
                            }
                        }
                        changed = true;
                    }
                    let node = tree.node(tag)?;
                    let valid_tag = !retained
                        && node.tag == "objecttag"
                        && node.attr("Name") == Some("rf")
                        && node.attr("Visible") != Some("no");
                    let texts = node.children.clone();
                    let valid = valid_tag
                        && texts.len() == 1
                        && source
                            .get(texts.first().unwrap_or(&usize::MAX))
                            .is_some_and(|text| {
                                if !named(*text, "t") {
                                    return false;
                                }
                                let angle = match number(text.attribute("RotationAngle"), "0") {
                                    Ok(a) => {
                                        ((a / 65536.).rem_euclid(360.) as f32).rem_euclid(360.)
                                    }
                                    Err(_) => return false,
                                };
                                let defaults =
                                    text.ancestors().find(|n| named(*n, "page")).map(|page| {
                                        page.attributes()
                                            .map(|a| (a.name().into(), a.value().into()))
                                            .collect()
                                    });
                                text_reader
                                    .read(*text, defaults.as_ref(), false)
                                    .and_then(presentation::NativeText::into_document)
                                    .is_ok_and(|(label, format)| {
                                        labels = labels.saturating_add(label.len());
                                        label.len() <= crate::document::MAX_ANNOTATION_TEXT
                                            && labels <= crate::document::MAX_DRAWING_TEXT
                                            && label_position(*text, &label, &format, SCALE, angle)
                                                .is_ok()
                                    })
                            });
                    if valid {
                        retained = true;
                        if tree
                            .node(tag)?
                            .attr("TagType")
                            .is_some_and(|v| v != "Unknown")
                        {
                            tree.node_mut(tag)?.set("TagType", "Unknown".into());
                            changed = true;
                        }
                        changed |= texts
                            .first()
                            .and_then(|i| source.get(i))
                            .is_some_and(|t| t.attribute("p").is_none());
                        changed |= strip(tree, tag, &["id", "Name", "TagType", "Visible"])?;
                        // TextReader consumes only t/s content. Anything else
                        // is unsupported TLC label data, not another drawing.
                        for text in texts {
                            let allowed = &[
                                "id",
                                "p",
                                "BoundingBox",
                                "CaptionFont",
                                "CaptionSize",
                                "CaptionFace",
                                "CaptionColor",
                                "CaptionLineHeight",
                                "LineHeight",
                                "LineStarts",
                                "WordWrapWidth",
                                "Justification",
                                "CaptionJustification",
                                "InterpretChemically",
                                "Visible",
                                "RotationAngle",
                                "color",
                                "Z",
                            ];
                            changed |= strip(tree, text, allowed)?;
                            for run in tree.node(text)?.children.clone() {
                                if tree.node(run)?.tag != "s" {
                                    tree.detach(run)?;
                                    changed = true;
                                } else {
                                    changed |=
                                        strip(tree, run, &["font", "size", "face", "color"])?;
                                    for child in tree.node(run)?.children.clone() {
                                        tree.detach(child)?;
                                        changed = true;
                                    }
                                }
                            }
                        }
                    } else {
                        tree.detach(tag)?;
                        changed = true;
                    }
                }
                if tree.node(spot)?.attr("ShowRf") == Some("yes") && !retained {
                    changed = true;
                }
            }
        }
        if count > MAX_TLC_SPOTS {
            invalid = true;
        }
        if invalid {
            tree.detach(index)?;
            notices.push((index, true));
            continue;
        }
        // Invalid explicit style values must not make an otherwise useful
        // plate refuse the whole drawing after approximation.
        for item in tree.descendants(index)? {
            if !matches!(tree.node(item)?.tag.as_str(), "tlcplate" | "tlcspot") {
                continue;
            }
            let booleans: &[&str] = if tree.node(item)?.tag == "tlcplate" {
                &["ShowOrigin", "ShowSolventFront", "ShowBorders"]
            } else {
                &["ShowRf"]
            };
            for key in booleans {
                if tree
                    .node(item)?
                    .attr(key)
                    .is_some_and(|value| !matches!(value, "yes" | "no"))
                {
                    tree.node_mut(item)?.set(key, "no".into());
                    changed = true;
                }
            }
            if tree.node(item)?.attr("Z").is_some_and(|z| {
                !z.parse::<i32>()
                    .is_ok_and(|n| (-1_000_000..=1_000_000).contains(&n))
            }) {
                tree.node_mut(item)?.set("Z", "0".into());
                changed = true;
            }
            for key in ["LineWidth", "BoldWidth"] {
                if let Some(value) = tree.node(item)?.attr(key) {
                    let n = number(Some(value), "0.6").unwrap_or(0.6).clamp(0.1, 12.);
                    if !number(Some(value), "0.6").is_ok_and(|v| v == n) {
                        tree.node_mut(item)?.set(key, n.to_string());
                        changed = true;
                    }
                }
            }
            if let Some(color) = tree.node(item)?.attr("color") {
                let valid = numeric::integer(color)
                    .ok()
                    .flatten()
                    .and_then(|n| usize::try_from(n).ok())
                    .is_some_and(|n| text_reader.palette().colors.get(n).is_some());
                if !valid {
                    tree.node_mut(item)?.set("color", "3".into());
                    changed = true;
                }
            }
        }
        if changed {
            notices.push((index, false));
        }
    }
    Ok(notices)
}
