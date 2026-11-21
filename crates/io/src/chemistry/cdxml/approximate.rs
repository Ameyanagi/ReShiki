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
    f64::consts::{FRAC_1_SQRT_2, PI},
};

/// Orbital name part for ChemDraw's shaded phase. Only this pass writes it.
pub(super) const GRADIENT: &str = "Gradient";

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
    "tlcplate",
];
/// Children of a left-out object that still import on their own.
const SALVAGED: &[&str] = &["t", "graphic", "arrow", "curve", "fragment", "group"];

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Kind {
    DoubledCurve,
    DoubledPair,
    Faded,
    ArcStraight,
    EquilibriumRatio,
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
    TlcAppearance,
    BrokenCurve,
    VectorPicture,
    UnknownGraphic,
    OtherObject,
    ChemistrySetting,
}
impl Kind {
    /// Singular, plural, and how one or several were imported; `None` when
    /// they were left out.
    fn words(self) -> (&'static str, &'static str, Option<[&'static str; 2]>) {
        let same = |how| Some([how, how]);
        match self {
            Self::DoubledCurve => (
                "doubled curve",
                "doubled curves",
                Some(["as a single line", "as single lines"]),
            ),
            Self::DoubledPair => (
                "doubled curve",
                "doubled curves",
                Some(["as two grouped curves", "as pairs of grouped curves"]),
            ),
            Self::Faded => ("see-through object", "see-through objects", same("opaque")),
            Self::ArcStraight => ("arc arrow", "arc arrows", same("straight")),
            Self::EquilibriumRatio => (
                "unbalanced equilibrium arrow",
                "unbalanced equilibrium arrows",
                Some([
                    "with its shorter arrow lengthened to a fifth of the other",
                    "with their shorter arrows lengthened to a fifth of the other",
                ]),
            ),
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
            Self::StereoFlag => (
                "stereo flag",
                "stereo flags",
                same("as text in a rounded box"),
            ),
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
            Self::TlcAppearance => (
                "TLC plate",
                "TLC plates",
                same("with ordinary spots and supported Rf labels"),
            ),
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
            Self::ChemistrySetting => (
                "atom or bond setting ReShiki does not know",
                "atom or bond settings ReShiki does not know",
                None,
            ),
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
        next_id: 0,
        headless: HashSet::new(),
        resized: HashSet::new(),
        affected: HashSet::new(),
        budget: 0,
        bonded: HashSet::new(),
    };
    let mut curve_points = 0;
    for index in pass.tree.descendants(0)? {
        let node = pass.tree.node(index)?;
        if let Some(id) = node.attr("id") {
            pass.original.insert(id.to_owned());
        }
        if node.tag == "b" {
            let ends = ["B", "E"].map(|key| node.attr(key).map(str::to_owned));
            pass.bonded.extend(ends.into_iter().flatten());
        }
        // The reader may read a curve twice, inside a group of curves.
        if node.tag == "curve" {
            let count = |key: &str| {
                node.attr(key)
                    .map_or(0, |text| text.split_whitespace().count())
            };
            curve_points += 2 * (count("CurvePoints") / 2 + count("CurvePoints3D") / 3);
        }
    }
    pass.next_id = pass
        .original
        .iter()
        .filter_map(|id| id.parse::<u32>().ok())
        .max()
        .map_or(1, |id| id.saturating_add(1));
    pass.budget = 900_000usize.saturating_sub(curve_points);
    pass.objects(0)?;
    pass.brackets()?;
    for index in pass.tree.descendants(0)? {
        pass.element(index)?;
    }
    // Defaults each caption and label took were settled and counted above.
    // The document's label size is also its drawing style's.
    if let Some(size) = pass.attr(0, "LabelSize")?.as_deref().and_then(number)
        && !(4.0..=144.).contains(&size)
    {
        pass.set(0, "LabelSize", spell(size.clamp(4., 144.)))?;
    }
    for index in pass.tree.descendants(0)? {
        for key in ["LabelFace", "CaptionFace"] {
            if let Some(face) = pass
                .attr(index, key)?
                .and_then(|face| face.trim().parse::<i64>().ok())
                .and_then(plain_face)
            {
                pass.set(index, key, face.to_string())?;
            }
        }
    }
    // Validate owned Rf captions after the common text fallbacks have settled
    // their runs and inherited defaults, just like ordinary captions.
    for (index, dropped) in super::graphics::tlc::normalize(&mut pass.tree)? {
        pass.changed = true;
        pass.count(
            index,
            if dropped {
                Kind::TlcPlate
            } else {
                Kind::TlcAppearance
            },
        );
    }
    if !pass.changed {
        return Ok((xml.into(), None, 0));
    }
    pass.references()?;
    Ok((pass.tree.serialize()?, pass.notice(), pass.affected.len()))
}

struct Pass {
    tree: Tree,
    log: BTreeMap<Kind, usize>,
    changed: bool,
    /// Identifiers the file had before anything was rewritten.
    original: HashSet<String>,
    /// An identifier no object in the file has.
    next_id: u32,
    /// Arrows that became lines without a head.
    headless: HashSet<String>,
    /// Objects already counted for a size out of range.
    resized: HashSet<usize>,
    /// Objects drawn differently or left out, each once however many of
    /// their styles changed.
    affected: HashSet<usize>,
    /// Curve points the doubled curves' lines may still add: the reader
    /// counts every curve point against one budget, these lines twice.
    budget: usize,
    /// Atoms at either end of a bond.
    bonded: HashSet<String>,
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
/// The faces the text reader draws, when `face` has others.
fn plain_face(face: i64) -> Option<i64> {
    let known = face & (1 | 2 | 4 | 32 | 64);
    (known != face && face >= 0).then_some(known)
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
/// A path of Bézier segments moved `offset` along its normals: each segment
/// in quarters, whose ends move along the curve's normal there and whose
/// controls move with them. Corners meet at their mitre, or with a straight
/// piece where that would reach past four offsets.
fn parallel(
    start: [f64; 2],
    segments: &[[[f64; 2]; 3]],
    offset: f64,
    closed: bool,
) -> ([f64; 2], Vec<[[f64; 2]; 3]>) {
    let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) / 2., (a[1] + b[1]) / 2.];
    let halves = |[a, b, c, d]: [[f64; 2]; 4]| {
        let (ab, bc, cd) = (mid(a, b), mid(b, c), mid(c, d));
        let (abc, bcd) = (mid(ab, bc), mid(bc, cd));
        let m = mid(abc, bcd);
        [[a, ab, abc, m], [m, bcd, cd, d]]
    };
    // The unit normal of the first of these directions that has a length.
    let normal = |directions: [([f64; 2], [f64; 2]); 3]| {
        directions.into_iter().find_map(|(a, b)| {
            let (x, y) = (b[0] - a[0], b[1] - a[1]);
            let length = x.hypot(y);
            (length > 1e-9).then(|| [-y / length * offset, x / length * offset])
        })
    };
    let shift = |p: [f64; 2], n: [f64; 2]| [p[0] + n[0], p[1] + n[1]];
    // Where the two moved ends at `corner` meet: their mitre, if close.
    let join = |corner: [f64; 2], a: [f64; 2], b: [f64; 2]| {
        let scale = offset * offset / (offset * offset + a[0] * b[0] + a[1] * b[1]);
        let mitre = [(a[0] + b[0]) * scale, (a[1] + b[1]) * scale];
        (scale.is_finite() && mitre[0].hypot(mitre[1]) <= 4. * offset.abs())
            .then(|| shift(corner, mitre))
    };
    // Each piece's anchor, its moved controls and end, and both end normals.
    let mut pieces = Vec::new();
    let mut from = start;
    for &[b, c, d] in segments {
        for [p0, p1, p2, p3] in halves([from, b, c, d]).map(halves).into_iter().flatten() {
            if let (Some(n0), Some(n3)) = (
                normal([(p0, p1), (p0, p2), (p0, p3)]),
                normal([(p2, p3), (p1, p3), (p0, p3)]),
            ) {
                pieces.push((p0, n0, [shift(p1, n0), shift(p2, n3), shift(p3, n3)], n3));
            }
        }
        from = d;
    }
    let Some(&(anchor, normal, ..)) = pieces.first() else {
        return (start, Vec::new());
    };
    let mut first = shift(anchor, normal);
    let mut path: Vec<[[f64; 2]; 3]> = Vec::new();
    let mut previous: Option<[f64; 2]> = None;
    for (p0, n0, piece, n3) in pieces {
        if let (Some(before), Some(last)) = (previous, path.last_mut()) {
            let begin = shift(p0, n0);
            if (last[2][0] - begin[0]).hypot(last[2][1] - begin[1]) > 1e-6 {
                match join(p0, before, n0) {
                    Some(corner) => last[2] = corner,
                    None => {
                        let end = last[2];
                        path.push([end, begin, begin]);
                    }
                }
            }
        }
        path.push(piece);
        previous = Some(n3);
    }
    if closed && let (Some(before), Some(last)) = (previous, path.last_mut()) {
        if (last[2][0] - first[0]).hypot(last[2][1] - first[1]) > 1e-6 {
            match join(start, before, normal) {
                Some(corner) => (last[2], first) = (corner, corner),
                None => {
                    let end = last[2];
                    path.push([end, first, first]);
                }
            }
        } else {
            last[2] = first;
        }
    }
    (first, path)
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
    fn count(&mut self, index: usize, kind: Kind) {
        *self.log.entry(kind).or_default() += 1;
        // A text run belongs to its caption or label.
        let object = self
            .tree
            .node(index)
            .ok()
            .filter(|node| node.tag == "s")
            .and_then(|node| node.parent)
            .unwrap_or(index);
        self.affected.insert(object);
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
        // Older ChemDraw files keep atoms and bonds straight in a group;
        // ChemDraw reads them as one fragment there, and so does this.
        let loose: Vec<_> = self
            .node(index)?
            .children
            .iter()
            .copied()
            .filter(|&child| {
                self.node(child)
                    .is_ok_and(|node| matches!(node.tag.as_str(), "n" | "b"))
            })
            .collect();
        if !loose.is_empty() && matches!(self.node(index)?.tag.as_str(), "group" | "page") {
            let fragment = self.tree.create("fragment")?;
            self.set(fragment, "id", self.next_id.to_string())?;
            self.next_id = self.next_id.saturating_add(1);
            for child in loose {
                self.tree.append(fragment, child)?;
            }
            self.tree.append(index, fragment)?;
            self.changed = true;
        }
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
                // TLC normalization owns its lane, spot and Rf subtree.
                // Orphan lanes/spots still take the unknown-object warning.
                if tag != "tlcplate" {
                    self.objects(child)?;
                }
                continue;
            }
            changed = true;
            match tag.as_str() {
                // A template file's grid has nothing to draw.
                "templategrid" => {}
                "bioshape" => self.count(child, Kind::BioShape),
                "plasmidmap" => self.count(child, Kind::PlasmidMap),
                "table" => self.count(child, Kind::Table),
                "gepplate" => self.count(child, Kind::GelPlate),
                "tlcplate" => self.count(child, Kind::TlcPlate),
                _ => self.count(child, Kind::OtherObject),
            }
            self.tree.node_mut(child)?.parent = None;
            if root {
                continue;
            }
            for part in self.salvage(child, 0, false)? {
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
    fn salvage(&mut self, index: usize, depth: usize, hidden: bool) -> Result<Vec<usize>> {
        let mut kept = Vec::new();
        if depth > 64 {
            return Ok(kept);
        }
        let hidden = hidden || self.node(index)?.attr("Visible") == Some("no");
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
                kept.extend(self.salvage(child, depth + 1, hidden)?);
            }
        }
        // What a hidden object holds stays hidden with it.
        // A hidden group is refused, so its drawings are hidden one by one.
        if hidden {
            for &part in &kept {
                for leaf in self.tree.descendants(part)? {
                    let node = self.node(leaf)?;
                    // An atom's label shows with its atom.
                    let label = node
                        .parent
                        .map(|parent| self.node(parent))
                        .transpose()?
                        .is_some_and(|parent| parent.tag == "n");
                    if matches!(node.tag.as_str(), "t" | "graphic" | "arrow" | "curve") && !label {
                        self.tree.node_mut(leaf)?.set("Visible", "no".into());
                    }
                }
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
        // What the binary file held that the CDX decoder skipped.
        if let Some(count) = self.attr(index, crate::exchange::SKIPPED)? {
            self.remove(index, &[crate::exchange::SKIPPED])?;
            self.count(index, Kind::ChemistrySetting);
            // The notice counts every setting, the save question the object.
            let more = count.parse::<usize>().unwrap_or(1).saturating_sub(1);
            *self.log.entry(Kind::ChemistrySetting).or_default() += more;
        }
        self.text(index)?;
        // Hidden artwork is not drawn, and still marks out its page.
        if self.node(index)?.attr("Visible") == Some("no") {
            return Ok(());
        }
        match self.node(index)?.tag.as_str() {
            "arrow" => self.arrow(index),
            "n" => self.hydrogens_first(index),
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
                self.count(index, Kind::VectorPicture);
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

    /// ChemDraw anchors a label written hydrogens first, such as H2O, on its
    /// first letter; ReShiki puts the element on the atom and the hydrogens
    /// before it. A lone atom moves so its label covers the same place.
    fn hydrogens_first(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        // Not an atom with a 3D position, which would have to move as well.
        if node.attr("id").is_none_or(|id| self.bonded.contains(id))
            || node.attr("xyz").is_some()
            || !super::labels::hydrogens_first(&self.tree, node)?
        {
            return Ok(());
        }
        let Some([x, y]) = node.attr("p").and_then(point) else {
            return Ok(());
        };
        let label = node
            .children
            .iter()
            .filter_map(|&child| self.node(child).ok())
            .find(|child| child.tag == "t");
        let Some(&[left, _, right, _]) = label
            .and_then(|label| label.attr("BoundingBox"))
            .and_then(numbers)
            .as_deref()
        else {
            return Ok(());
        };
        self.set(
            index,
            "p",
            format!("{} {}", spell(right - (x - left)), spell(y)),
        )
    }
    /// ChemDraw's alpha makes the object see-through; ReShiki draws it opaque.
    fn fade(&mut self, index: usize) -> Result<()> {
        if let Some(alpha) = self.attr(index, "alpha")?.as_deref().and_then(number) {
            self.remove(index, &["alpha"])?;
            if alpha < 1. {
                self.count(index, Kind::Faded);
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
                self.count(index, Kind::Size);
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
                self.count(index, Kind::Size);
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
        // ReShiki draws the shorter arrow of an equilibrium at least a fifth
        // as long as the other.
        if let Some(ratio) = self
            .attr(index, "ArrowEquilibriumRatio")?
            .as_deref()
            .and_then(number)
            && !(20.0..=500.).contains(&ratio)
        {
            self.set(
                index,
                "ArrowEquilibriumRatio",
                spell(ratio.clamp(20., 500.)),
            )?;
            self.count(index, Kind::EquilibriumRatio);
        }
        // The arrow reader draws an arc around its centre; without a centre
        // and both axes, or with a sweep past a full turn, the arrow runs
        // straight from its tail to its head.
        let node = self.node(index)?;
        if let Some(angle) = node.attr("AngularSize").and_then(number)
            && angle != 0.
        {
            let radius = |key: &str| {
                let center = node.attr("Center3D").and_then(point)?;
                let end = node.attr(key).and_then(point)?;
                Some((end[0] - center[0]).hypot(end[1] - center[1]))
            };
            let drawn = angle.abs() <= 360.
                && [radius("MajorAxisEnd3D"), radius("MinorAxisEnd3D")]
                    .iter()
                    .all(|radius| radius.is_some_and(|r| r > 1e-6));
            if !drawn {
                self.set(index, "AngularSize", "0")?;
                self.count(index, Kind::ArcStraight);
            }
        }
        let node = self.node(index)?;
        if node.attr("Head3D").is_none() || node.attr("Tail3D").is_none() {
            // Nothing says where the arrow runs.
            self.drop_element(index)?;
            self.count(index, Kind::UnknownGraphic);
        }
        Ok(())
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
        let doubled = bits & 512 != 0;
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
            self.count(index, Kind::ClosedCurveHead);
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
                self.count(index, Kind::CurvedArrowMark);
            }
            // The arrow reader draws no thinner line.
            if arrow {
                let bold = bits & 4 != 0
                    || self
                        .attr(index, "LineType")?
                        .is_some_and(|line| line.contains("Bold"));
                let width = if bold { "BoldWidth" } else { "LineWidth" };
                self.clamp(index, width, 0.1, 12.)?;
            }
        }
        self.head_sizes(index)?;
        if doubled {
            let pair = self.double(index, bits)?;
            self.count(
                index,
                if pair {
                    Kind::DoubledPair
                } else {
                    Kind::DoubledCurve
                },
            );
        }
        Ok(())
    }
    /// ChemDraw strokes a doubled curve as two lines either side of a gap as
    /// wide as the bold width. Without a fill or heads they become a group of
    /// two parallel curves.
    fn double(&mut self, index: usize, bits: i64) -> Result<bool> {
        let node = self.node(index)?;
        let root = self.node(0)?;
        let heads = ["ArrowheadHead", "ArrowheadTail"]
            .iter()
            .any(|key| !matches!(node.attr(key).unwrap_or("None"), "None" | "Unspecified"));
        let Some(values) = node.attr("CurvePoints").and_then(numbers) else {
            return Ok(false);
        };
        if heads
            || bits & (128 | 256) != 0
            || matches!(node.attr("FillType"), Some("Solid" | "Shaded"))
            || node
                .attr("FadePercent")
                .and_then(number)
                .is_some_and(|fade| fade < 1000.)
        {
            return Ok(false);
        }
        let width = |key: &str, default: f64| {
            node.attr(key)
                .or_else(|| root.attr(key))
                .and_then(number)
                .unwrap_or(default)
        };
        let bold = width("BoldWidth", 2.);
        let line = if bits & 4 != 0 || node.attr("LineType").is_some_and(|t| t.contains("Bold")) {
            bold
        } else {
            width("LineWidth", 0.6)
        };
        // Each anchor between its two controls, as the reader takes them;
        // the point list was made whole above.
        let list = values.as_chunks::<2>().0;
        let (Some(&first), Some(&start), Some(rest), Some(&last)) =
            (list.first(), list.get(1), list.get(2..), list.last())
        else {
            return Ok(false);
        };
        let mut segments = rest.as_chunks::<3>().0.to_vec();
        let closed = bits & 1 != 0 || node.attr("Closed") == Some("yes");
        if closed && segments.last().is_some_and(|[.., end]| *end != start) {
            segments.push([last, first, start]);
        }
        // A drawn path holds 20,000 commands at most, and the file's curves
        // stay inside the reader's point budget.
        let rails =
            [1., -1.].map(|side| parallel(start, &segments, side * (bold + line) / 2., closed));
        let added: usize = rails.iter().map(|(_, path)| 2 * (3 * path.len() + 3)).sum();
        if rails
            .iter()
            .any(|(_, path)| path.is_empty() || path.len() > 19_000)
            || added > self.budget
        {
            return Ok(false);
        }
        self.budget -= added;
        let twin = self.tree.clone_subtree(index)?;
        self.tree
            .node_mut(twin)?
            .attributes
            .retain(|(key, _)| key != "id");
        for (curve, (first, path)) in [index, twin].into_iter().zip(rails) {
            self.set(curve, "CurvePoints", curve_points(first, &path))?;
        }
        self.group(index, twin)?;
        Ok(true)
    }
    /// Puts `extra` beside `index` in a new group where `index` stood.
    fn group(&mut self, index: usize, extra: usize) -> Result<()> {
        let parent = self.node(index)?.parent;
        let group = self.tree.create("group")?;
        if let Some(parent) = parent {
            let siblings = &mut self.tree.node_mut(parent)?.children;
            if let Some(slot) = siblings.iter_mut().find(|i| **i == index) {
                *slot = group;
            }
            self.tree.node_mut(group)?.parent = Some(parent);
        }
        self.tree.node_mut(index)?.parent = None;
        self.tree.append(group, index)?;
        self.tree.append(group, extra)
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
            self.count(index, Kind::BrokenCurve);
        }
        Ok(false)
    }

    fn graphic(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        // A mark on an atom is the mark reader's to judge.
        if node.attr("GraphicType") != Some("Symbol") && self.marks_atom(node) {
            return Ok(());
        }
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
            Some("Symbol") => {
                self.symbol_frame(index)?;
                self.symbol(index)
            }
            Some("Arc") => self.arc_graphic(index),
            Some("Bracket") => Ok(()),
            _ => {
                self.drop_element(index)?;
                self.count(index, Kind::UnknownGraphic);
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
                self.count(index, Kind::UnknownGraphic);
                Ok(())
            }
        }
    }
    /// ChemDraw's orbital names in the spelling the reader knows: the shape,
    /// its phase, then `2` for the mirrored one. ChemDraw shades one phase of
    /// a plain orbital of more than one part, and the whole of a Shaded one.
    fn orbital(&mut self, index: usize) -> Result<()> {
        let Some(name) = self.attr(index, "OrbitalType")? else {
            return Ok(());
        };
        let (base, phase) = if let Some(base) = name.strip_suffix("Filled") {
            (base, "Solid")
        } else if let Some(base) = name.strip_suffix("Shaded") {
            (base, GRADIENT)
        } else {
            (name.as_str(), "")
        };
        let (shape, mirrored) = match base {
            "s" | "oval" | "lobe" => (base, false),
            "p" | "dxy" => (base, false),
            // ChemDraw's plus fills the small lobe or the ring, where
            // ReShiki's mirrored phase is.
            "hybridPlus" => ("sp3", true),
            "hybridMinus" => ("sp3", false),
            "dz2Plus" => ("dz2", true),
            "dz2Minus" => ("dz2", false),
            _ => return Ok(()),
        };
        // A plain s, oval or lobe is an outline, as the reader reads it.
        let single = matches!(shape, "s" | "oval" | "lobe");
        if phase.is_empty() && single {
            return Ok(());
        }
        let phase = if phase.is_empty() { GRADIENT } else { phase };
        // ChemDraw's dxy has its lobes along its axis and across it, the
        // reader's between them: turn the axis back an eighth of a turn.
        if shape == "dxy"
            && let Some(&[x1, y1, x2, y2]) = self
                .attr(index, "BoundingBox")?
                .as_deref()
                .and_then(numbers)
                .as_deref()
        {
            let (x, y) = ((x1 - x2) * FRAC_1_SQRT_2, (y1 - y2) * FRAC_1_SQRT_2);
            let end = [x2 + x + y, y2 + y - x];
            self.set(
                index,
                "BoundingBox",
                format!(
                    "{} {} {} {}",
                    spell(end[0]),
                    spell(end[1]),
                    spell(x2),
                    spell(y2)
                ),
            )?;
        }
        self.set(
            index,
            "OrbitalType",
            format!("{shape}{phase}{}", if mirrored { "2" } else { "" }),
        )
    }
    /// ChemDraw draws a sign, dagger or dot upright on a free symbol's first
    /// point, as large as the distance to its second, and a lone pair with a
    /// dot on each point. The reader turns a symbol to the line from its
    /// second point to its first, so that line is laid out here as ChemDraw
    /// draws the symbol. A symbol on an atom is the mark reader's to place.
    fn symbol_frame(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        if self.marks_atom(node) {
            return Ok(());
        }
        let Some(&[ax, ay, bx, by]) = node.attr("BoundingBox").and_then(numbers).as_deref() else {
            return Ok(());
        };
        let (dx, dy) = (ax - bx, ay - by);
        let size = dx.hypot(dy);
        let middle = [(ax + bx) / 2., (ay + by) / 2.];
        // The symbol's middle and the axis the reader turns it to.
        let (origin, axis) = match node.attr("SymbolType") {
            Some(
                "Plus" | "Minus" | "CirclePlus" | "CircleMinus" | "Dagger" | "DoubleDagger"
                | "HDot" | "HDash" | "Attachment",
            ) => ([ax, ay], [size, 0.]),
            // The reader doubles a dot's axis.
            Some("Electron" | "Radical") => ([ax, ay], [size / 2., 0.]),
            // Its dots stand 0.4 of the doubled axis either side of the middle.
            Some("LonePair" | "ElectronPair") => (middle, [dx * 1.25, dy * 1.25]),
            // A bar 1.4 times as long as the distance, between the points.
            Some("LonePairBar") => (middle, [dx * 2., dy * 2.]),
            // The dot on the first point and the upright sign on the second:
            // the reader's dot is 0.45 of its axis behind the middle and the
            // sign 0.22 ahead, so the axis runs from the first point toward
            // the second, 1/0.67 times as long.
            Some("RadicalCation" | "RadicalAnion") => {
                let axis = [-dx / 0.67, -dy / 0.67];
                ([ax + 0.45 * axis[0], ay + 0.45 * axis[1]], axis)
            }
            _ => return Ok(()),
        };
        self.set(
            index,
            "BoundingBox",
            format!(
                "{} {} {} {}",
                spell(origin[0]),
                spell(origin[1]),
                spell(origin[0] - axis[0]),
                spell(origin[1] - axis[1])
            ),
        )
    }
    fn marks_atom(&self, node: &Element) -> bool {
        node.children
            .iter()
            .any(|&child| self.node(child).is_ok_and(|child| child.tag == "represent"))
    }
    fn symbol(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        let marks_atom = self.marks_atom(node);
        // Earlier ReShiki versions wrote an atom's lone pair with the value
        // ChemDraw uses for the bar, which never marks an atom.
        if marks_atom && node.attr("SymbolType") == Some("LonePairBar") {
            return self.set(index, "SymbolType", "LonePair");
        }
        // ChemDraw's label in its caption size, and the widths of that label
        // and its rounded box, in ems.
        let (label, text_width, box_width) = match node.attr("SymbolType") {
            Some("Racemic") => ("Rac", 1.78, 2.3),
            Some("Absolute") => ("Abs", 1.72, 2.29),
            Some("Relative") => ("Rel", 1.5, 1.96),
            Some(
                "CirclePlus" | "CircleMinus" | "Plus" | "Minus" | "Radical" | "Electron"
                | "LonePair" | "ElectronPair" | "RadicalCation" | "RadicalAnion" | "HDot" | "HDash"
                | "Attachment" | "Dagger" | "DoubleDagger" | "LonePairBar",
            ) => return Ok(()),
            // A symbol that marks an atom is the mark reader's to judge.
            _ if marks_atom => return Ok(()),
            _ => {
                self.drop_element(index)?;
                self.count(index, Kind::UnknownGraphic);
                return Ok(());
            }
        };
        let Some(&[x, y, _, _]) = node.attr("BoundingBox").and_then(numbers).as_deref() else {
            return Ok(());
        };
        let color = node.attr("color").map(str::to_owned);
        // The label takes the caption size of its page or document, as read.
        let mut size = 10.;
        let mut at = Some(index);
        while let Some(i) = at {
            let node = self.node(i)?;
            if matches!(node.tag.as_str(), "page" | "CDXML")
                && let Some(value) = node.attr("CaptionSize").and_then(number)
            {
                size = value.clamp(4., 144.);
                break;
            }
            at = node.parent;
        }
        let node = self.node(index)?;
        let kept: Vec<(String, String)> = node
            .attributes
            .iter()
            .filter(|(key, _)| matches!(key.as_str(), "Z" | "color" | "Visible"))
            .cloned()
            .collect();
        for child in node.children.clone() {
            self.tree.detach(child)?;
        }
        // The box, centered on the symbol's first point as ChemDraw draws it,
        // with ChemDraw's proportions.
        let frame = self.tree.create("graphic")?;
        let (half_width, half_height) = (box_width * size / 2., 0.66 * size);
        let mut attributes = vec![
            ("GraphicType".into(), "Rectangle".into()),
            ("RectangleType".into(), "RoundEdge".into()),
            ("CornerRadius".into(), spell(size * 57.)),
            (
                "BoundingBox".into(),
                format!(
                    "{} {} {} {}",
                    spell(x + half_width),
                    spell(y - half_height),
                    spell(x - half_width),
                    spell(y + half_height)
                ),
            ),
        ];
        attributes.extend(kept);
        self.tree.node_mut(frame)?.attributes = attributes;
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
        // The label's baseline, and its box, which places it on import.
        let left = spell(x - text_width * size / 2.);
        node.set("p", format!("{left} {}", spell(y + 0.35 * size)));
        // One left-aligned line, whatever the page sets for captions.
        node.set("CaptionJustification", "Left".into());
        node.set("CaptionLineHeight", "auto".into());
        node.set("WordWrapWidth", "0".into());
        node.set(
            "BoundingBox",
            format!(
                "{left} {} {} {}",
                spell(y - 0.56 * size),
                spell(x + text_width * size / 2.),
                spell(y + 0.56 * size)
            ),
        );
        self.group(index, frame)?;
        self.changed = true;
        self.count(index, Kind::StereoFlag);
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
        // A negative size draws the other half of a half head.
        let length = match node.attr("HeadSize") {
            None => 1000.,
            Some(size) => match number(size) {
                Some(size) => size,
                // The reader refuses it as it stands.
                None => return Ok(()),
            },
        };
        let half = |head: &'static str| match head {
            "HalfLeft" if length < 0. => "HalfRight",
            _ => head,
        };
        let (head, tail, length) = (half(head), tail.map(half), length.abs());
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
        node.set("HeadSize", spell(length));
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
            self.count(index, Kind::UnknownGraphic);
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
            self.count(single.index, Kind::SingleBracket);
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
            if face == 65535 {
                // ChemDraw's value for "not set".
                self.remove(index, &[key])?;
            } else if counted && let Some(known) = plain_face(face) {
                // A default is settled where a caption or label takes it.
                self.set(index, key, known.to_string())?;
                self.count(index, Kind::TextFace);
            }
        }
        // A size a caption or label inherits from its page or the document
        // is settled for each one that takes it.
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
                self.count(index, Kind::TextSize);
            }
        }
        if !matches!(tag.as_str(), "t" | "n" | "page" | "CDXML") {
            return Ok(());
        }
        // Captions turn; atom labels and inherited defaults stay upright.
        let parent = self.node(index)?.parent;
        // TLC Rf captions are owned text, with their own measured baseline and
        // explicit turn. Other object tags retain the legacy upright fallback.
        let tlc_rf_caption = tag == "t"
            && parent
                .map(|i| super::graphics::tlc::is_rf_tag(&self.tree, i))
                .transpose()?
                .unwrap_or(false);
        let caption = tlc_rf_caption
            || tag == "t"
                && match parent {
                    Some(parent) => !matches!(self.node(parent)?.tag.as_str(), "n" | "objecttag"),
                    None => true,
                };
        if !caption
            && self
                .attr(index, "RotationAngle")?
                .as_deref()
                .and_then(number)
                .is_some_and(|angle| angle != 0.)
        {
            self.remove(index, &["RotationAngle"])?;
            if tag == "t" {
                self.count(index, Kind::RotatedText);
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
    /// An atom label, including an abbreviation's, takes one font, size and
    /// weight; only its colours may differ.
    fn label_style(&mut self, index: usize) -> Result<()> {
        let node = self.node(index)?;
        let Some(atom) = node.parent else {
            return Ok(());
        };
        let parent = self.node(atom)?;
        if parent.tag != "n" {
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
        self.count(index, Kind::LabelStyle);
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
        // Runs without a size take the one the text inherits.
        let size_key = format!("{prefix}Size");
        let inherits = node.attr(&size_key).is_none()
            && node.children.iter().any(|&child| {
                self.node(child)
                    .is_ok_and(|run| run.tag == "s" && run.attr("size").is_none())
            });
        let inherited_size = if inherits {
            inherited(self, node, &size_key)?
                .as_deref()
                .and_then(number)
                .filter(|size| !(4.0..=144.).contains(size))
        } else {
            None
        };
        // The same for an outline or shadow the text inherits.
        let face_key = format!("{prefix}Face");
        let inherits = node.attr(&face_key).is_none()
            && node.children.iter().any(|&child| {
                self.node(child)
                    .is_ok_and(|run| run.tag == "s" && run.attr("face").is_none())
            });
        let inherited_face = if inherits {
            inherited(self, node, &face_key)?
                .and_then(|face| face.trim().parse::<i64>().ok())
                .and_then(plain_face)
        } else {
            None
        };
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
        if let Some(size) = inherited_size {
            self.set(index, &size_key, spell(size.clamp(4., 144.)))?;
            self.count(index, Kind::TextSize);
        }
        if let Some(face) = inherited_face {
            self.set(index, &face_key, face.to_string())?;
            self.count(index, Kind::TextFace);
        }
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
            self.count(index, Kind::Paragraph);
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
