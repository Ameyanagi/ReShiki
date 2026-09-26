//! Editable document serialization. Chemistry validation and presentation stay
//! separate; the original Python writer is the differential test reference.
use crate::{
    document::{Document, Point},
    engine::{Request, TextMetrics},
    graphics::PathCommand,
    typography::{TextFormat, TextStyle},
};
use std::collections::HashMap;
mod clipboard;
mod groups;
mod labels;
mod ligands;
mod objects;
mod tree;
pub(crate) use clipboard::write as write_clipboard;
use tree::{Key, Tree};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid editable drawing: {0}")]
    Invalid(String),
    #[error(transparent)]
    Chemistry(#[from] crate::chemistry::document::Error),
    #[error("Editable drawing exceeds its size or work limit")]
    Limit,
}
type Result<T> = std::result::Result<T, Error>;
fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}

#[derive(Default)]
pub struct Options<'a> {
    pub text_layout: Option<&'a HashMap<u64, TextMetrics>>,
    pub graphic_paths: Option<&'a HashMap<u64, Vec<PathCommand>>>,
    pub graphic_parts: Option<&'a HashMap<u64, Vec<crate::scientific::Part>>>,
    pub atom_indicators: Option<&'a [crate::atom_labels::Indicator]>,
}
impl<'a> From<&'a Request> for Options<'a> {
    fn from(request: &'a Request) -> Self {
        Self {
            text_layout: request.text_layout.as_ref(),
            graphic_paths: request.graphic_paths.as_ref(),
            graphic_parts: request.graphic_parts.as_ref(),
            atom_indicators: request.atom_indicators.as_deref(),
        }
    }
}

// The bridge first serializes into a JSON Value, promoting f32 to f64.
// Keep that arithmetic at the file boundary without intermediate f32 rounding.
fn real(value: f32) -> f64 {
    f64::from(value)
}
fn default_real(value: f32) -> f64 {
    value.to_string().parse().unwrap_or_else(|_| real(value))
}
fn number(value: f64) -> String {
    format!("{value:?}")
}
fn yes(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}
#[derive(Clone, Copy, Default)]
struct P {
    x: f64,
    y: f64,
}
impl From<Point> for P {
    fn from(p: Point) -> Self {
        Self {
            x: real(p.x),
            y: real(p.y),
        }
    }
}
impl P {
    fn add(self, x: f64, y: f64) -> Self {
        Self {
            x: self.x + x,
            y: self.y + y,
        }
    }
    fn lerp(self, b: Self, t: f64) -> Self {
        self.add((b.x - self.x) * t, (b.y - self.y) * t)
    }
}

struct Writer<'a> {
    doc: &'a Document,
    options: Options<'a>,
    tree: Tree,
    fonts: Key,
    colors: Key,
    page: Key,
    fragment: Key,
    font_ids: HashMap<String, usize>,
    color_ids: HashMap<[u8; 3], usize>,
    atom_indices: HashMap<u64, usize>,
    atom_nodes: Vec<Key>,
    bond_nodes: Vec<Key>,
    objects: Vec<(u64, Key)>,
    next: usize,
    scale: f64,
    shift: P,
    work: usize,
    variable_labels: bool,
}

/// Serialize a complete drawing without mutating it. Reject unrepresentable
/// appearances and ownership rather than detaching or flattening objects.
pub fn write(document: &Document, options: Options<'_>) -> Result<String> {
    write_impl(document, options, false, false)
}

/// Transfer a validated drawing even when its chemical assignment is pending.
/// Preserve explicit input; do not manufacture charges or molecular properties.
pub(crate) fn write_preserving(document: &Document, options: Options<'_>) -> Result<String> {
    write_impl(document, options, true, false)
}

