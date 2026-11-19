//! ChemDraw styles and objects that ReShiki cannot draw yet.
//!
//! Before the readers see an imported file, each such style is rewritten to
//! the nearest one they read and counted for one import notice. Objects with
//! no near equivalent are left out and named in the same notice. The readers
//! and their checks against the reference reader stay as they are: input they
//! refuse for another reason, such as broken structure or a limit, is still
//! refused, and so is any value this pass does not fully understand.
use super::{
    Result,
    tree::{Element, Tree},
};
use std::{
    collections::{BTreeMap, HashMap, HashSet},
    f64::consts::PI,
};

/// Orbital name part for the grey phase. Only this pass writes it.
pub(super) const GREY: &str = "Grey";

const ALLOWED: &[&str] = &[
    "CDXML",
    "page",
    "fragment",
    "n",
    "b",
    "t",
    "s",
    "fonttable",
    "font",
    "colortable",
    "color",
    "arrow",
    "graphic",
    "curve",
    "group",
    "represent",
    "objecttag",
    "embeddedobject",
    "ColoredMolecularArea",
    "chemicalproperty",
    "annotation",
    "scheme",
    "step",
    "bracketedgroup",
    "bracketattachment",
    "crossingbond",
];
/// Children of a left-out object that still import on their own.
const SALVAGED: &[&str] = &["t", "graphic", "arrow", "curve", "fragment", "group"];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    ShadedOrbital,
    DoubledCurve,
    Faded,
    BlockArrow,
    ArcStraight,
    CurvedArrowMark,
    ClosedCurveHead,
    Size,
    SingleBracket,
    StereoFlag,
    RotatedText,
    TextFace,
    TextSize,
    LabelStyle,
    Paragraph,
    PlasmidMap,
    Table,
    BioShape,
    GelPlate,
    TlcPlate,
    BrokenCurve,
    VectorPicture,
    UnknownGraphic,
    OtherObject,
}
impl Kind {
    /// Singular, plural, and how one or several were imported; `None` when
    /// they were left out.
    fn words(self) -> (&'static str, &'static str, Option<[&'static str; 2]>) {
        let same = |how| Some([how, how]);
        match self {
            Self::ShadedOrbital => ("shaded orbital", "shaded orbitals", same("in flat grey")),
            Self::DoubledCurve => (
                "doubled curve",
                "doubled curves",
                Some(["as a single line", "as single lines"]),
            ),
            Self::Faded => ("see-through object", "see-through objects", same("opaque")),
            Self::BlockArrow => (
                "block arrow",
                "block arrows",
                Some(["as a line arrow", "as line arrows"]),
            ),
            Self::ArcStraight => ("arc arrow", "arc arrows", same("straight")),
            Self::CurvedArrowMark => (
                "curved arrow",
                "curved arrows",
                same("without the double shaft, no-go or dipole mark"),
            ),
            Self::ClosedCurveHead => (
                "closed or filled curve",
                "closed or filled curves",
                same("without arrowheads"),
            ),
            Self::Size => (
                "line, arrowhead or shadow size",
                "line, arrowhead or shadow sizes",
                same("at the nearest size ReShiki draws"),
            ),
            Self::SingleBracket => (
                "single bracket",
                "single brackets",
                Some(["as a curve", "as curves"]),
            ),
            Self::StereoFlag => ("stereo flag", "stereo flags", same("as text")),
            Self::RotatedText => ("rotated caption", "rotated captions", same("upright")),
            Self::TextFace => (
                "outlined or shadowed text",
                "outlined or shadowed texts",
                same("in the plain face"),
            ),
            Self::TextSize => (
                "text size",
                "text sizes",
                same("at the nearest size ReShiki draws"),
            ),
            Self::LabelStyle => (
                "atom label with mixed text styles",
                "atom labels with mixed text styles",
                Some([
                    "in the style of its first character",
                    "in the style of their first character",
                ]),
            ),
            Self::Paragraph => (
                "caption",
                "captions",
                same("with automatic line spacing or width"),
            ),
            Self::BioShape => ("BioDraw shape", "BioDraw shapes", None),
            Self::PlasmidMap => (
                "plasmid map",
                "plasmid maps",
                same("as its ring, regions and labels"),
            ),
            Self::Table => ("table", "tables", same("as its cell contents")),
            Self::GelPlate => ("gel plate", "gel plates", None),
            Self::TlcPlate => ("TLC plate", "TLC plates", None),
            Self::BrokenCurve => (
                "curve with an incomplete point list",
                "curves with an incomplete point list",
                None,
            ),
            Self::VectorPicture => (
                "picture without a bitmap copy",
                "pictures without a bitmap copy",
                None,
            ),
            Self::UnknownGraphic => (
                "incomplete or unknown graphic",
                "incomplete or unknown graphics",
                None,
            ),
            Self::OtherObject => ("other ChemDraw object", "other ChemDraw objects", None),
        }
    }
}

/// The file as the readers accept it, and what changed on the way.
///
/// The text is returned unchanged, without a notice, when nothing needs
/// rewriting or when this pass cannot read the file at all; the readers then
/// report their own error.
pub fn approximate(xml: &str) -> (String, Option<String>) {
    match rewrite(xml) {
        Ok((text, notice, _)) => (text, notice),
        Err(_) => (xml.into(), None),
    }
}

/// How many objects of the file [`approximate`] draws differently or leaves
/// out. A save back to ChemDraw's format loses their original form.
pub(crate) fn approximated(xml: &str) -> usize {
    rewrite(xml).map_or(0, |(_, _, count)| count)
}

fn rewrite(xml: &str) -> Result<(String, Option<String>, usize)> {
    let mut pass = Pass {
        tree: Tree::parse_import(xml)?,
        log: BTreeMap::new(),
        changed: false,
        original: HashSet::new(),
        headless: HashSet::new(),
        resized: HashSet::new(),
    };
    for index in pass.tree.descendants(0)? {
        if let Some(id) = pass.node(index)?.attr("id") {
            pass.original.insert(id.to_owned());
        }
    }
    pass.objects(0)?;
    pass.brackets()?;
    for index in pass.tree.descendants(0)? {
        pass.element(index)?;
    }
    if !pass.changed {
        return Ok((xml.into(), None, 0));
    }
    pass.references()?;
    let count = pass.log.values().sum();
    Ok((pass.tree.serialize()?, pass.notice(), count))
}

struct Pass {
    tree: Tree,
    log: BTreeMap<Kind, usize>,
    changed: bool,
    /// Identifiers the file had before anything was rewritten.
    original: HashSet<String>,
    /// Arrows that became lines without a head.
    headless: HashSet<String>,
    /// Objects already counted for a size out of range.
    resized: HashSet<usize>,
}

fn numbers(text: &str) -> Option<Vec<f64>> {
    text.split_whitespace()
        .map(|part| part.parse::<f64>().ok().filter(|value| value.is_finite()))
        .collect()
}
/// The x and y of a point; a 3D point's depth is not drawn.
fn point(text: &str) -> Option<[f64; 2]> {
    match numbers(text)?.as_slice() {
        [x, y, ..] => Some([*x, *y]),
        _ => None,
    }
}
fn number(text: &str) -> Option<f64> {
    text.trim().parse::<f64>().ok().filter(|n| n.is_finite())
}
fn spell(value: f64) -> String {
    let text = format!("{value:.4}");
    text.trim_end_matches('0').trim_end_matches('.').into()
}
fn points(values: &[[f64; 2]]) -> String {
    values
        .iter()
        .map(|[x, y]| format!("{} {}", spell(*x), spell(*y)))
        .collect::<Vec<_>>()
        .join(" ")
}
/// ChemDraw's point list for a path of Bézier segments: every anchor between
/// its incoming and outgoing control points.
fn curve_points(start: [f64; 2], segments: &[[[f64; 2]; 3]]) -> String {
    let mut list = vec![start, start];
    for [a, b, end] in segments {
        list.extend([*a, *b, *end]);
    }
    list.push(segments.last().map_or(start, |segment| segment[2]));
    points(&list)
}
/// Whether three anchors make the two straight legs of a bent arrow, as the
/// arrow reader judges it after scaling. Stricter here: a curve this takes
/// for a path only loses a mark.
fn elbow(values: &[f64]) -> bool {
    let &[
        _,
        _,
        sx,
        sy,
        ax,
        ay,
        bx,
        by,
        cx,
        cy,
        dx,
        dy,
        ex,
        ey,
        fx,
        fy,
        _,
        _,
    ] = values
    else {
        return false;
    };
    let on = |p: [f64; 2], from: [f64; 2], to: [f64; 2]| {
        let (x, y) = (to[0] - from[0], to[1] - from[1]);
        let length = x.hypot(y);
        if length < 1e-6 {
            return (p[0] - from[0]).hypot(p[1] - from[1]) < 0.02;
        }
        let cross = ((p[0] - from[0]) * y - (p[1] - from[1]) * x).abs() / length;
        let along = ((p[0] - from[0]) * x + (p[1] - from[1]) * y) / (length * length);
        cross < 0.02 && (0.0..=1.).contains(&along)
    };
    let (start, corner, end) = ([sx, sy], [cx, cy], [fx, fy]);
    on([ax, ay], start, corner)
        && on([bx, by], start, corner)
        && on([dx, dy], corner, end)
        && on([ex, ey], corner, end)
}
/// An elliptical arc from angle `start` through `sweep` radians, in segments
/// of at most a quarter turn.
fn arc(
    center: [f64; 2],
    u: [f64; 2],
    v: [f64; 2],
    start: f64,
    sweep: f64,
) -> ([f64; 2], Vec<[[f64; 2]; 3]>) {
    let count = (sweep.abs() / (PI / 2.)).ceil().clamp(1., 16.);
    let step = sweep / count;
    let k = 4. / 3. * (step / 4.).tan();
    let at = |angle: f64| {
        let (sin, cos) = angle.sin_cos();
        (
            [
                center[0] + u[0] * cos + v[0] * sin,
                center[1] + u[1] * cos + v[1] * sin,
            ],
            [-u[0] * sin + v[0] * cos, -u[1] * sin + v[1] * cos],
        )
    };
    let (first, _) = at(start);
    let segments = (0..count as usize)
        .map(|i| {
            let (p0, t0) = at(start + step * i as f64);
            let (p1, t1) = at(start + step * (i + 1) as f64);
            [
                [p0[0] + k * t0[0], p0[1] + k * t0[1]],
                [p1[0] - k * t1[0], p1[1] - k * t1[1]],
                p1,
            ]
        })
        .collect();
    (first, segments)
}

impl Pass {
    fn node(&self, index: usize) -> Result<&Element> {
        self.tree.node(index)
    }
    fn attr(&self, index: usize, key: &str) -> Result<Option<String>> {
        Ok(self.node(index)?.attr(key).map(str::to_owned))
    }
    fn set(&mut self, index: usize, key: &str, value: impl Into<String>) -> Result<()> {
        self.changed = true;
        self.tree.node_mut(index)?.set(key, value.into());
        Ok(())
    }
    fn remove(&mut self, index: usize, keys: &[&str]) -> Result<bool> {
        let node = self.tree.node_mut(index)?;
        let before = node.attributes.len();
        node.attributes
            .retain(|(key, _)| !keys.contains(&key.as_str()));
        let removed = node.attributes.len() != before;
        self.changed |= removed;
        Ok(removed)
    }
    fn count(&mut self, kind: Kind) {
        *self.log.entry(kind).or_default() += 1;
    }
    fn drop_element(&mut self, index: usize) -> Result<()> {
        self.changed = true;
        self.tree.detach(index)
    }
    fn attached(&self, index: usize) -> Result<bool> {
        Ok(index == 0 || self.node(index)?.parent.is_some())
    }

