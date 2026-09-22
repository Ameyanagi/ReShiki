//! Read native-precision vector graphics and deferred pictures without editing XML.
//! Source keys are document-order element ordinals, never CDXML id attributes.
mod integer;
mod picture;
mod shapes;
use super::{numeric, presentation};
use crate::{
    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, LinePattern, PathCommand},
    scientific::Phase,
};
pub use integer::NativeLayer;
pub use picture::PictureSource;
use roxmltree::Node;
use serde::Serialize;
use std::collections::{BTreeSet, HashMap};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Numeric(#[from] super::numeric::Error),
    #[error(transparent)]
    Presentation(#[from] presentation::Error),
    #[error("{0}")]
    Native(&'static str),
    #[error("{0}")]
    Missing(String),
    #[error("CDXML graphics exceed the size or work limit")]
    Limit,
    #[error("Graphic IDs exceed the unsigned 64-bit document range")]
    IdBoundary,
    #[error("Graphic layer exceeds the signed 32-bit document range")]
    LayerBoundary,
    #[error("{0}")]
    Document(String),
    #[error("{0}")]
    Picture(String),
}
type Result<T> = std::result::Result<T, Error>;
const WORK_LIMIT: usize = 100_000_000;
const PATH_LIMIT: usize = 1_000_000;
const GRAPHIC_LIMIT: usize = 100_000;

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct NativePoint {
    pub x: f64,
    pub y: f64,
}
impl NativePoint {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    fn into_document(self) -> crate::document::Point {
        crate::document::Point::new(self.x as f32, self.y as f32)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "command", content = "points", rename_all = "snake_case")]
pub enum NativeCommand {
    Move(NativePoint),
    Cubic(NativePoint, NativePoint, NativePoint),
    Close,
}
impl NativeCommand {
    fn into_document(self) -> PathCommand {
        match self {
            Self::Move(p) => PathCommand::Move(p.into_document()),
            Self::Cubic(a, b, c) => {
                PathCommand::Cubic(a.into_document(), b.into_document(), c.into_document())
            }
            Self::Close => PathCommand::Close,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NativeStyle {
    pub stroke: presentation::NativeColor,
    pub fill: Option<presentation::NativeColor>,
    pub width_pt: f64,
    pub pattern: LinePattern,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct NativeGraphic {
    pub id: u64,
    pub kind: GraphicKind,
    pub origin: NativePoint,
    pub axis_x: NativePoint,
    pub axis_y: NativePoint,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub style: Option<NativeStyle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sides: Option<BracketSides>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase: Option<Phase>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub phase_flipped: Option<bool>,
    pub layer: NativeLayer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub path: Option<Vec<NativeCommand>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub picture_source: Option<PictureSource>,
}
impl NativeGraphic {
    /// This is the JSON->Value->Document numeric boundary: direct f64->f32.
    /// Image normalization shares the existing bounded application decoder.
    pub fn into_document(self, budget: &mut crate::pictures::exchange::Budget) -> Result<Graphic> {
        let picture = self
            .picture_source
            .map(|p| p.into_document(budget))
            .transpose()?;
        let style = if let Some(s) = self.style {
            GraphicStyle {
                stroke: s.stroke.into_document()?,
                fill: s
                    .fill
                    .map(presentation::NativeColor::into_document)
                    .transpose()?,
                width_pt: s.width_pt as f32,
                pattern: s.pattern,
            }
        } else {
            GraphicStyle::default()
        };
        let graphic = Graphic {
            id: self.id,
            kind: self.kind,
            origin: self.origin.into_document(),
            axis_x: self.axis_x.into_document(),
            axis_y: self.axis_y.into_document(),
            style,
            sides: self.sides.unwrap_or_default(),
            phase: self.phase.unwrap_or_default(),
            phase_flipped: self.phase_flipped.unwrap_or(false),
            layer: self.layer.into_document()?,
            path: self
                .path
                .unwrap_or_default()
                .into_iter()
                .map(NativeCommand::into_document)
                .collect(),
            picture,
        };
        graphic.validate().map_err(Error::Document)?;
        Ok(graphic)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct ReadGraphics {
    pub graphics: Vec<NativeGraphic>,
    /// Newly claimed element ordinal and assigned document ID, in output order.
    pub bindings: Vec<(usize, u64)>,
}
impl ReadGraphics {
    /// The caller receives all graphics only after conversion succeeds.
    pub fn into_document(self) -> Result<Vec<Graphic>> {
        let mut budget = crate::pictures::exchange::Budget::default();
        self.graphics
            .into_iter()
            .map(|g| g.into_document(&mut budget))
            .collect()
    }
}
fn named(node: Node<'_, '_>, name: &str) -> bool {
    node.is_element() && node.tag_name().namespace().is_none() && node.tag_name().name() == name
}
fn float(el: Node<'_, '_>, key: &str, default: &str) -> Result<f64> {
    Ok(numeric::float(el.attribute(key).unwrap_or(default))?)
}
fn required<'a, 'input>(el: Node<'a, 'input>, key: &str) -> Result<&'a str> {
    el.attribute(key)
        .ok_or_else(|| Error::Missing(numeric::quoted(key)))
}
struct Reader<'a, 'input> {
    root: Node<'a, 'input>,
    scale: f64,
    colors: presentation::Palette,
    middle: NativeLayer,
    work: std::cell::Cell<usize>,
    points: usize,
}
impl Reader<'_, '_> {
    fn spend(&self, count: usize) -> Result<()> {
        let work = self.work.get().checked_add(count).ok_or(Error::Limit)?;
        self.work.set(work);
        if work > WORK_LIMIT {
            return Err(Error::Limit);
        }
        Ok(())
    }
    fn layer(&mut self, el: Node<'_, '_>) -> Result<NativeLayer> {
        let text = el.attribute("Z").unwrap_or("0");
        self.spend(text.len() + self.middle.length())?;
        Ok(NativeLayer::parse(text)?.relative(&self.middle))
    }
    fn point(&mut self, text: &str) -> Result<NativePoint> {
        self.spend(text.len())?;
        let mut values = text.split_whitespace().map(numeric::float);
        let x = values.next().transpose()?;
        let y = values.next().transpose()?;
        let mut finite = x.is_some_and(f64::is_finite) && y.is_some_and(f64::is_finite);
        for value in values {
            finite &= value?.is_finite();
        }
        if !finite {
            return Err(Error::Native("Invalid graphic coordinates"));
        }
        Ok(NativePoint::new(
            x.unwrap_or(0.0) * self.scale,
            y.unwrap_or(0.0) * self.scale,
        ))
    }
    fn style(
        &self,
        el: Node<'_, '_>,
        filled: bool,
        dashed: bool,
        bold: bool,
    ) -> Result<NativeStyle> {
        // Root defaults can be reused by many graphics; charge each scalar
        // conversion rather than letting a long default induce quadratic work.
        let text = el
            .attribute("color")
            .or(self.root.attribute("color"))
            .unwrap_or("3");
        self.spend(text.len())?;
        let color = numeric::integer(text)?
            .and_then(|n| usize::try_from(n).ok())
            .and_then(|n| self.colors.colors.get(n))
            .copied()
            .ok_or(Error::Native("Invalid graphic color index"))?;
        let key = if bold { "BoldWidth" } else { "LineWidth" };
        let text = el
            .attribute(key)
            .or(self.root.attribute(key))
            .unwrap_or(if bold { "2" } else { "0.6" });
        self.spend(text.len())?;
        let width = numeric::float(text)?;
        if !width.is_finite() || !(0.0..=12.0).contains(&width) {
            return Err(Error::Native(
                "Graphic line width is outside the supported range",
            ));
        }
        let attribute = |key, default| -> Result<f64> {
            let text = el.attribute(key).unwrap_or(default);
            self.spend(text.len())?;
            Ok(numeric::float(text)?)
        };
        if attribute("FadePercent", "100")? != 100.0 || attribute("alpha", "1")? != 1.0 {
            return Err(Error::Native(
                "Transparent/faded CDXML graphics are not supported yet",
            ));
        }
        Ok(NativeStyle {
            stroke: color,
            fill: filled.then_some(color),
            width_pt: width.max(0.1),
            pattern: if dashed {
                LinePattern::Dashed
            } else {
                LinePattern::Solid
            },
        })
    }
    fn base(
        &mut self,
        el: Node<'_, '_>,
        kind: GraphicKind,
        frame: [NativePoint; 3],
        style: NativeStyle,
        path: Vec<NativeCommand>,
    ) -> Result<NativeGraphic> {
        let [origin, axis_x, axis_y] = frame;
        Ok(NativeGraphic {
            id: 0,
            kind,
            origin,
            axis_x,
            axis_y,
            style: Some(style),
            sides: Some(BracketSides::Both),
            phase: None,
            phase_flipped: None,
            layer: self.layer(el)?,
            path: Some(path),
            picture_source: None,
        })
    }
}

/// Equivalent to native local_pictures=True. Claimed elements are skipped by
/// identity, including when their IDs are duplicated or absent. No input mutates.
pub fn read(
    root: Node<'_, '_>,
    scale: f64,
    first_id: u64,
    claimed: &BTreeSet<usize>,
) -> Result<ReadGraphics> {
    // palette validates all externally supplied XML size/depth limits as well.
    let colors = presentation::palette(root)?;
    if claimed.len() > GRAPHIC_LIMIT {
        return Err(Error::Limit);
    }
    let identities: HashMap<_, _> = root
        .document()
        .descendants()
        .filter(Node::is_element)
        .enumerate()
        .map(|(i, n)| (n.id(), i))
        .collect();
    let mut middle = None;
    for el in root.descendants().filter(|n| {
        ["fragment", "n", "b", "t", "arrow"]
            .iter()
            .any(|k| named(*n, k))
    }) {
        if let Some(text) = el.attribute("Z") {
            let z = NativeLayer::parse(text)?;
            if middle.as_ref().is_none_or(|m| &z < m) {
                middle = Some(z);
            }
        }
    }
    let middle = if let Some(middle) = middle {
        middle
    } else {
        NativeLayer::parse("32767")?
    };
    let mut reader = Reader {
        root,
        scale,
        colors,
        middle,
        work: std::cell::Cell::new(root.document().input_text().len()),
        points: 0,
    };
    let mut stack: Vec<_> = root
        .children()
        .filter(|n| named(*n, "page"))
        .rev()
        .map(|n| n.children())
        .collect();
    let mut result = ReadGraphics {
        graphics: vec![],
        bindings: vec![],
    };
    while let Some(children) = stack.last_mut() {
        let Some(el) = children.next() else {
            stack.pop();
            continue;
        };
        if !el.is_element() {
            continue;
        }
        reader.spend(1)?;
        if named(el, "fragment") {
            stack.push(el.children());
            continue;
        }
        if !["graphic", "curve", "group", "embeddedobject"]
            .iter()
            .any(|key| named(el, key))
        {
            continue;
        }
        let identity = *identities.get(&el.id()).ok_or(Error::Limit)?;
        if el.attribute("SupersededBy").is_some_and(|v| !v.is_empty())
            || claimed.contains(&identity)
        {
            continue;
        }
        let mut value = if named(el, "group") {
            let children: Vec<_> = el.children().filter(Node::is_element).collect();
            if !children.is_empty()
                && children.iter().all(|n| {
                    named(*n, "curve")
                        && identities
                            .get(&n.id())
                            .is_some_and(|i| !claimed.contains(i))
                })
            {
                let mut parts = Vec::with_capacity(children.len());
                for child in children {
                    parts.push(reader.curve(child)?);
                }
                if let [a, b] = parts.as_slice()
                    && a.path == b.path
                    && a.style.as_ref().is_some_and(|s| s.fill.is_some())
                    && b.style.as_ref().is_some_and(|s| s.fill.is_none())
                {
                    let mut value = b.clone();
                    if let Some(style) = &mut value.style {
                        style.fill = a.style.as_ref().and_then(|s| s.fill);
                    }
                    value.layer = reader.layer(el)?;
                    value
                } else {
                    stack.push(el.children());
                    continue;
                }
            } else {
                stack.push(el.children());
                continue;
            }
        } else if named(el, "embeddedobject") {
            reader.picture(el)?
        } else if named(el, "curve") {
            reader.curve(el)?
        } else {
            reader.graphic(el)?
        };
        if result.graphics.len() >= GRAPHIC_LIMIT {
            return Err(Error::Limit);
        }
        value.id = first_id
            .checked_add(u64::try_from(result.graphics.len()).map_err(|_| Error::IdBoundary)?)
            .ok_or(Error::IdBoundary)?;
        result.bindings.push((identity, value.id));
        result.graphics.push(value);
    }
    Ok(result)
}

/// Convenience parser; externally supplied Nodes receive the same bounds.
pub fn parse(
    text: &str,
    scale: f64,
    first_id: u64,
    claimed: &BTreeSet<usize>,
) -> Result<ReadGraphics> {
    let document = presentation::parse(text)?;
    read(document.root_element(), scale, first_id, claimed)
}