fn write_impl(
    document: &Document,
    options: Options<'_>,
    preserve_drawing: bool,
    variable_labels: bool,
) -> Result<String> {
    let resolved = crate::canvas_theme::resolved_document(document);
    let document = resolved.as_ref();
    document.validate().map_err(invalid)?;
    if document
        .atoms
        .iter()
        .any(|a| a.charge != 0 && a.display.hide_charge && !ligands::hidden_charge(a))
    {
        return Err(invalid(
            "CDXML cannot yet preserve hidden charge labels. Show charges before editable export, or use ReShiki (.rsk), SVG, PNG or PDF. The chemical charges are retained.",
        ));
    }
    let haworth = crate::haworth::interchange::export_bonds(document).map_err(invalid)?;
    // ChemDraw 26 discards nested MultiAttachment definitions when saving a
    // Fragment label. Expand these groups for editable exchange so every real
    // atom and target survives; native/figure output keeps the compact label.
    let mut expanded;
    let document = if document.abbreviations.iter().any(|g| {
        g.members
            .iter()
            .any(|id| document.atom(*id).is_some_and(|a| a.attachment.is_some()))
    }) {
        expanded = document.clone();
        expanded.abbreviations.retain(|g| {
            !g.members
                .iter()
                .any(|id| document.atom(*id).is_some_and(|a| a.attachment.is_some()))
        });
        &expanded
    } else {
        document
    };
    if document.bonds.iter().any(|b| b.ring_arc)
        || (!variable_labels && document.atoms.iter().any(|a| a.display.variable.is_some()))
    {
        return Err(invalid(
            "CDXML cannot yet preserve inner ring curves or variable atom labels. Save as ReShiki (.rsk) or export SVG/PDF to keep this appearance.",
        ));
    }
    let interchange = crate::attachments::interchange_graph(document).map_err(invalid)?;
    let (graph, rings) = match crate::chemistry::document::prepare(&interchange) {
        Ok(molecule) => (molecule.state.graph, molecule.state.rings.atoms),
        Err(crate::chemistry::document::Error::Sanitization(_)) if preserve_drawing => {
            let graph = crate::chemistry::document::drawing_graph(&interchange)?;
            let rings = crate::chemistry::rings::perceive(&graph, Default::default())
                .map_err(|e| invalid(e.to_string()))?
                .atoms;
            (graph, rings)
        }
        Err(error) => return Err(error.into()),
    };
    if document.bonds.iter().enumerate().any(|(i, bond)| {
        // A chemically aromatic bold edge cannot specify a tetrahedral center.
        // Other projected styles still require a verified interchange mapping.
        let aromatic_bold = matches!(bond.display.as_str(), "bold" | "wedge")
            && graph.bonds.get(i).is_some_and(|b| b.aromatic)
            && [bond.a, bond.b]
                .iter()
                .all(|id| document.atom(*id).is_some_and(|a| a.stereo.is_none()));
        bond.projection && bond.display != "plain" && !haworth.contains(&i) && !aromatic_bold
    }) {
        return Err(invalid(
            "CDXML cannot yet preserve non-stereochemical front-bond emphasis or projected wedge styles. Restore plain bond appearance before editable export, or use ReShiki (.rsk), SVG, PNG or PDF to retain the appearance.",
        ));
    }
    let mut w = Writer::new(document, options)?;
    w.variable_labels = variable_labels;
    w.atoms(&graph)?;
    w.bonds()?;
    w.circles(&rings)?;
    w.labels()?;
    w.annotations()?;
    w.marks()?;
    w.hidden_charges()?;
    w.arrows()?;
    let middle = w.graphics()?;
    let arrow_nodes: std::collections::HashSet<_> = document
        .arrows
        .iter()
        .filter_map(|a| {
            w.objects
                .iter()
                .find(|(id, _)| *id == a.id)
                .map(|(_, n)| *n)
        })
        .collect();
    for key in w.tree.descendants(w.page)? {
        if matches!(
            w.tree.node(key)?.tag,
            "fragment" | "n" | "b" | "t" | "arrow"
        ) || arrow_nodes.contains(&key)
        {
            w.tree.set(key, "Z", middle.to_string())?;
        }
    }
    w.crossings(middle)?;
    w.groups()?;
    w.ring_fills()?;
    w.abbreviations()?;
    if !document.ring_fills.is_empty() || document.canvas_theme.is_dark() {
        // Editable readers need a distinct stacking ordinal for every object.
        // Equal Z values on atoms/bonds can make an opaque fill cover them.
        let mut layers = Vec::new();
        for key in w.tree.descendants(w.page)? {
            if let Some(z) = w.tree.get(key, "Z")? {
                layers.push((
                    z.parse::<usize>()
                        .map_err(|_| invalid("Invalid stacking order"))?,
                    key,
                ));
            }
        }
        layers.sort_by_key(|(z, _)| *z);
        for (rank, (_, key)) in layers.into_iter().enumerate() {
            w.tree.set(key, "Z", (rank + 1).to_string())?;
        }
    }
    if document.canvas_theme.is_dark() {
        // A real filled object survives paste into another ChemDraw document;
        // a document background setting alone does not travel with a selection.
        let (lo, hi) = crate::scene::bounds(&crate::scene::primitives(document));
        let background = w.color([255; 3])?;
        w.tree.set(0, "bgcolor", background.clone())?;
        let id = w.id()?;
        // ChemDraw treats Z=0 as unspecified. Reserve a positive backmost rank.
        for key in w.tree.descendants(w.page)? {
            if let Some(z) = w.tree.get(key, "Z")? {
                let z = z
                    .parse::<usize>()
                    .map_err(|_| invalid("Invalid stacking order"))?;
                w.tree.set(key, "Z", (z + 1).to_string())?;
            }
        }
        let rectangle = w.tree.add(
            Some(w.page),
            "graphic",
            [
                ("id", id),
                ("Z", "1".into()),
                ("GraphicType", "Rectangle".into()),
                ("RectangleType", "Filled".into()),
                ("FillType", "Solid".into()),
                ("LineType", "Solid".into()),
                ("LineWidth", "0".into()),
                ("color", background),
                (
                    "BoundingBox",
                    format!("{} {}", w.position(lo), w.position(hi)),
                ),
            ],
        )?;
        let children = &mut w.tree.node_mut(w.page)?.children;
        children.retain(|key| *key != rectangle);
        children.insert(0, rectangle);
    }
    w.tree.serialize()
}