    fn notice(&self) -> Option<String> {
        let phrase = |kind: Kind, count: usize| {
            let (one, many, how) = kind.words();
            let name = if count == 1 { one } else { many };
            match how {
                Some([how, _]) if count == 1 => format!("{count} {name} {how}"),
                Some([_, how]) => format!("{count} {name} {how}"),
                None => format!("{count} {name}"),
            }
        };
        let list = |kept: bool| {
            self.log
                .iter()
                .filter(|(kind, _)| kind.words().2.is_some() == kept)
                .map(|(kind, count)| phrase(*kind, *count))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let (kept, left_out) = (list(true), list(false));
        // The list comes first: the status bar shows the start of a notice.
        let mut notice = String::new();
        if !kept.is_empty() {
            notice = format!("Nearest style used: {kept}.");
        }
        if !left_out.is_empty() {
            if !notice.is_empty() {
                notice.push(' ');
            }
            notice.push_str(&format!("Left out: {left_out}."));
        }
        if notice.is_empty() {
            return None;
        }
        notice.push_str(
            " ReShiki cannot draw these yet. Keep the original ChemDraw file if you need them.",
        );
        Some(notice)
    }

    /// Replace every object the readers do not know by the parts of it that
    /// import on their own.
    fn objects(&mut self, index: usize) -> Result<()> {
        let children = self.node(index)?.children.clone();
        let root = self.node(index)?.tag == "CDXML";
        // The new child list is built once, however many objects go.
        let mut kept = Vec::with_capacity(children.len());
        let mut changed = false;
        for child in children {
            let node = self.node(child)?;
            let tag = node.tag.clone();
            // A hidden tag shows nothing, and brackets around nothing are
            // plain brackets.
            if tag == "objecttag" && node.attr("Visible") == Some("no")
                || tag == "bracketedgroup" && node.attr("BracketedObjectIDs").is_none()
            {
                changed = true;
                self.tree.node_mut(child)?.parent = None;
                continue;
            }
            // Reaction schemes are checked as the file has them.
            if matches!(tag.as_str(), "scheme" | "step") {
                kept.push(child);
                continue;
            }
            if ALLOWED.contains(&tag.as_str()) {
                kept.push(child);
                self.objects(child)?;
                continue;
            }
            changed = true;
            match tag.as_str() {
                // A template file's grid has nothing to draw.
                "templategrid" => {}
                "bioshape" => self.count(Kind::BioShape),
                "plasmidmap" => self.count(Kind::PlasmidMap),
                "table" => self.count(Kind::Table),
                "gepplate" => self.count(Kind::GelPlate),
                "tlcplate" => self.count(Kind::TlcPlate),
                _ => self.count(Kind::OtherObject),
            }
            self.tree.node_mut(child)?.parent = None;
            if root {
                continue;
            }
            for part in self.salvage(child, 0)? {
                self.tree.node_mut(part)?.parent = Some(index);
                kept.push(part);
                self.objects(part)?;
            }
        }
        if changed {
            self.changed = true;
            self.tree.node_mut(index)?.children = kept;
        }
        Ok(())
    }
    fn salvage(&mut self, index: usize, depth: usize) -> Result<Vec<usize>> {
        let mut kept = Vec::new();
        if depth > 64 {
            return Ok(kept);
        }
        if self.node(index)?.tag == "plasmidmap"
            && let Some(ring) = self.plasmid_ring(index)?
        {
            kept.push(ring);
        }
        for child in self.node(index)?.children.clone() {
            let tag = self.node(child)?.tag.clone();
            if tag == "plasmidregion" {
                // A region is stored as an arc arrow around the map's centre;
                // its shaft spacing is the width of the band.
                let region = self.tree.node_mut(child)?;
                region.tag = "arrow".into();
                region
                    .attributes
                    .retain(|(key, _)| key != "ArrowShaftSpacing");
                kept.push(child);
            } else if SALVAGED.contains(&tag.as_str()) {
                kept.push(child);
            } else if tag == "page" || !ALLOWED.contains(&tag.as_str()) {
                kept.extend(self.salvage(child, depth + 1)?);
            }
        }
        Ok(kept)
    }
    fn plasmid_ring(&mut self, map: usize) -> Result<Option<usize>> {
        let node = self.node(map)?;
        let (Some(center), Some(radius)) = (
            node.attr("p").and_then(numbers),
            node.attr("RingRadius").and_then(number),
        ) else {
            return Ok(None);
        };
        let &[x, y, ..] = center.as_slice() else {
            return Ok(None);
        };
        // The radius is stored in ChemDraw's fixed-point units.
        let radius = radius / 65536.;
        let mut attributes = vec![
            ("GraphicType".to_owned(), "Oval".to_owned()),
            ("OvalType".into(), "Circle".into()),
            (
                "BoundingBox".into(),
                format!(
                    "{} {} {} {}",
                    spell(x + radius),
                    spell(y),
                    spell(x),
                    spell(y)
                ),
            ),
            ("Center3D".into(), format!("{} {} 0", spell(x), spell(y))),
            (
                "MajorAxisEnd3D".into(),
                format!("{} {} 0", spell(x + radius), spell(y)),
            ),
            (
                "MinorAxisEnd3D".into(),
                format!("{} {} 0", spell(x), spell(y + radius)),
            ),
        ];
        for key in ["Z", "color", "LineWidth"] {
            if let Some(value) = node.attr(key) {
                attributes.push((key.into(), value.into()));
            }
        }
        let ring = self.tree.create("graphic")?;
        self.tree.node_mut(ring)?.attributes = attributes;
        Ok(Some(ring))
    }

    fn element(&mut self, index: usize) -> Result<()> {
        if !self.attached(index)? {
            return Ok(());
        }
        self.text(index)?;
        // Hidden artwork is not drawn, and still marks out its page.
        if self.node(index)?.attr("Visible") == Some("no") {
            return Ok(());
        }
        match self.node(index)?.tag.as_str() {
            "arrow" => self.arrow(index),
            "curve" => self.curve(index),
            // The picture reader takes a bitmap; a vector-only or linked
            // picture has none.
            "embeddedobject"
                if !["PNG", "TIFF", "JPEG", "GIF", "BMP"].iter().any(|key| {
                    self.node(index)
                        .is_ok_and(|node| node.attr(key).is_some_and(|data| !data.is_empty()))
                }) =>
            {
                self.drop_element(index)?;
                self.count(Kind::VectorPicture);
                Ok(())
            }
            "graphic"
                if self
                    .node(index)?
                    .attr("SupersededBy")
                    .is_none_or(str::is_empty) =>
            {
                self.graphic(index)
            }
            _ => Ok(()),
        }
    }

    /// ChemDraw's FadePercent only spreads a shaded fill, which ReShiki
    /// draws its own way; alpha makes the object see-through.
    fn fade(&mut self, index: usize) -> Result<()> {
        if self
            .attr(index, "FadePercent")?
            .as_deref()
            .and_then(number)
            .is_some()
        {
            self.remove(index, &["FadePercent"])?;
        }
        if let Some(alpha) = self.attr(index, "alpha")?.as_deref().and_then(number) {
            self.remove(index, &["alpha"])?;
            if alpha < 1. {
                self.count(Kind::Faded);
            }
        }
        Ok(())
    }
    fn clamp(&mut self, index: usize, key: &str, low: f64, high: f64) -> Result<()> {
        if let Some(value) = self.attr(index, key)?.as_deref().and_then(number)
            && !(low..=high).contains(&value)
        {
            self.set(index, key, spell(value.clamp(low, high)))?;
            if self.resized.insert(index) {
                self.count(Kind::Size);
            }
        }
        Ok(())
    }
    fn head_sizes(&mut self, index: usize) -> Result<()> {
        self.clamp(index, "HeadSize", 50., 2400.)?;
        self.clamp(index, "ArrowheadWidth", 25., 1600.)?;
        let length = self
            .attr(index, "HeadSize")?
            .as_deref()
            .and_then(number)
            .unwrap_or(1000.);
        // The head's notch may take nine tenths of its length. The reader
        // decides that from the quotient, so stay clear of its rounding.
        if let Some(center) = self
            .attr(index, "ArrowheadCenterSize")?
            .as_deref()
            .and_then(number)
            && length > 0.
            && !(0.0..=0.9).contains(&(1. - center / length))
        {
            let nearest = if center > length {
                length
            } else {
                length * 0.1001
            };
            self.set(index, "ArrowheadCenterSize", spell(nearest))?;
            if self.resized.insert(index) {
                self.count(Kind::Size);
            }
        }
        Ok(())
    }
    /// A curve takes one stroke flag; several, as ChemDraw joins them in one
    /// value, go into the curve's own flags. ChemDraw draws a wavy curve
    /// straight, so that flag has nothing to keep.
    fn stroke_flags(&mut self, index: usize) -> Result<()> {
        let Some(value) = self.attr(index, "LineType")? else {
            return Ok(());
        };
        let flags: Vec<_> = value.split_whitespace().collect();
        if flags.len() <= 1
            || flags
                .iter()
                .any(|flag| !matches!(*flag, "Solid" | "Dashed" | "Bold" | "Wavy"))
        {
            return Ok(());
        }
        let Some(bits) = self.curve_type(index)? else {
            return Ok(());
        };
        self.remove(index, &["LineType"])?;
        let dashed = if flags.contains(&"Dashed") { 2 } else { 0 };
        let bold = if flags.contains(&"Bold") { 4 } else { 0 };
        self.set(index, "CurveType", (bits | dashed | bold).to_string())
    }
    fn curve_type(&self, index: usize) -> Result<Option<i64>> {
        Ok(match self.node(index)?.attr("CurveType") {
            None => Some(0),
            Some(text) => text.trim().parse::<i64>().ok().filter(|bits| *bits >= 0),
        })
    }

    fn arrow(&mut self, index: usize) -> Result<()> {
        self.fade(index)?;
        if self
            .attr(index, "FillType")?
            .is_some_and(|fill| matches!(fill.as_str(), "Solid" | "Shaded"))
        {
            self.set(index, "FillType", "None")?;
            self.count(Kind::BlockArrow);
        }
        self.head_sizes(index)?;
        self.clamp(index, "LineWidth", 0.1, 12.)?;
        self.clamp(index, "BoldWidth", 0.1, 12.)?;
        if self
            .attr(index, "ArrowShaftSpacing")?
            .as_deref()
            .and_then(number)
            .is_some_and(|gap| gap != 0.)
        {
            self.clamp(index, "ArrowShaftSpacing", 50., 1600.)?;
        }
        if self
            .attr(index, "AngularSize")?
            .as_deref()
            .and_then(number)
            .is_some_and(|angle| angle != 0.)
        {
            return self.arc_arrow(index);
        }
        let node = self.node(index)?;
        if node.attr("Head3D").is_none() || node.attr("Tail3D").is_none() {
            // Nothing says where the arrow runs.
            self.drop_element(index)?;
            self.count(Kind::UnknownGraphic);
        }
        Ok(())
    }
    /// ChemDraw stores its Arc tool and its arc arrows as an arrow around a
    /// centre. The same curve is a Bézier path, which the readers know.
    fn arc_arrow(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        let pair = |key: &str| node.attr(key).and_then(point);
        let geometry = (|| {
            let angle = node.attr("AngularSize").and_then(number)?;
            let (center, tail, head) = (pair("Center3D")?, pair("Tail3D")?, pair("Head3D")?);
            let (major, minor) = (pair("MajorAxisEnd3D")?, pair("MinorAxisEnd3D")?);
            let u = [major[0] - center[0], major[1] - center[1]];
            let v = [minor[0] - center[0], minor[1] - center[1]];
            let determinant = u[0] * v[1] - u[1] * v[0];
            let w = [tail[0] - center[0], tail[1] - center[1]];
            let a = (w[0] * v[1] - w[1] * v[0]) / determinant;
            let b = (u[0] * w[1] - u[1] * w[0]) / determinant;
            let scale = a.hypot(b);
            if !scale.is_finite() || scale == 0. || angle.abs() > 360. {
                return None;
            }
            let (u, v) = ([u[0] * scale, u[1] * scale], [v[0] * scale, v[1] * scale]);
            let start = b.atan2(a);
            // The sweep runs against the angle in a right-handed frame. When
            // both directions are possible, the stored head decides.
            let end = |sweep: f64| {
                let (first, segments) = arc(center, u, v, start, sweep);
                let last = segments.last().map_or(first, |segment| segment[2]);
                (last[0] - head[0]).hypot(last[1] - head[1])
            };
            let sweep = -angle.to_radians() * determinant.signum();
            let sweep = if end(-sweep) + 1e-6 < end(sweep) {
                -sweep
            } else {
                sweep
            };
            Some(arc(center, u, v, start, sweep))
        })();
        let Some((first, segments)) = geometry else {
            self.set(index, "AngularSize", "0")?;
            self.count(Kind::ArcStraight);
            return Ok(());
        };
        let node = self.tree.node_mut(index)?;
        node.tag = "curve".into();
        node.set("CurvePoints", curve_points(first, &segments));
        if !["ArrowheadHead", "ArrowheadTail"]
            .iter()
            .any(|key| matches!(node.attr(key), Some("Full" | "HalfLeft" | "HalfRight")))
            && let Some(id) = node.attr("id")
        {
            self.headless.insert(id.to_owned());
        }
        self.remove(
            index,
            &[
                "AngularSize",
                "Center3D",
                "MajorAxisEnd3D",
                "MinorAxisEnd3D",
                "Head3D",
                "Tail3D",
                "FillType",
                "BoundingBox",
            ],
        )?;
        self.changed = true;
        self.curve(index)
    }

    fn curve(&mut self, index: usize) -> Result<()> {
        self.stroke_flags(index)?;
        self.fade(index)?;
        self.clamp(index, "LineWidth", 0., 12.)?;
        self.clamp(index, "BoldWidth", 0., 12.)?;
        let Some(mut bits) = self.curve_type(index)? else {
            return Ok(());
        };
        let original = bits;
        // Older files keep the arrowheads in the flags.
        for (bit, key, head) in [
            (8, "ArrowheadHead", "Full"),
            (16, "ArrowheadTail", "Full"),
            (32, "ArrowheadHead", "HalfLeft"),
            (64, "ArrowheadTail", "HalfLeft"),
        ] {
            if bits & bit != 0 && self.attr(index, key)?.is_none() {
                self.set(index, key, head)?;
            }
        }
        if bits & 512 != 0 {
            self.count(Kind::DoubledCurve);
        }
        bits &= 1 | 2 | 4 | 128 | 256;
        if bits != original {
            self.set(index, "CurveType", bits.to_string())?;
        }
        if !self.curve_geometry(index)? {
            return Ok(());
        }
        let node = self.node(index)?;
        let heads = ["ArrowheadHead", "ArrowheadTail"]
            .iter()
            .any(|key| !matches!(node.attr(key).unwrap_or("None"), "None" | "Unspecified"));
        let closed = bits & 1 != 0 || node.attr("Closed") == Some("yes");
        // A shaded curve is closed and filled as well.
        let filled =
            bits & (128 | 256) != 0 || matches!(node.attr("FillType"), Some("Solid" | "Shaded"));
        if heads && (closed || filled) {
            self.remove(index, &["ArrowheadHead", "ArrowheadTail"])?;
            self.count(Kind::ClosedCurveHead);
            if let Some(id) = self.attr(index, "id")? {
                self.headless.insert(id);
            }
        } else if heads {
            // A curve of one segment, or of the two legs of a bent arrow,
            // with solid heads is an arrow, which keeps a no-go or dipole
            // mark; a longer one is a path, which has neither. No curve has
            // a double shaft.
            let node = self.node(index)?;
            let arrow = matches!(node.attr("ArrowheadType"), None | Some("Solid"))
                && node
                    .attr("CurvePoints")
                    .and_then(numbers)
                    .is_some_and(|values| values.len() == 12 || elbow(&values));
            let mut marks = vec!["ArrowShaftSpacing", "AngularSize"];
            if !arrow {
                marks.extend(["Dipole", "NoGo"]);
            }
            let marked = marks.iter().any(|key| match *key {
                "Dipole" => node.attr(key) == Some("yes"),
                "NoGo" => node
                    .attr(key)
                    .is_some_and(|mark| !mark.eq_ignore_ascii_case("none")),
                _ => node.attr(key).and_then(number).is_some_and(|n| n != 0.),
            });
            if marked {
                self.remove(index, &marks)?;
                self.count(Kind::CurvedArrowMark);
            }
        }
        self.head_sizes(index)
    }
    /// Whether the curve still stands after its point list was made readable.
    fn curve_geometry(&mut self, index: usize) -> Result<bool> {
        let node = self.node(index)?;
        let Some(text) = node.attr("CurvePoints") else {
            // Some files keep only the 3D list; without either there is
            // nothing to draw.
            let flat = node
                .attr("CurvePoints3D")
                .and_then(numbers)
                .filter(|values| values.len() % 3 == 0 && values.len() >= 18)
                .map(|values| {
                    values
                        .as_chunks::<3>()
                        .0
                        .iter()
                        .map(|[x, y, _]| [*x, *y])
                        .collect::<Vec<_>>()
                });
            return match flat {
                Some(flat) => {
                    self.set(index, "CurvePoints", points(&flat))?;
                    self.curve_geometry(index)
                }
                None => {
                    self.drop_element(index)?;
                    Ok(false)
                }
            };
        };
        let count = text.split_whitespace().count();
        if count >= 12 && count.is_multiple_of(6) {
            return Ok(true);
        }
        // Anchor, two controls, anchor, …: add the end controls ChemDraw keeps.
        if count >= 8
            && count % 6 == 2
            && let Some(values) = numbers(text)
        {
            let mut list: Vec<_> = values.as_chunks::<2>().0.to_vec();
            if let (Some(&first), Some(&last)) = (list.first(), list.last()) {
                list.insert(0, first);
                list.push(last);
            }
            self.set(index, "CurvePoints", points(&list))?;
            return Ok(true);
        }
        self.drop_element(index)?;
        // A single point or an empty list draws nothing.
        if count > 6 {
            self.count(Kind::BrokenCurve);
        }
        Ok(false)
    }

    fn graphic(&mut self, index: usize) -> Result<()> {
        self.fade(index)?;
        self.clamp(index, "LineWidth", 0., 12.)?;
        self.clamp(index, "BoldWidth", 0., 12.)?;
        let node = self.node(index)?;
        let arrow = node.attr("ArrowType").is_some_and(|kind| kind != "NoHead");
        match node.attr("GraphicType") {
            Some("Line") if arrow => self.legacy_arrow(index),
            Some("Line") => Ok(()),
            Some("Oval")
                if !["Center3D", "MajorAxisEnd3D", "MinorAxisEnd3D"]
                    .iter()
                    .all(|key| node.attr(key).is_some()) =>
            {
                self.oval_axes(index)
            }
            // A shadow of 0.1 to 36 pt, in hundredths.
            Some("Rectangle" | "Oval") => self.clamp(index, "ShadowSize", 10., 3600.),
            Some("Orbital") => self.orbital(index),
            Some("Symbol") => self.symbol(index),
            Some("Arc") => self.arc_graphic(index),
            Some("Bracket") => Ok(()),
            _ => {
                self.drop_element(index)?;
                self.count(Kind::UnknownGraphic);
                Ok(())
            }
        }
    }
    /// A circle stored with its centre and one radius has the other radius
    /// at a right angle. Without them nothing says where the oval is.
    fn oval_axes(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        let end = |key: &str| node.attr(key).and_then(point);
        let circle = node
            .attr("OvalType")
            .is_some_and(|flags| flags.split_whitespace().any(|flag| flag == "Circle"));
        match (end("Center3D"), end("MajorAxisEnd3D")) {
            (Some(center), Some(major)) if circle && node.attr("MinorAxisEnd3D").is_none() => {
                let (dx, dy) = (major[0] - center[0], major[1] - center[1]);
                self.set(
                    index,
                    "MinorAxisEnd3D",
                    format!("{} {} 0", spell(center[0] - dy), spell(center[1] + dx)),
                )?;
                self.graphic(index)
            }
            _ => {
                self.drop_element(index)?;
                self.count(Kind::UnknownGraphic);
                Ok(())
            }
        }
    }
    /// ChemDraw's orbital names in the spelling the reader knows: the shape,
    /// its phase, then `2` for the mirrored one.
    fn orbital(&mut self, index: usize) -> Result<()> {
        let Some(name) = self.attr(index, "OrbitalType")? else {
            return Ok(());
        };
        let (base, phase) = if let Some(base) = name.strip_suffix("Filled") {
            (base, "Solid")
        } else if let Some(base) = name.strip_suffix("Shaded") {
            (base, GREY)
        } else {
            (name.as_str(), "")
        };
        let (shape, mirrored) = match base {
            "s" | "oval" | "lobe" | "p" | "dxy" => (base, false),
            // ChemDraw's plus fills the small lobe or the ring, where
            // ReShiki's mirrored phase is.
            "hybridPlus" => ("sp3", true),
            "hybridMinus" => ("sp3", false),
            "dz2Plus" => ("dz2", true),
            "dz2Minus" => ("dz2", false),
            _ => return Ok(()),
        };
        // Plain names the reader already knows keep their present reading.
        if phase.is_empty() && shape != "sp3" {
            return Ok(());
        }
        if phase == GREY {
            self.count(Kind::ShadedOrbital);
        }
        self.set(
            index,
            "OrbitalType",
            format!("{shape}{phase}{}", if mirrored { "2" } else { "" }),
        )
    }
    fn symbol(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        let marks_atom = node
            .children
            .iter()
            .any(|&child| self.node(child).is_ok_and(|child| child.tag == "represent"));
        // Earlier ReShiki versions wrote an atom's lone pair with the value
        // ChemDraw uses for the bar, which never marks an atom.
        if marks_atom && node.attr("SymbolType") == Some("LonePairBar") {
            return self.set(index, "SymbolType", "LonePair");
        }
        let label = match node.attr("SymbolType") {
            Some("Racemic") => "Rac",
            Some("Absolute") => "Abs",
            Some("Relative") => "Rel",
            Some(
                "CirclePlus" | "CircleMinus" | "Plus" | "Minus" | "Radical" | "Electron"
                | "LonePair" | "ElectronPair" | "RadicalCation" | "RadicalAnion" | "HDot" | "HDash"
                | "Attachment" | "Dagger" | "DoubleDagger" | "LonePairBar",
            ) => return Ok(()),
            // A symbol that marks an atom is the mark reader's to judge.
            _ if marks_atom => return Ok(()),
            _ => {
                self.drop_element(index)?;
                self.count(Kind::UnknownGraphic);
                return Ok(());
            }
        };
        let Some(&[x1, y1, x2, y2]) = node.attr("BoundingBox").and_then(numbers).as_deref() else {
            return Ok(());
        };
        let color = node.attr("color").map(str::to_owned);
        for child in node.children.clone() {
            self.tree.detach(child)?;
        }
        let run = self.tree.create("s")?;
        let text = self.tree.node_mut(run)?;
        text.text = label.into();
        if let Some(color) = color {
            text.set("color", color);
        }
        self.tree.append(index, run)?;
        let node = self.tree.node_mut(index)?;
        node.tag = "t".into();
        node.attributes
            .retain(|(key, _)| matches!(key.as_str(), "id" | "Z" | "Visible"));
        node.set("p", format!("{} {}", spell(x1.min(x2)), spell(y1.max(y2))));
        self.changed = true;
        self.count(Kind::StereoFlag);
        Ok(())
    }
    /// An arrow from before ChemDraw stored arrows as their own object, with
    /// the settings ChemDraw itself gives it on opening.
    fn legacy_arrow(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        let Some(&[hx, hy, tx, ty]) = node.attr("BoundingBox").and_then(numbers).as_deref() else {
            return Ok(());
        };
        let (head, tail, shape, gap): (&str, Option<&str>, &str, Option<&str>) =
            match node.attr("ArrowType") {
                Some("FullHead") => ("Full", None, "Solid", None),
                Some("HalfHead") => ("HalfLeft", None, "Solid", None),
                Some("Resonance") => ("Full", Some("Full"), "Solid", None),
                Some("Equilibrium") => ("HalfLeft", Some("HalfLeft"), "Solid", Some("400")),
                Some("RetroSynthetic") => ("Full", None, "Angle", Some("600")),
                Some("Hollow") => ("Full", None, "Hollow", None),
                Some("NoGo" | "Dipole") => ("Full", None, "Solid", None),
                _ => return Ok(()),
            };
        let kind = node.attr("ArrowType").unwrap_or_default().to_owned();
        let length = node.attr("HeadSize").and_then(number).unwrap_or(1000.);
        let node = self.tree.node_mut(index)?;
        node.tag = "arrow".into();
        node.attributes.retain(|(key, _)| {
            !matches!(key.as_str(), "GraphicType" | "ArrowType" | "BoundingBox")
        });
        node.set("Head3D", format!("{} {} 0", spell(hx), spell(hy)));
        node.set("Tail3D", format!("{} {} 0", spell(tx), spell(ty)));
        node.set("ArrowheadHead", head.into());
        if let Some(tail) = tail {
            node.set("ArrowheadTail", tail.into());
        }
        node.set("ArrowheadType", shape.into());
        node.set("ArrowheadCenterSize", spell(length * 0.875));
        node.set("ArrowheadWidth", spell(length * 0.25));
        if let Some(gap) = gap {
            node.set("ArrowShaftSpacing", gap.into());
        }
        match kind.as_str() {
            "NoGo" => node.set("NoGo", "Cross".into()),
            "Dipole" => node.set("Dipole", "yes".into()),
            _ => {}
        }
        self.changed = true;
        self.arrow(index)
    }
    /// An arc from before ChemDraw stored arcs as arrows: its start point,
    /// then its centre.
    fn arc_graphic(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        let (Some(&[sx, sy, cx, cy]), Some(angle)) = (
            node.attr("BoundingBox").and_then(numbers).as_deref(),
            node.attr("AngularSize").and_then(number),
        ) else {
            self.drop_element(index)?;
            self.count(Kind::UnknownGraphic);
            return Ok(());
        };
        let heads = match node.attr("ArrowType") {
            Some("FullHead") => Some("Full"),
            Some("HalfHead") => Some("HalfLeft"),
            _ => None,
        };
        // The stored point is where the arc ends. With a head, the arc
        // arrives there from the far side; a plain arc turns back from it.
        let u = [sx - cx, sy - cy];
        let sweep = angle.clamp(-360., 360.).to_radians();
        let start = if heads.is_some() { sweep } else { -sweep };
        let (first, segments) = arc([cx, cy], u, [-u[1], u[0]], start, -start);
        let node = self.tree.node_mut(index)?;
        node.tag = "curve".into();
        node.attributes.retain(|(key, _)| {
            !matches!(
                key.as_str(),
                "GraphicType" | "ArrowType" | "BoundingBox" | "AngularSize"
            )
        });
        node.set("CurvePoints", curve_points(first, &segments));
        match heads {
            Some(head) => node.set("ArrowheadHead", head.into()),
            None => {
                if let Some(id) = node.attr("id") {
                    self.headless.insert(id.to_owned());
                }
            }
        }
        self.changed = true;
        self.curve(index)
    }

    /// ChemDraw saves a drawn bracket pair as two single brackets. Two that
    /// face each other upright become the pair again; any other single
    /// bracket keeps its shape as a curve.
    fn brackets(&mut self) -> Result<()> {
        struct Single {
            index: usize,
            parent: Option<usize>,
            kind: String,
            from: [f64; 2],
            to: [f64; 2],
            /// Colour, width, visibility and the like: a pair has one of each.
            style: Vec<(String, String)>,
        }
        let order = self.tree.descendants(0)?;
        let mut attached = HashSet::new();
        for &index in &order {
            let node = self.node(index)?;
            if node.tag == "bracketattachment"
                && let Some(id) = node.attr("GraphicID")
            {
                attached.insert(id.to_owned());
            }
        }
        let mut singles = Vec::new();
        for &index in &order {
            let node = self.node(index)?;
            if node.tag != "graphic"
                || node.attr("GraphicType") != Some("Bracket")
                || node.attr("SupersededBy").is_some_and(|id| !id.is_empty())
                || node
                    .attr("BracketUsage")
                    .is_some_and(|usage| usage != "Unspecified")
                || node.attr("id").is_some_and(|id| attached.contains(id))
            {
                continue;
            }
            let (Some(kind @ ("Round" | "Square" | "Curly")), Some(&[x1, y1, x2, y2])) = (
                node.attr("BracketType"),
                node.attr("BoundingBox").and_then(numbers).as_deref(),
            ) else {
                continue;
            };
            singles.push(Single {
                index,
                parent: node.parent,
                kind: kind.into(),
                from: [x1, y1],
                to: [x2, y2],
                style: node
                    .attributes
                    .iter()
                    .filter(|(key, _)| {
                        !matches!(
                            key.as_str(),
                            "id" | "Z" | "BoundingBox" | "Warning" | "GraphicType" | "BracketType"
                        )
                    })
                    .cloned()
                    .collect(),
            });
        }
        // An upright left bracket runs upwards and a right one downwards.
        let upright = |s: &Single| (s.from[0] - s.to[0]).abs() < 0.01;
        let mut pairs = Vec::new();
        // Each bracket is weighed against every other; a drawing with more
        // of them than any figure has keeps them all as curves.
        let candidates = if singles.len() <= 500 {
            singles.as_slice()
        } else {
            &[]
        };
        for (i, left) in candidates.iter().enumerate() {
            for (j, right) in candidates.iter().enumerate() {
                if left.parent == right.parent
                    && left.kind == right.kind
                    && left.style == right.style
                    && upright(left)
                    && upright(right)
                    && left.from[1] > left.to[1]
                    && right.from[1] < right.to[1]
                    && right.from[0] > left.from[0]
                    && (left.from[1] - right.to[1]).abs() < 0.5
                    && (left.to[1] - right.from[1]).abs() < 0.5
                {
                    pairs.push((right.from[0] - left.from[0], i, j));
                }
            }
        }
        pairs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut used = HashSet::new();
        for (_, i, j) in pairs {
            if used.contains(&i) || used.contains(&j) {
                continue;
            }
            used.extend([i, j]);
            let (Some(left), Some(right)) = (singles.get(i), singles.get(j)) else {
                continue;
            };
            let bounds = format!(
                "{} {} {} {}",
                spell(right.from[0]),
                spell(left.from[1]),
                spell(left.from[0]),
                spell(left.to[1])
            );
            let pair = format!("{}Pair", left.kind);
            let (keep, other) = (left.index, right.index);
            self.set(keep, "BracketType", pair)?;
            self.set(keep, "BoundingBox", bounds)?;
            self.drop_element(other)?;
        }
        for (i, single) in singles.iter().enumerate() {
            if used.contains(&i) {
                continue;
            }
            let (dx, dy) = (single.to[0] - single.from[0], single.to[1] - single.from[1]);
            let height = dx.hypot(dy);
            if height == 0. {
                self.drop_element(single.index)?;
                continue;
            }
            // The lips point away from the side the bracket bulges to.
            let lip = (height * 0.1).min(8.);
            let inward = [-dy / height * lip, dx / height * lip];
            let at = |x: f64, y: f64| {
                [
                    single.from[0] + inward[0] * x + dx * y,
                    single.from[1] + inward[1] * x + dy * y,
                ]
            };
            let line = |end: [f64; 2]| [end, end, end];
            let (start, segments) = match single.kind.as_str() {
                "Square" => {
                    let corner = at(0., 0.);
                    (
                        at(1., 0.),
                        vec![
                            [at(1., 0.), corner, corner],
                            [corner, at(0., 1.), at(0., 1.)],
                            line(at(1., 1.)),
                        ],
                    )
                }
                "Round" => (
                    at(1., 0.),
                    vec![[at(-0.33, 0.2), at(-0.33, 0.8), at(1., 1.)]],
                ),
                _ => (
                    at(1., 0.),
                    vec![
                        [at(0., 0.), at(0.6, 0.42), at(0., 0.5)],
                        [at(0.6, 0.58), at(0., 1.), at(1., 1.)],
                    ],
                ),
            };
            let node = self.tree.node_mut(single.index)?;
            node.tag = "curve".into();
            node.attributes.retain(|(key, _)| {
                !matches!(
                    key.as_str(),
                    "GraphicType" | "BracketType" | "BoundingBox" | "LipSize" | "Warning"
                )
            });
            node.set("CurvePoints", curve_points(start, &segments));
            self.changed = true;
            self.count(Kind::SingleBracket);
        }
        Ok(())
    }

    /// Caption and label settings the text reader refuses.
    fn text(&mut self, index: usize) -> Result<()> {
        let tag = self.node(index)?.tag.clone();
        let counted = matches!(tag.as_str(), "t" | "s");
        for key in ["face", "LabelFace", "CaptionFace"] {
            let Some(face) = self
                .attr(index, key)?
                .and_then(|face| face.trim().parse::<i64>().ok())
            else {
                continue;
            };
            let known = face & (1 | 2 | 4 | 32 | 64);
            if face == 65535 {
                // ChemDraw's value for "not set".
                self.remove(index, &[key])?;
            } else if known != face && face >= 0 {
                self.set(index, key, known.to_string())?;
                if counted {
                    self.count(Kind::TextFace);
                }
            }
        }
        // The document's own defaults belong to its drawing style.
        let sizes: &[&str] = match tag.as_str() {
            "s" => &["size"],
            "t" | "n" => &["LabelSize", "CaptionSize"],
            _ => &[],
        };
        for &key in sizes {
            if let Some(size) = self.attr(index, key)?.as_deref().and_then(number)
                && !(4.0..=144.).contains(&size)
            {
                self.set(index, key, spell(size.clamp(4., 144.)))?;
                self.count(Kind::TextSize);
            }
        }
        if !matches!(tag.as_str(), "t" | "n" | "page" | "CDXML") {
            return Ok(());
        }
        if self
            .attr(index, "RotationAngle")?
            .as_deref()
            .and_then(number)
            .is_some_and(|angle| angle != 0.)
        {
            self.remove(index, &["RotationAngle"])?;
            if tag == "t" {
                self.count(Kind::RotatedText);
            }
        }
        // Any other value, such as Auto, reads as the default left alignment.
        for key in ["Justification", "CaptionJustification"] {
            if self.attr(index, key)?.is_some_and(|value| {
                !matches!(value.as_str(), "Left" | "Center" | "Right" | "Full")
            }) {
                self.remove(index, &[key])?;
            }
        }
        if tag == "t" {
            self.paragraph(index)?;
            self.label_style(index)?;
        }
        Ok(())
    }
    /// An element's label takes one font, size and weight; only its colours
    /// may differ.
    fn label_style(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        let Some(atom) = node.parent else {
            return Ok(());
        };
        let parent = self.node(atom)?;
        if parent.tag != "n" || !matches!(parent.attr("NodeType"), None | Some("Element")) {
            return Ok(());
        }
        let root = self.node(0)?;
        // A run without a setting takes it from the label, the atom, then
        // the document, as the text reader does. 65535 is "not set".
        fn style<'a>(
            run: &'a Element,
            above: [&'a Element; 3],
        ) -> Option<(Option<&'a str>, f64, i64)> {
            let inherited = |key: &str| above.iter().find_map(|element| element.attr(key));
            let set = |face: &&str| face.trim() != "65535";
            let font = run.attr("font").or(inherited("LabelFont"));
            let size = run.attr("size").or(inherited("LabelSize")).unwrap_or("10");
            let face = run
                .attr("face")
                .filter(set)
                .or(inherited("LabelFace").filter(set))
                .unwrap_or("0");
            Some((font, number(size)?, face.trim().parse::<i64>().ok()?))
        }
        let above = [node, parent, root];
        let runs: Vec<_> = node
            .children
            .iter()
            .copied()
            .filter(|&child| self.node(child).is_ok_and(|run| run.tag == "s"))
            .collect();
        let Some(&first) = runs.first() else {
            return Ok(());
        };
        let Some((font, points, face)) = style(self.node(first)?, above) else {
            return Ok(());
        };
        let mut mixed = Vec::new();
        for &run in runs.iter().skip(1) {
            match style(self.node(run)?, above) {
                Some((run_font, run_points, run_face)) => {
                    if run_font != font || run_points != points || run_face & 7 != face & 7 {
                        mixed.push((run, run_face));
                    }
                }
                None => return Ok(()),
            }
        }
        if mixed.is_empty() {
            return Ok(());
        }
        let (font, size) = (font.map(str::to_owned), spell(points));
        for (run, run_face) in mixed {
            match &font {
                Some(font) => self.set(run, "font", font.clone())?,
                None => {
                    self.remove(run, &["font"])?;
                }
            }
            self.set(run, "size", size.clone())?;
            self.set(run, "face", (run_face & !7 | face & 7).to_string())?;
        }
        self.count(Kind::LabelStyle);
        Ok(())
    }
    fn paragraph(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        let label = node
            .parent
            .map(|parent| self.node(parent))
            .transpose()?
            .is_some_and(|parent| parent.tag == "n");
        let prefix = if label { "Label" } else { "Caption" };
        // The reader takes a setting from the text, then from its atom or
        // page, then from the document.
        let mut defaults = node.parent;
        if !label {
            while let Some(parent) = defaults {
                let parent_node = self.node(parent)?;
                if parent_node.tag == "page" {
                    break;
                }
                defaults = parent_node.parent;
            }
        }
        let inherited = |pass: &Self, from: &Element, key: &str| -> Result<Option<String>> {
            if let Some(value) = from.attr(key) {
                return Ok(Some(value.to_owned()));
            }
            if let Some(defaults) = defaults
                && let Some(value) = pass.node(defaults)?.attr(key)
            {
                return Ok(Some(value.to_owned()));
            }
            pass.attr(0, key)
        };
        let run = node
            .children
            .iter()
            .map(|&child| self.node(child))
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .find(|child| child.tag == "s");
        let size = match run.and_then(|run| run.attr("size")) {
            Some(size) => Some(size.to_owned()),
            None => inherited(self, node, &format!("{prefix}Size"))?,
        }
        .as_deref()
        .map_or(Some(10.), number)
        .map(|size| size.clamp(4., 144.));
        let height_key = format!("{prefix}LineHeight");
        let height = match inherited(self, node, &height_key)? {
            Some(height) => Some(height),
            None => inherited(self, node, "LineHeight")?,
        };
        let width = inherited(self, node, "WordWrapWidth")?;
        let spacing = height
            .filter(|height| {
                !matches!(
                    height.as_str(),
                    "auto" | "automatic" | "variable" | "0" | "1"
                )
            })
            .as_deref()
            .and_then(number)
            .zip(size)
            .map(|(height, size)| height / size);
        let mut changed = false;
        if spacing.is_some_and(|spacing| !(0.8..=3.).contains(&spacing)) {
            self.set(index, &height_key, "auto")?;
            changed = true;
        }
        if width
            .as_deref()
            .and_then(number)
            .is_some_and(|width| width != 0. && !(10.0..=2000.).contains(&width))
        {
            self.set(index, "WordWrapWidth", "0")?;
            changed = true;
        }
        if changed {
            self.count(Kind::Paragraph);
        }
        Ok(())
    }

    /// A reaction step may name an object this pass left out, or an arc that
    /// is no longer an arrow. The steps are only checked, never kept, so those
    /// names go; any other name stays for the check.
    fn references(&mut self) -> Result<()> {
        let order = self.tree.descendants(0)?;
        let mut present = HashSet::new();
        let mut replaced = HashMap::new();
        for &index in &order {
            let node = self.node(index)?;
            if let Some(id) = node.attr("id") {
                present.insert(id.to_owned());
                if let Some(replacement) = node.attr("SupersededBy") {
                    replaced.insert(id.to_owned(), replacement.to_owned());
                }
            }
        }
        let gone = |id: &str| self.original.contains(id) && !present.contains(id);
        let mut steps = Vec::new();
        for index in order {
            if self.node(index)?.tag == "step" {
                steps.push(index);
            }
        }
        for index in steps {
            let mut attributes = self.node(index)?.attributes.clone();
            for (key, value) in &mut attributes {
                if key == "id" || key.starts_with("ReactionStepAtomMap") {
                    continue;
                }
                let arrows = key == "ReactionStepArrows";
                *value = value
                    .split_whitespace()
                    .filter(|id| {
                        let stands = |id: &str| !gone(id) && !self.headless.contains(id);
                        !gone(id)
                            && !(arrows
                                && (self.headless.contains(*id)
                                    || replaced.get(*id).is_some_and(|other| !stands(other))))
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
            }
            self.tree.node_mut(index)?.attributes = attributes;
        }
        Ok(())
    }
}