impl<'a> Writer<'a> {
    fn spend(&mut self, n: usize) -> Result<()> {
        self.work = self.work.checked_sub(n).ok_or(Error::Limit)?;
        Ok(())
    }
    fn id(&mut self) -> Result<String> {
        let id = self.next;
        self.next = self.next.checked_add(1).ok_or(Error::Limit)?;
        Ok(id.to_string())
    }
    fn new(doc: &'a Document, options: Options<'a>) -> Result<Self> {
        let style = &doc.drawing_style;
        // Default drawing settings are omitted from the native document. The
        // reference then loads their decimal values from the shared style file.
        let style_real = if style.is_default() {
            default_real
        } else {
            real
        };
        let scale = style_real(style.bond_length_pt) / style_real(style.bond_length_world);
        let mut points: Vec<P> = doc
            .atoms
            .iter()
            .map(|a| a.position.into())
            .chain(doc.annotations.iter().map(|a| a.position.into()))
            .collect();
        for a in &doc.arrows {
            points.extend([P::from(a.start), P::from(a.end)]);
            if let Some(c) = a.control {
                points.push(c.into());
            }
        }
        for g in &doc.graphics {
            if g.kind == crate::graphics::GraphicKind::Picture {
                let o = P::from(g.origin);
                let x = P::from(g.axis_x);
                let y = P::from(g.axis_y);
                points.extend([
                    o,
                    o.add(x.x, x.y),
                    o.add(y.x, y.y),
                    o.add(x.x + y.x, x.y + y.y),
                ]);
            } else if let Some(path) = options.graphic_paths.and_then(|paths| paths.get(&g.id)) {
                points.extend(path.iter().flat_map(PathCommand::points).map(P::from));
            }
        }
        if points.iter().any(|p| !p.x.is_finite() || !p.y.is_finite()) {
            return Err(invalid("Non-finite drawing coordinate"));
        }
        let shift = P {
            x: 30.
                - points
                    .iter()
                    .map(|p| p.x * scale)
                    .reduce(f64::min)
                    .unwrap_or(0.),
            y: 30.
                - points
                    .iter()
                    .map(|p| p.y * scale)
                    .reduce(f64::min)
                    .unwrap_or(0.),
        };
        let mut tree = Tree::new();
        for (key, value) in [
            ("BondLength", style.bond_length_pt),
            ("LabelSize", style.font_size_pt),
            ("CaptionSize", style.font_size_pt),
        ] {
            tree.set(0, key, number(style_real(value)))?;
        }
        for (key, value) in [
            ("LabelFont", "3"),
            ("CaptionFont", "3"),
            ("LabelFace", "0"),
            ("CaptionFace", "0"),
        ] {
            tree.set(0, key, value)?;
        }
        for (key, value) in [
            ("LineWidth", style.line_width_pt),
            ("BoldWidth", style.bold_width_pt),
            ("MarginWidth", style.margin_width_pt),
            ("HashSpacing", style.hash_spacing_pt),
        ] {
            tree.set(0, key, number(style_real(value)))?;
        }
        tree.set(
            0,
            "BondSpacing",
            // Keep the shortest decimal percentage. Widening the f32 ratio
            // first produces 11.9999997 for 12%; ChemDraw truncates that to
            // 11.9% when it stores the value in its binary format.
            number(default_real(style.bond_spacing_ratio * 100.)),
        )?;
        tree.set(0, "ChainAngle", "120")?;
        let fonts = tree.add(Some(0), "fonttable", [])?;
        let colors = tree.add(Some(0), "colortable", [])?;
        let page = tree.add(
            Some(0),
            "page",
            [("id", "1".into()), ("BoundingBox", "0 0 612 792".into())],
        )?;
        let fragment = tree.add(Some(page), "fragment", [("id", "2".into())])?;
        let mut w = Self {
            doc,
            options,
            tree,
            fonts,
            colors,
            page,
            fragment,
            font_ids: HashMap::new(),
            color_ids: HashMap::new(),
            atom_indices: doc
                .atoms
                .iter()
                .enumerate()
                .map(|(i, a)| (a.id, i))
                .collect(),
            atom_nodes: Vec::new(),
            bond_nodes: Vec::new(),
            objects: Vec::new(),
            next: doc.atoms.len() + 3,
            scale,
            shift,
            work: 50_000_000,
            variable_labels: false,
        };
        w.font(&style.font_family)?;
        // ChemDraw requires the first two explicit entries to remain white and
        // black, even when the drawing uses a dark page. Bypass theme mapping.
        w.raw_color([255; 3])?;
        w.raw_color([0; 3])?;
        Ok(w)
    }
    fn position(&self, p: impl Into<P>) -> String {
        let p = p.into();
        format!(
            "{:.6} {:.6}",
            p.x * self.scale + self.shift.x,
            p.y * self.scale + self.shift.y
        )
    }
    fn font(&mut self, name: &str) -> Result<String> {
        if let Some(id) = self.font_ids.get(name) {
            return Ok(id.to_string());
        }
        let id = self.font_ids.len() + 3;
        self.tree.add(
            Some(self.fonts),
            "font",
            [
                ("id", id.to_string()),
                ("charset", "utf-8".into()),
                ("name", name.into()),
            ],
        )?;
        self.font_ids.insert(name.into(), id);
        Ok(id.to_string())
    }
    fn color(&mut self, rgb: [u8; 3]) -> Result<String> {
        self.raw_color(self.doc.canvas_theme.color(rgb))
    }
    fn raw_color(&mut self, rgb: [u8; 3]) -> Result<String> {
        if let Some(id) = self.color_ids.get(&rgb) {
            return Ok(id.to_string());
        }
        let id = self.color_ids.len() + 2;
        let attrs = ["r", "g", "b"]
            .into_iter()
            .zip(rgb)
            .map(|(axis, c)| (axis, format!("{:.8}", f64::from(c) / 255.)))
            .collect::<Vec<_>>();
        self.tree.add(Some(self.colors), "color", attrs)?;
        self.color_ids.insert(rgb, id);
        Ok(id.to_string())
    }
    fn atom(&self, id: u64) -> Result<&crate::document::Atom> {
        self.atom_indices
            .get(&id)
            .and_then(|i| self.doc.atoms.get(*i))
            .ok_or_else(|| invalid("Missing drawing atom"))
    }
    fn atom_xml(&self, id: u64) -> Result<String> {
        self.atom_indices
            .get(&id)
            .map(|i| (i + 3).to_string())
            .ok_or_else(|| invalid("Missing drawing atom ID"))
    }
    fn atom_node(&self, id: u64) -> Result<Key> {
        self.atom_indices
            .get(&id)
            .and_then(|i| self.atom_nodes.get(*i))
            .copied()
            .ok_or_else(|| invalid("Missing atom object"))
    }
}
