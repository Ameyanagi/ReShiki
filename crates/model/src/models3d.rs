//! 3D models of structures (#386): a stored conformer of one molecule, drawn as
//! vector ball & stick, stick, wireframe or space-filling in place of its 2D
//! drawing or beside it.
//!
//! A model links to its molecule by atom IDs and never changes chemistry,
//! identities or the 2D layout, which stays for switching back. Its frame is
//! placed relative to the molecule's 2D centre, so moving, aligning and copying
//! the atoms carries it along. A chemical edit to the molecule drops the model
//! instead of repairing it; a molecule shown in 3D then shows its 2D drawing.
//! Chemical formats (MOL, CDX/CDXML, SMILES) keep the 2D atoms and omit models.
use crate::{
    depth_appearance::Scope,
    document::{Atom, AtomStereo, Bond, Document, Point},
    scene::{Outline, Primitive},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// How a molecule with a model is shown.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Display {
    /// The 2D drawing; the model is kept so switching back is instant.
    #[default]
    TwoD,
    /// The model in place of the 2D drawing.
    ThreeD,
    /// The 2D drawing with the model beside it.
    Both,
}

/// The 2D layout a molecule's 2D buttons show.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// The layout as drawn.
    #[default]
    Original,
    /// The model flattened as it is turned now, far bonds lighter.
    View,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Style {
    #[default]
    BallStick,
    /// Bonds as thick cylinders joined at the atoms.
    Stick,
    /// Bonds as thin lines in the atom colours; no balls.
    Wireframe,
    /// Balls at van der Waals radii; no bonds.
    SpaceFilling,
}
impl Style {
    pub const ALL: [Self; 4] = [
        Self::BallStick,
        Self::Stick,
        Self::Wireframe,
        Self::SpaceFilling,
    ];
    /// Ball radius (Å) of an atom drawn in this style.
    fn atom_radius(self, symbol: &str) -> f32 {
        match self {
            Self::BallStick => element(symbol).1,
            Self::Stick => STICK_RADIUS,
            Self::Wireframe => WIRE_RADIUS,
            Self::SpaceFilling => van_der_waals(symbol),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Look {
    /// CYLview-like shading with thin dark outlines, for journals.
    #[default]
    Flat,
    /// Brighter highlights and deeper shade without outlines, for slides.
    Glossy,
}

/// Which hydrogens the model shows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Hydrogens {
    #[default]
    All,
    /// Only hydrogens on N, O or S.
    Polar,
    None,
}

/// What the model is drawn on.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Background {
    /// The paper.
    #[default]
    None,
    /// A soft grey rounded card filling the frame.
    Card,
}

/// The force field that minimized the conformer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ForceField {
    Mmff94,
    #[default]
    Mmff94s,
    Uff,
}

/// A drawing atom's conformer position in ångström.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Placed {
    pub id: u64,
    pub xyz: [f32; 3],
}

/// A force-field hydrogen that is not a drawing atom, bonded to `parent`.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Hydrogen {
    pub parent: u64,
    pub xyz: [f32; 3],
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Model3d {
    pub display: Display,
    /// Every atom of one connected molecule.
    pub atoms: Vec<Placed>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub hydrogens: Vec<Hydrogen>,
    /// View rotation as a unit quaternion `[w, x, y, z]`.
    pub rotation: [f32; 4],
    /// Frame centre minus the molecule's 2D centre, in drawing units.
    pub offset: Point,
    /// Reference side in drawing units: the model's farthest ball edge from
    /// its centre reaches 44% of it, which sets the drawing scale.
    pub size: f32,
    #[serde(default)]
    pub style: Style,
    #[serde(default)]
    pub look: Look,
    #[serde(default = "depth_fade")]
    pub depth_fade: f32,
    #[serde(default = "perspective")]
    pub perspective: f32,
    #[serde(default)]
    pub show_hydrogens: Hydrogens,
    #[serde(default)]
    pub background: Background,
    pub force_field: ForceField,
    #[serde(default, skip_serializing_if = "is_original")]
    pub layout: Layout,
    /// The layout as drawn, kept while the drawing is flattened from the model
    /// so that choosing the original drawing brings it back.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub drawn: Option<Drawn>,
    /// Progress of the 2D → 3D morph in 0..1, set only on the canvas's
    /// drawing copy while it plays; never saved.
    #[serde(skip)]
    pub morph: Option<f32>,
}
fn is_original(layout: &Layout) -> bool {
    *layout == Layout::Original
}
fn depth_fade() -> f32 {
    0.35
}

/// A bond's atoms, stereo, stereo atoms and stereo authority.
pub type DrawnBond = (u64, u64, Option<String>, Vec<u64>, bool);

/// What flattening a molecule changes: each atom's place, depth and stored
/// stereo, each bond's stored stereo, and the molecule's depth appearance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Drawn {
    pub atoms: Vec<(u64, Point, f32, Option<AtomStereo>)>,
    pub bonds: Vec<DrawnBond>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depth_appearance: Vec<Scope>,
    /// The centre of the molecule's 2D atoms when this layout was kept, so
    /// that the layout comes back wherever the molecule has been moved,
    /// transformed or pasted since, centred on it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub at: Option<Point>,
}

impl Drawn {
    /// The layout of `model`'s molecule as it is now.
    pub fn of(doc: &Document, model: &Model3d) -> Self {
        let ids = model.ids();
        let own: HashMap<_, _> = ids.iter().map(|id| (*id, *id)).collect();
        Self {
            atoms: doc
                .atoms
                .iter()
                .filter(|a| ids.contains(&a.id))
                .map(|a| (a.id, a.position, a.depth, a.stereo.clone()))
                .collect(),
            bonds: doc
                .bonds
                .iter()
                .filter(|b| ids.contains(&b.a))
                .map(|b| {
                    let stereo = b.stereo.clone();
                    (
                        b.a,
                        b.b,
                        stereo,
                        b.stereo_atoms.clone(),
                        b.stereo_authoritative,
                    )
                })
                .collect(),
            depth_appearance: crate::depth_appearance::remap(&doc.depth_appearance, &own),
            at: layout_center(doc, model),
        }
    }

    /// This layout, kept while `model`'s molecule is drawn as it is now.
    pub fn kept(self, doc: &Document, model: &Model3d) -> Self {
        Self {
            at: layout_center(doc, model),
            ..self
        }
    }

    /// Put this layout back on the drawing, moved as far as the molecule
    /// moved since it was kept.
    fn restore(self, doc: &mut Document, model: &Model3d) {
        let (dx, dy) = match (self.at, layout_center(doc, model)) {
            (Some(at), Some(now)) => (now.x - at.x, now.y - at.y),
            _ => (0., 0.),
        };
        let ids: Vec<_> = self.atoms.iter().map(|a| a.0).collect();
        for (id, position, depth, stereo) in self.atoms {
            if let Some(atom) = doc.atom_mut(id) {
                let position = position.offset(dx, dy);
                (atom.position, atom.depth, atom.stereo) = (position, depth, stereo);
            }
        }
        for (a, b, stereo, atoms, authoritative) in self.bonds {
            if let Some(bond) = doc.bonds.iter_mut().find(|x| x.a == a && x.b == b) {
                (bond.stereo, bond.stereo_atoms) = (stereo, atoms);
                bond.stereo_authoritative = authoritative;
            }
        }
        crate::depth_appearance::clear(doc, &ids);
        doc.depth_appearance.extend(self.depth_appearance);
    }

    fn remap(&mut self, mapping: &HashMap<u64, u64>) -> Option<()> {
        let map = |id: &mut u64| {
            *id = *mapping.get(id)?;
            Some(())
        };
        for (id, _, _, stereo) in &mut self.atoms {
            map(id)?;
            for n in stereo.iter_mut().flat_map(|s| &mut s.neighbors) {
                map(n)?;
            }
        }
        for (a, b, _, atoms, _) in &mut self.bonds {
            map(a)?;
            map(b)?;
            atoms.iter_mut().try_for_each(map)?;
        }
        self.depth_appearance = crate::depth_appearance::remap(&self.depth_appearance, mapping);
        Some(())
    }
}
fn perspective() -> f32 {
    0.2
}

const MAX_ATOMS: usize = 100_000;
const MAX_COORDINATE: f32 = 10_000.;
const GREY: [u8; 3] = [0x99; 3];
const BOND_RADIUS: f32 = 0.085;
const STICK_RADIUS: f32 = 0.16;
const WIRE_RADIUS: f32 = 0.02;
const CARD: [u8; 3] = [0xf4, 0xf6, 0xf8];
const CARD_EDGE: [u8; 3] = [0xe3, 0xe7, 0xea];
/// The flat look's outline on a light canvas, and the soft light rim that
/// replaces it on a dark one: colour and opacity.
const OUTLINE: ([u8; 3], f32) = ([30, 34, 38], 0.85);
const RIM: ([u8; 3], f32) = ([225, 232, 238], 0.55);
/// The radial gradient's inner circle, as a fraction of the ball radius.
pub const FOCAL_RADIUS: f32 = 0.05;

/// Hydrogen where the theme greys it like carbon, kept lighter than carbon,
/// on a light canvas and on a dark one.
const HYDROGEN: [u8; 3] = [0xee, 0xf0, 0xf2];
const HYDROGEN_DARK: [u8; 3] = [0xd9, 0xde, 0xe3];

/// An element's model colour: the drawing's element swatch for its theme and
/// canvas, user themes included, or Presentation's where the theme has none
/// (Publication), else the prototype's table. Where the theme greys hydrogen
/// like carbon (Presentation, Pastel), hydrogen is a light grey instead.
fn element_color(doc: &Document, symbol: &str) -> [u8; 3] {
    let canvas = doc.canvas_theme;
    let swatch = |element: &str| {
        crate::canvas_theme::element_swatch(doc, element, canvas).or_else(|| {
            crate::canvas_theme::ColorTheme::Presentation.element_swatch(element, canvas)
        })
    };
    match swatch(symbol) {
        Some(rgb) if symbol == "H" && swatch("C") == Some(rgb) => {
            if canvas.is_dark() {
                HYDROGEN_DARK
            } else {
                HYDROGEN
            }
        }
        Some(rgb) => rgb,
        None => element(symbol).0,
    }
}

/// Prototype colour and ball & stick radius (Å). Elements it lacks use its
/// grey with radii in proportion to the others.
fn element(symbol: &str) -> ([u8; 3], f32) {
    match symbol {
        "H" => ([0xee, 0xf0, 0xf2], 0.23),
        "C" => ([0x8d, 0x94, 0x9b], 0.33),
        "N" => ([0x3f, 0x6f, 0xd6], 0.32),
        "O" => ([0xe0, 0x48, 0x3d], 0.31),
        "S" => ([0xe3, 0xb5, 0x2e], 0.40),
        "F" => ([0x5f, 0xbf, 0x4f], 0.30),
        "Cl" => ([0x3f, 0xae, 0x4a], 0.40),
        "Br" => ([0x9b, 0x4a, 0x2c], 0.44),
        "P" => ([0xef, 0x8a, 0x2f], 0.40),
        "B" => (GREY, 0.30),
        "Si" => (GREY, 0.40),
        "As" | "Se" => (GREY, 0.42),
        "I" => (GREY, 0.48),
        _ => (GREY, 0.35),
    }
}

/// Van der Waals radius (Å) for space-filling: the prototype's, then Bondi's.
fn van_der_waals(symbol: &str) -> f32 {
    match symbol {
        "H" => 1.10,
        "C" => 1.70,
        "N" => 1.55,
        "O" => 1.52,
        "S" | "P" => 1.80,
        "F" => 1.47,
        "Cl" => 1.75,
        "Br" | "As" => 1.85,
        "B" => 1.92,
        "Si" => 2.10,
        "Se" => 1.90,
        "I" => 1.98,
        _ => 1.80,
    }
}

impl Model3d {
    /// A model of `atoms` (one whole molecule of `doc`) shown in 2D, facing
    /// its broadest side and drawn at the drawing's bond-length scale.
    pub fn new(
        doc: &Document,
        atoms: Vec<Placed>,
        hydrogens: Vec<Hydrogen>,
        force_field: ForceField,
    ) -> Self {
        let mut model = Self {
            display: Display::TwoD,
            atoms,
            hydrogens,
            rotation: [1., 0., 0., 0.],
            offset: Point::default(),
            size: 1.,
            style: Style::default(),
            look: Look::default(),
            depth_fade: depth_fade(),
            perspective: perspective(),
            show_hydrogens: Hydrogens::default(),
            background: Background::default(),
            force_field,
            layout: Layout::Original,
            drawn: None,
            morph: None,
        };
        model.rotation = model.initial_view(doc);
        let points = model.points(doc).unwrap_or_default();
        // The model's ångström scale matches the drawing: 1.5 Å per bond length.
        let units = doc.drawing_style.bond_length_world / 1.5;
        model.size =
            (reach(&points, heavy_center(&points), Style::BallStick) * units / 0.44).max(1.);
        model
    }

    /// The opening view, facing the broadest side; double-click restores it.
    pub fn initial_view(&self, doc: &Document) -> [f32; 4] {
        let points = self.points(doc).unwrap_or_default();
        let center = heavy_center(&points);
        let heavy: Vec<_> = points
            .iter()
            .filter(|(symbol, _)| *symbol != "H")
            .map(|(_, p)| sub(*p, center))
            .collect();
        initial_rotation(&heavy)
    }

    pub fn contains(&self, id: u64) -> bool {
        self.atoms.iter().any(|a| a.id == id)
    }

    fn ids(&self) -> HashSet<u64> {
        self.atoms.iter().map(|a| a.id).collect()
    }

    /// Element and position of each atom, then each force-field hydrogen.
    fn points<'a>(&self, doc: &'a Document) -> Option<Vec<(&'a str, [f32; 3])>> {
        let mut points = self
            .atoms
            .iter()
            .map(|p| Some((doc.atom(p.id)?.element.as_str(), p.xyz)))
            .collect::<Option<Vec<_>>>()?;
        points.extend(self.hydrogens.iter().map(|h| ("H", h.xyz)));
        Some(points)
    }

    /// Whether each of [`Self::points`] is drawn with these hydrogens shown.
    fn shown(&self, doc: &Document) -> Vec<bool> {
        let polar = |id: u64| {
            doc.atom(id)
                .is_some_and(|a| matches!(a.element.as_str(), "N" | "O" | "S"))
        };
        let keep = |hydrogen: bool, on_polar: &dyn Fn() -> bool| {
            !hydrogen
                || match self.show_hydrogens {
                    Hydrogens::All => true,
                    Hydrogens::Polar => on_polar(),
                    Hydrogens::None => false,
                }
        };
        let atoms = self.atoms.iter().map(|p| {
            let hydrogen = doc.atom(p.id).is_some_and(|a| a.element == "H");
            keep(hydrogen, &|| {
                doc.bonds
                    .iter()
                    .any(|b| (b.a == p.id && polar(b.b)) || (b.b == p.id && polar(b.a)))
            })
        });
        let hydrogens = self
            .hydrogens
            .iter()
            .map(|h| keep(true, &|| polar(h.parent)));
        let shown: Vec<bool> = atoms.chain(hydrogens).collect();
        // A molecule of hydrogens alone stays visible and selectable.
        if shown.contains(&true) {
            shown
        } else {
            vec![true; shown.len()]
        }
    }

    /// Map every ID; `None` when one is missing.
    pub fn remap(&mut self, mapping: &HashMap<u64, u64>) -> Option<()> {
        for atom in &mut self.atoms {
            atom.id = *mapping.get(&atom.id)?;
        }
        for h in &mut self.hydrogens {
            h.parent = *mapping.get(&h.parent)?;
        }
        if let Some(drawn) = &mut self.drawn {
            drawn.remap(mapping)?;
        }
        Some(())
    }
}

/// The model showing this atom's molecule, in any display.
pub fn model_of(doc: &Document, id: u64) -> Option<&Model3d> {
    doc.models.iter().find(|m| m.contains(id))
}

/// Whether this atom is drawn as part of a 3D model instead of in 2D.
pub fn hides(doc: &Document, id: u64) -> bool {
    doc.models
        .iter()
        .any(|m| m.display == Display::ThreeD && m.contains(id))
}

impl Document {
    /// [`Self::atom_visible`] and not shown as a 3D model: the atoms that
    /// picking, selection regions, markers and erasing can reach.
    pub fn atom_drawn(&self, id: u64) -> bool {
        self.atom_visible(id) && !hides(self, id)
    }
    pub fn bond_drawn(&self, a: u64, b: u64) -> bool {
        self.bond_visible(a, b) && !hides(self, a) && !hides(self, b)
    }
}

/// The 2D drawing without the atoms shown as 3D models, for painting and for
/// the rear-opacity paint the scene is built with.
pub fn drawn(doc: &Document) -> std::borrow::Cow<'_, Document> {
    let hidden: HashSet<_> = doc
        .models
        .iter()
        .filter(|m| m.display == Display::ThreeD)
        .flat_map(Model3d::ids)
        .collect();
    if hidden.is_empty() {
        return std::borrow::Cow::Borrowed(doc);
    }
    let mut flat = doc.clone();
    flat.atoms.retain(|a| !hidden.contains(&a.id));
    flat.bonds
        .retain(|b| !hidden.contains(&b.a) && !hidden.contains(&b.b));
    flat.abbreviations.retain(|g| !hidden.contains(&g.anchor));
    flat.models.clear();
    // Captions stay; only their links to the hidden atoms go.
    flat.molecule_names.clear();
    crate::projection::prune_centroids(&mut flat);
    crate::ring_fills::prune(&mut flat);
    crate::depth_appearance::prune(&mut flat);
    crate::generic::prune(&mut flat);
    crate::reactions::prune(&mut flat);
    crate::arrow_anchors::reconcile(&mut flat);
    std::borrow::Cow::Owned(flat)
}

/// The model's projection: rotated ångström points about the heavy-atom
/// centre, drawn around `origin` at `scale` drawing units per ångström.
struct View<'a> {
    symbols: Vec<&'a str>,
    rotated: Vec<[f32; 3]>,
    /// Whether each point is drawn, see [`Model3d::shown`].
    shown: Vec<bool>,
    style: Style,
    reach: f32,
    origin: Point,
    scale: f32,
    camera: f32,
    perspective: f32,
    /// Wireframe line width in drawing units.
    wire: f32,
}
impl View<'_> {
    fn new<'a>(doc: &'a Document, model: &Model3d) -> Option<View<'a>> {
        let center = layout_center(doc, model)?;
        let points = model.points(doc)?;
        let shown = model.shown(doc);
        let c = heavy_center(&points);
        let drawn: Vec<_> = points
            .iter()
            .zip(&shown)
            .filter(|(_, shown)| **shown)
            .map(|(p, _)| *p)
            .collect();
        let reach = reach(&drawn, c, model.style);
        Some(View {
            rotated: points
                .iter()
                .map(|(_, p)| rotate(model.rotation, sub(*p, c)))
                .collect(),
            symbols: points.into_iter().map(|(s, _)| s).collect(),
            shown,
            style: model.style,
            reach,
            origin: center.offset(model.offset.x, model.offset.y),
            scale: 0.44 * model.size / reach,
            camera: reach * (1.2 + 9. * (1. - model.perspective)),
            perspective: model.perspective,
            wire: (model.size / 190.).max(1.6),
        })
    }
    /// Drawing point and perspective factor of a rotated point.
    fn project(&self, p: [f32; 3]) -> (Point, f32) {
        let f = if self.perspective > 0. {
            self.camera / (self.camera - p[2])
        } else {
            1.
        };
        (
            Point::new(
                self.origin.x + p[0] * self.scale * f,
                self.origin.y - p[1] * self.scale * f,
            ),
            f,
        )
    }
    /// Index, projected centre and radius of each drawn ball.
    fn balls(&self) -> impl Iterator<Item = (usize, Point, f32)> + '_ {
        self.symbols
            .iter()
            .zip(&self.rotated)
            .enumerate()
            .filter(|(i, _)| self.shown.get(*i) == Some(&true))
            .map(|(i, (s, p))| {
                let (q, f) = self.project(*p);
                (i, q, self.style.atom_radius(s) * self.scale * f)
            })
    }
    /// Move each point a fraction `e` of the way from its flat start: a drawing
    /// atom's place in the 2D drawing, wherever the frame is, or for a
    /// force-field hydrogen its parent's.
    fn morph(&mut self, doc: &Document, model: &Model3d, e: f32) {
        let origin = self.origin;
        let flat = |id| {
            doc.atom(id).map_or([0.; 3], |a| {
                [
                    (a.position.x - origin.x) / self.scale,
                    -(a.position.y - origin.y) / self.scale,
                    0.,
                ]
            })
        };
        let starts = model
            .atoms
            .iter()
            .map(|a| a.id)
            .chain(model.hydrogens.iter().map(|h| h.parent))
            .map(flat);
        for (p, start) in self.rotated.iter_mut().zip(starts) {
            *p = mix(start, *p, e);
        }
    }
    /// A square about the origin holding the model's bounding sphere (its
    /// farthest drawn ball edge from the centre) in any rotation, perspective
    /// included, so the frame keeps its size and place while the model turns.
    fn bounds(&self) -> (Point, Point) {
        // Seen from the camera, a sphere of radius r spans r / √(1 − (r/c)²)
        // on the plane through its centre; the camera is always beyond reach.
        let spread = if self.perspective > 0. {
            let k = self.reach / self.camera;
            1. / (1. - k * k).sqrt()
        } else {
            1.
        };
        // A wireframe atom without shown bonds is a dot as wide as two lines.
        let dot = if self.style == Style::Wireframe {
            self.wire
        } else {
            0.
        };
        let half = self.reach * self.scale * spread + dot;
        (
            self.origin.offset(-half, -half),
            self.origin.offset(half, half),
        )
    }
}

/// The frame's top-left and bottom-right corners in drawing units: a square
/// that holds the model in every rotation, or `None` while an atom is missing.
pub fn frame(doc: &Document, model: &Model3d) -> Option<(Point, Point)> {
    Some(View::new(doc, model)?.bounds())
}

/// The frame's corners, clockwise from the top left.
pub fn frame_corners(doc: &Document, model: &Model3d) -> Option<[Point; 4]> {
    let (lo, hi) = frame(doc, model)?;
    Some([lo, Point::new(hi.x, lo.y), hi, Point::new(lo.x, hi.y)])
}

/// The centre of the molecule's 2D atoms, which the frame's offset is from.
fn layout_center(doc: &Document, model: &Model3d) -> Option<Point> {
    let (lo, hi) = layout_bounds(doc, model)?;
    Some(Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.))
}

fn layout_bounds(doc: &Document, model: &Model3d) -> Option<(Point, Point)> {
    let mut points = model
        .atoms
        .iter()
        .map(|p| doc.atom(p.id).map(|a| a.position));
    let first = points.next()??;
    points.try_fold((first, first), |(lo, hi), p| {
        let p = p?;
        Some((
            Point::new(lo.x.min(p.x), lo.y.min(p.y)),
            Point::new(hi.x.max(p.x), hi.y.max(p.y)),
        ))
    })
}

/// Show the molecule of model `index` as `display`; returns whether it changed.
/// Beside the drawing the frame sits one bond length right of the atoms.
pub fn set_display(doc: &mut Document, index: usize, display: Display) -> bool {
    let Some(model) = doc.models.get(index) else {
        return false;
    };
    let Some((_, hi)) = layout_bounds(doc, model) else {
        return false;
    };
    // Leaving the place beside the drawing centres the frame on it again;
    // otherwise the frame stays where it is.
    let offset = match display {
        Display::Both => {
            let centered = Model3d {
                offset: Point::default(),
                ..model.clone()
            };
            let Some((left, _)) = frame(doc, &centered) else {
                return false;
            };
            Point::new(hi.x + doc.drawing_style.bond_length_world - left.x, 0.)
        }
        _ if model.display == Display::Both => Point::default(),
        Display::TwoD | Display::ThreeD => model.offset,
    };
    let Some(model) = doc.models.get_mut(index) else {
        return false;
    };
    let changed = model.display != display || model.offset != offset;
    model.display = display;
    model.offset = offset;
    if changed {
        doc.require_version(crate::document::MODELS_3D);
    }
    changed
}

/// Show model `index`'s molecule in its drawn layout again, if it was
/// flattened; returns whether the drawing changed. The frame stays put.
pub fn restore_drawing(doc: &mut Document, index: usize) -> bool {
    let origin = doc.models.get(index).and_then(|m| frame_origin(doc, m));
    let Some(model) = doc.models.get_mut(index) else {
        return false;
    };
    let Some(drawn) = model.drawn.take() else {
        return false;
    };
    let model = model.clone();
    drawn.restore(doc, &model);
    if let Some(origin) = origin {
        keep_place(doc, index, origin);
    }
    true
}

/// The centre of the model's frame, in drawing units.
pub fn frame_origin(doc: &Document, model: &Model3d) -> Option<Point> {
    let center = layout_center(doc, model)?;
    Some(center.offset(model.offset.x, model.offset.y))
}

/// Keep model `index`'s frame centred on `origin` after its 2D atoms moved,
/// as when flattening or restoring the drawing; beside the drawing it is
/// placed beside the moved drawing instead.
pub fn keep_place(doc: &mut Document, index: usize, origin: Point) {
    let Some(model) = doc.models.get(index) else {
        return;
    };
    if model.display == Display::Both {
        set_display(doc, index, Display::Both);
        return;
    }
    let Some(center) = layout_center(doc, model) else {
        return;
    };
    if let Some(model) = doc.models.get_mut(index) {
        model.offset = Point::new(origin.x - center.x, origin.y - center.y);
    }
}

/// The selected molecule: the whole connected component of the selected atoms
/// when they all belong to one, in document order.
pub fn selected_molecule(doc: &Document, selected: &[u64]) -> Option<Vec<u64>> {
    let atoms: Vec<_> = doc
        .expand_abbreviation_selection(selected)
        .into_iter()
        .filter(|id| doc.atom(*id).is_some())
        .collect();
    let component = crate::molecule_names::component(doc, *atoms.first()?);
    atoms
        .iter()
        .all(|id| component.contains(id))
        .then_some(component)
}

/// [`selected_molecule`] when the selection is all of it and nothing else
/// but its name.
pub fn whole_molecule(doc: &Document, selected: &[u64]) -> Option<Vec<u64>> {
    let atoms = selected_molecule(doc, selected)?;
    let named = crate::molecule_names::include_annotations(doc, &atoms);
    let expanded = doc.expand_abbreviation_selection(selected);
    (expanded.iter().all(|id| named.contains(id)) && atoms.iter().all(|id| expanded.contains(id)))
        .then_some(atoms)
}

/// The atoms of the models whose frame contains `p`, topmost first.
pub fn frame_hit(doc: &Document, p: Point) -> Option<Vec<u64>> {
    model_at(doc, p, 0.).map(|m| m.atoms.iter().map(|a| a.id).collect())
}

/// The topmost shown model whose frame, shrunk by `inset` on every side,
/// contains `p`.
pub fn model_at(doc: &Document, p: Point, inset: f32) -> Option<&Model3d> {
    doc.models
        .iter()
        .rev()
        .filter(|m| m.display != Display::TwoD)
        .find(|m| {
            frame(doc, m).is_some_and(|(lo, hi)| {
                (lo.x + inset..=hi.x - inset).contains(&p.x)
                    && (lo.y + inset..=hi.y - inset).contains(&p.y)
            })
        })
}

/// Whether the selection includes a molecule whose model is shown (3D or
/// both), which the selection box must not scale as a drawing.
pub fn shows_3d(doc: &Document, ids: &[u64]) -> bool {
    doc.models
        .iter()
        .any(|m| m.display != Display::TwoD && ids.iter().any(|id| m.contains(*id)))
}

/// Whether every selected object is an atom of a shown model, so that corner
/// handles resize the models' frames with [`resize`].
pub fn only_shown(doc: &Document, ids: &[u64]) -> bool {
    !ids.is_empty()
        && ids.iter().all(|id| {
            doc.models
                .iter()
                .any(|m| m.display != Display::TwoD && m.contains(*id))
        })
}

/// Scale the frames of the shown models among `ids` by `scale` about `pivot`,
/// as a corner handle does. The 2D drawings and the views stay.
pub fn resize(doc: &mut Document, ids: &[u64], pivot: Point, scale: f32) {
    if !scale.is_finite() || scale <= 0. {
        return;
    }
    for index in 0..doc.models.len() {
        let Some(model) = doc.models.get(index) else {
            continue;
        };
        if model.display == Display::TwoD || !ids.iter().any(|id| model.contains(*id)) {
            continue;
        }
        let Some(center) = layout_center(doc, model) else {
            continue;
        };
        // Every frame point is the origin plus a multiple of the size.
        let size = (model.size * scale).clamp(1., 1e5);
        let s = size / model.size;
        let origin = center.offset(model.offset.x, model.offset.y);
        let moved = Point::new(
            pivot.x + (origin.x - pivot.x) * s,
            pivot.y + (origin.y - pivot.y) * s,
        );
        if let Some(model) = doc.models.get_mut(index) {
            model.size = size;
            model.offset = Point::new(moved.x - center.x, moved.y - center.y);
        }
    }
}

/// Turn the model of atom `id` to the unit quaternion `rotation`; returns
/// whether it changed.
pub fn set_rotation(doc: &mut Document, id: u64, rotation: [f32; 4]) -> bool {
    let norm = rotation.iter().map(|v| v * v).sum::<f32>();
    let Some(model) = doc.models.iter_mut().find(|m| m.contains(id)) else {
        return false;
    };
    if !rotation.iter().all(|v| v.is_finite()) || (norm - 1.).abs() > 1e-3 {
        return false;
    }
    let changed = model.rotation != rotation;
    model.rotation = rotation;
    changed
}

/// `rotation` turned by a drag of `dx`, `dy` screen pixels, 0.01 radian per
/// pixel as in the prototype: about the screen's vertical and horizontal axes,
/// or with `roll` about the view axis.
pub fn turn(rotation: [f32; 4], dx: f32, dy: f32, roll: bool) -> [f32; 4] {
    let axis = |angle: f32, i: usize| {
        let (s, c) = (angle / 2.).sin_cos();
        let mut q = [c, 0., 0., 0.];
        if let Some(v) = q.get_mut(i + 1) {
            *v = s;
        }
        q
    };
    let step = if roll {
        axis(-dx * 0.01, 2)
    } else {
        compose(axis(dx * 0.01, 1), axis(dy * 0.01, 0))
    };
    let q = compose(step, rotation);
    let n = q.iter().map(|v| v * v).sum::<f32>().sqrt();
    if !n.is_finite() || n < 1e-6 {
        return rotation;
    }
    q.map(|v| v / n)
}

/// Turn each model shown only in 3D that holds one of `ids` by a tilt of
/// `degrees` about the screen's horizontal (`around_x`) or vertical axis, in
/// the same sense as [`crate::projection::tilt`] turns a 2D drawing and as a
/// drag of [`turn`] does: a positive X tilt sends the top away, a positive Y
/// tilt the right side.
pub fn tilt(doc: &mut Document, ids: &[u64], degrees: f32, around_x: bool) {
    // `turn` takes screen pixels of 0.01 radian; screen y points down.
    let pixels = degrees.to_radians() * 100.;
    let (dx, dy) = if around_x {
        (0., -pixels)
    } else {
        (pixels, 0.)
    };
    for model in doc
        .models
        .iter_mut()
        .filter(|m| m.display == Display::ThreeD && ids.iter().any(|id| m.contains(*id)))
    {
        model.rotation = turn(model.rotation, dx, dy, false);
    }
}

/// The rotation `b`, then `a`.
fn compose(a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
    let [aw, ax, ay, az] = a;
    let [bw, bx, by, bz] = b;
    [
        aw * bw - ax * bx - ay * by - az * bz,
        aw * bx + ax * bw + ay * bz - az * by,
        aw * by - ax * bz + ay * bw + az * bx,
        aw * bz + ax * by - ay * bx + az * bw,
    ]
}

pub fn validate(doc: &Document) -> Result<(), String> {
    if doc.models.is_empty() {
        return Ok(());
    }
    // Raw serde paths skip the version probe; refuse models in older files.
    if doc.version < crate::document::MODELS_3D {
        return Err(format!(
            "3D models need document version {} or later",
            crate::document::MODELS_3D
        ));
    }
    let finite = |p: &[f32; 3]| p.iter().all(|v| v.is_finite() && v.abs() <= MAX_COORDINATE);
    let mut owned = HashSet::new();
    for m in &doc.models {
        let ids = m.ids();
        let norm = m.rotation.iter().map(|v| v * v).sum::<f32>();
        if m.atoms.is_empty()
            || m.atoms.len() > MAX_ATOMS
            || m.hydrogens.len() > m.atoms.len().saturating_mul(4)
            || ids.len() != m.atoms.len()
            || m.atoms
                .iter()
                .any(|a| !owned.insert(a.id) || !finite(&a.xyz))
            || m.hydrogens
                .iter()
                .any(|h| !ids.contains(&h.parent) || !finite(&h.xyz))
            || !m.rotation.iter().all(|v| v.is_finite())
            || (norm - 1.).abs() > 1e-3
            || !m.offset.is_finite()
            || m.offset.x.abs() > 1e6
            || m.offset.y.abs() > 1e6
            || !(1. ..=1e5).contains(&m.size)
            || !(0. ..=1.).contains(&m.depth_fade)
            || !(0. ..=1.).contains(&m.perspective)
            || m.drawn.as_ref().is_some_and(|d| {
                d.atoms.len() != ids.len()
                    || d.at.is_some_and(|p| !p.is_finite())
                    || d.atoms
                        .iter()
                        .any(|(id, p, z, _)| !ids.contains(id) || !p.is_finite() || !z.is_finite())
            })
        {
            return Err("Invalid 3D model".into());
        }
    }
    Ok(())
}

/// Drop models that lost an atom. Validation tolerates them, so edits that
/// remove atoms before reconciling stay valid; [`reconcile`] drops them too.
pub fn prune(doc: &mut Document) {
    let present: HashSet<_> = doc.atoms.iter().map(|a| a.id).collect();
    doc.models
        .retain(|m| m.atoms.iter().all(|a| present.contains(&a.id)));
}

/// Whether the model's atoms are exactly one whole connected molecule.
fn whole(doc: &Document, model: &Model3d) -> bool {
    let ids = model.ids();
    let component = model
        .atoms
        .first()
        .map(|a| crate::molecule_names::component(doc, a.id))
        .unwrap_or_default();
    component.len() == ids.len() && component.iter().all(|id| ids.contains(id))
}

/// Drop a loaded file's models that are not one whole molecule. Unlike
/// [`prune`], nothing is mid-edit here.
pub fn retain_whole(doc: &mut Document) {
    let models = std::mem::take(&mut doc.models);
    doc.models = models.into_iter().filter(|m| whole(doc, m)).collect();
}

/// Called in the same transaction as the edit. A model whose atoms are no
/// longer exactly one molecule, or whose molecule changed chemically since
/// `before` (element, charge, bonds, stereo, …; not its 2D layout), is dropped.
/// Atom IDs can be reused and equivalent atoms swapped without changing a
/// canonical SMILES, so the check compares atoms and bonds by ID.
pub fn reconcile(doc: &mut Document, before: &Document) {
    if doc.models.is_empty() {
        return;
    }
    let models = std::mem::take(&mut doc.models);
    doc.models = models
        .into_iter()
        .filter(|m| {
            let ids = m.ids();
            let kept = before.models.iter().any(|old| old.ids() == ids);
            whole(doc, m)
                && (!kept
                    || chemistry(before, &ids) == chemistry(doc, &ids)
                        && stereo(before, &ids) == stereo(doc, &ids))
        })
        .collect();
}

/// Atom chiral tags, then each bond's stereo, stereo atoms and unknown flag.
type Stereo = (Vec<u8>, Vec<(u8, Vec<usize>, bool)>);
/// Stereo as perceived from the drawing, which reads E/Z and R/S from 2D
/// positions and wedges as well as stored fields; `None` when the molecule
/// cannot be perceived.
fn stereo(doc: &Document, ids: &HashSet<u64>) -> Option<Stereo> {
    let atoms: Vec<_> = doc
        .atoms
        .iter()
        .map(|a| a.id)
        .filter(|id| ids.contains(id))
        .collect();
    let part = crate::editing::analysis_document(doc, &atoms);
    let metadata = crate::chemistry::document::prepare(&part)
        .ok()?
        .state
        .metadata;
    Some((
        metadata.atoms.iter().map(|a| a.chiral_tag).collect(),
        metadata
            .bonds
            .into_iter()
            .map(|b| (b.stereo, b.stereo_atoms, b.unknown_stereo))
            .collect(),
    ))
}

/// The chemistry of these atoms and their bonds, without presentation. Bond
/// appearance and stored stereo count only through the stereo they mark, see
/// [`stereo`]: flattening the view stores the drawn stereo without changing it.
fn chemistry(doc: &Document, ids: &HashSet<u64>) -> (Vec<Atom>, Vec<Bond>) {
    let atoms = doc
        .atoms
        .iter()
        .filter(|a| ids.contains(&a.id))
        .map(|a| Atom {
            display: Default::default(),
            cip_label: None,
            position: Point::default(),
            depth: 0.,
            marks: vec![],
            mark_serial: 0,
            map_num: 0,
            label_h: 0,
            text_style: None,
            stereo: None,
            ..a.clone()
        })
        .collect();
    let bonds = doc
        .bonds
        .iter()
        .filter(|b| ids.contains(&b.a) || ids.contains(&b.b))
        .map(|b| Bond {
            highlight: None,
            ring_arc: false,
            z_order: 0,
            indicator: Default::default(),
            cip_label: None,
            double_position: Default::default(),
            secondary_display: None,
            color: Default::default(),
            display: String::new(),
            stereo: None,
            stereo_atoms: vec![],
            stereo_authoritative: false,
            ..b.clone()
        })
        .collect();
    (atoms, bonds)
}

fn zip3<T: Copy>(a: [T; 3], b: [T; 3], f: impl Fn(T, T) -> T) -> [T; 3] {
    [f(a[0], b[0]), f(a[1], b[1]), f(a[2], b[2])]
}
fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    zip3(a, b, |a, b| a - b)
}
fn length(p: [f32; 3]) -> f32 {
    (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt()
}

/// Centroid of the heavy atoms, or of all atoms without one.
fn heavy_center(points: &[(&str, [f32; 3])]) -> [f32; 3] {
    let heavy: Vec<_> = points.iter().filter(|(s, _)| *s != "H").collect();
    let chosen: Vec<_> = if heavy.is_empty() {
        points.iter().collect()
    } else {
        heavy
    };
    let n = chosen.len().max(1) as f32;
    chosen
        .iter()
        .fold([0.; 3], |c, (_, p)| zip3(c, *p, |c, p| c + p / n))
}

/// Farthest ball edge from `center` in `style`, in ångström.
fn reach(points: &[(&str, [f32; 3])], center: [f32; 3], style: Style) -> f32 {
    points
        .iter()
        .map(|(s, p)| length(sub(*p, center)) + style.atom_radius(s))
        .fold(0., f32::max)
        .max(0.1)
}

/// Rotate `v` by the unit quaternion `[w, x, y, z]`.
fn rotate(q: [f32; 4], v: [f32; 3]) -> [f32; 3] {
    let [w, x, y, z] = q;
    let dot = x * v[0] + y * v[1] + z * v[2];
    let cross = [
        y * v[2] - z * v[1],
        z * v[0] - x * v[2],
        x * v[1] - y * v[0],
    ];
    let uu = x * x + y * y + z * z;
    let along = zip3([x, y, z], v, |u, v| 2. * dot * u + (w * w - uu) * v);
    zip3(along, cross, |a, c| a + 2. * w * c)
}

/// The prototype's opening view of `points`, the centred heavy atoms: their two
/// principal axes become screen x and −y, tilted slightly so it reads as 3D.
/// The prototype's rows a0, −a1, a0 × a1 made a reflection; the third row is
/// now x × y, a proper rotation showing the same face. A linear or one-atom
/// molecule falls back to fixed axes.
pub fn initial_rotation(points: &[[f32; 3]]) -> [f32; 4] {
    type V = [f64; 3];
    let dot = |a: V, b: V| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let cross = |a: V, b: V| {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    };
    let mut c = [[0.; 3]; 3];
    for p in points {
        let p = p.map(f64::from);
        for (row, pi) in c.iter_mut().zip(p) {
            for (value, pj) in row.iter_mut().zip(p) {
                *value += pi * pj;
            }
        }
    }
    let trace = c[0][0] + c[1][1] + c[2][2];
    // Power iteration, kept orthogonal to `previous`.
    let principal = |start: V, previous: Option<V>| -> V {
        let deflate = |v: V| match previous {
            Some(a) => {
                let d = dot(v, a);
                zip3(v, a, |v, a| v - d * a)
            }
            None => v,
        };
        let normal = |v: V| {
            let n = dot(v, v).sqrt();
            (n > 1e-9 * trace.max(1e-300)).then(|| v.map(|x| x / n))
        };
        let mut v = start;
        for _ in 0..60 {
            match normal(deflate(c.map(|row| dot(row, v)))) {
                Some(next) if trace > 1e-9 => v = next,
                // Degenerate: the coordinate axis least along `previous`.
                _ => {
                    let unit =
                        |i: usize| -> V { std::array::from_fn(|j| f64::from(u8::from(i == j))) };
                    let v = (0..3)
                        .map(|i| deflate(unit(i)))
                        .max_by(|a, b| dot(*a, *a).total_cmp(&dot(*b, *b)))
                        .unwrap_or([1., 0., 0.]);
                    let n = dot(v, v).sqrt().max(1e-300);
                    return v.map(|x| x / n);
                }
            }
        }
        v
    };
    let x = principal([1., 0.3, 0.2], None);
    let y = principal([0.2, 1., 0.3], Some(x)).map(|v| -v);
    let basis = [x, y, cross(x, y)];
    let (sx, cx) = (-0.35_f64).sin_cos();
    let (sy, cy) = 0.3_f64.sin_cos();
    let rx = [[1., 0., 0.], [0., cx, -sx], [0., sx, cx]];
    let ry = [[cy, 0., sy], [0., 1., 0.], [-sy, 0., cy]];
    let mul = |a: [V; 3], b: [V; 3]| {
        let columns = [
            [b[0][0], b[1][0], b[2][0]],
            [b[0][1], b[1][1], b[2][1]],
            [b[0][2], b[1][2], b[2][2]],
        ];
        a.map(|row| columns.map(|column| dot(row, column)))
    };
    quaternion(mul(mul(rx, ry), basis))
}

/// The unit quaternion of a proper rotation matrix (rows), `[w, x, y, z]`.
fn quaternion(m: [[f64; 3]; 3]) -> [f32; 4] {
    let trace = m[0][0] + m[1][1] + m[2][2];
    let q = if trace > 0. {
        let s = (trace + 1.).sqrt() * 2.;
        [
            s / 4.,
            (m[2][1] - m[1][2]) / s,
            (m[0][2] - m[2][0]) / s,
            (m[1][0] - m[0][1]) / s,
        ]
    } else if m[0][0] > m[1][1] && m[0][0] > m[2][2] {
        let s = (1. + m[0][0] - m[1][1] - m[2][2]).sqrt() * 2.;
        [
            (m[2][1] - m[1][2]) / s,
            s / 4.,
            (m[0][1] + m[1][0]) / s,
            (m[0][2] + m[2][0]) / s,
        ]
    } else if m[1][1] > m[2][2] {
        let s = (1. + m[1][1] - m[0][0] - m[2][2]).sqrt() * 2.;
        [
            (m[0][2] - m[2][0]) / s,
            (m[0][1] + m[1][0]) / s,
            s / 4.,
            (m[1][2] + m[2][1]) / s,
        ]
    } else {
        let s = (1. + m[2][2] - m[0][0] - m[1][1]).sqrt() * 2.;
        [
            (m[1][0] - m[0][1]) / s,
            (m[0][2] + m[2][0]) / s,
            (m[1][2] + m[2][1]) / s,
            s / 4.,
        ]
    };
    let n = q.iter().map(|v| v * v).sum::<f64>().sqrt();
    q.map(|v| (v / n) as f32)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    zip3(a, b, |a, b| a + (b - a) * t)
}
fn rgb(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| v.clamp(0., 255.).round() as u8)
}
fn float(c: [u8; 3]) -> [f32; 3] {
    c.map(f32::from)
}
const WHITE: [f32; 3] = [255.; 3];
const BLACK: [f32; 3] = [0.; 3];

/// Primitives of every model not shown as 2D, each back to front, in exact
/// colours faded toward the paper or card. A port of the approved prototype.
pub fn primitives(doc: &Document) -> Vec<Primitive> {
    let mut out = vec![];
    for model in doc.models.iter().filter(|m| m.display != Display::TwoD) {
        push_model(doc, model, &mut out);
    }
    out
}

/// Each bond between two drawn points as a pair of indices into the model's
/// points; none in space-filling.
fn sticks(doc: &Document, model: &Model3d, shown: &[bool]) -> Option<Vec<(usize, usize)>> {
    if model.style == Style::SpaceFilling {
        return Some(vec![]);
    }
    let index: HashMap<_, _> = model
        .atoms
        .iter()
        .enumerate()
        .map(|(i, a)| (a.id, i))
        .collect();
    let mut bonds: Vec<(usize, usize)> = doc
        .bonds
        .iter()
        .filter_map(|b| Some((*index.get(&b.a)?, *index.get(&b.b)?)))
        .collect();
    for (i, h) in model.hydrogens.iter().enumerate() {
        bonds.push((*index.get(&h.parent)?, model.atoms.len() + i));
    }
    bonds.retain(|(a, b)| shown.get(*a) == Some(&true) && shown.get(*b) == Some(&true));
    Some(bonds)
}

impl View<'_> {
    /// The half-bond from point `s` toward point `o`: its projected start,
    /// the projected bond middle and the half width at each. It tapers from
    /// its start's perspective to one middle width shared by both halves, so
    /// they meet at the colour seam and a stick never outgrows its joint.
    fn half_bond(&self, s: usize, o: usize) -> Option<(Point, Point, f32, f32)> {
        let (ps, po) = (*self.rotated.get(s)?, *self.rotated.get(o)?);
        let (start, fs) = self.project(ps);
        let (middle, fm) = self.project(zip3(ps, po, |a, b| (a + b) / 2.));
        let radius = match self.style {
            Style::BallStick => BOND_RADIUS * self.scale,
            Style::Stick => STICK_RADIUS * self.scale,
            Style::Wireframe => return Some((start, middle, self.wire / 2., self.wire / 2.)),
            Style::SpaceFilling => 0.,
        };
        Some((start, middle, radius * fs, radius * fm))
    }

    /// Stick style's joints: each half-bond's outline at its atom, from its
    /// left edge (toward the next half-bond counterclockwise on screen) round
    /// to its right edge. Neighbouring halves share one border point on their
    /// bisector, so a joint of three bonds has three borders and nothing
    /// overlaps; a free side of 180° or more gets a round cap. Halves seen
    /// nearly end-on are left out: the joint covers them.
    fn joints(&self, halves: &[(usize, usize)]) -> HashMap<(usize, usize), Vec<Point>> {
        struct Half {
            key: (usize, usize),
            angle: f32,
            start: Point,
            radius: f32,
            length: f32,
        }
        let mut at: HashMap<usize, Vec<Half>> = HashMap::new();
        for &(s, o) in halves {
            let Some((start, end, radius, _)) = self.half_bond(s, o) else {
                continue;
            };
            let (dx, dy) = (end.x - start.x, end.y - start.y);
            let length = dx.hypot(dy);
            if length < radius * END_ON {
                continue;
            }
            at.entry(s).or_default().push(Half {
                key: (s, o),
                angle: dy.atan2(dx),
                start,
                radius,
                length,
            });
        }
        let tau = std::f32::consts::TAU;
        let half_pi = std::f32::consts::FRAC_PI_2;
        let mut joints = HashMap::new();
        for mut list in at.into_values() {
            list.sort_by(|a, b| a.angle.total_cmp(&b.angle));
            let n = list.len();
            for (i, h) in list.iter().enumerate() {
                let neighbour = |step: usize| list.get((i + step) % n).filter(|_| n > 1);
                // The gap to a neighbour, counterclockwise (`next`) or
                // clockwise (`previous`); 360° without one.
                let gap = |other: Option<&Half>, sign: f32| {
                    other.map_or(tau, |o| {
                        let g = sign * (o.angle - h.angle);
                        if g <= 0. { g + tau } else { g }
                    })
                };
                let point = |angle: f32, d: f32| h.start.offset(angle.cos() * d, angle.sin() * d);
                // From the stick's edge to the shared border, on one side,
                // and the shift that moves the border a little under the
                // neighbour: whichever of the two is drawn later covers it,
                // so no pale seam shows between them.
                let side = |other: Option<&Half>, sign: f32| -> (Vec<Point>, Point) {
                    let g = gap(other, sign);
                    let r = h.radius;
                    let border = h.angle + sign * g / 2.;
                    let mut points = if g < std::f32::consts::PI - 0.02 {
                        let reach = other.map_or(h.length, |o| h.length.min(o.length));
                        let miter = r / (g / 2.).sin();
                        let d = miter.min(MITER * r).min(reach.max(r));
                        if d < miter {
                            // A narrow gap or a short neighbour: the edge runs
                            // on straight toward where the edges would meet,
                            // then turns in to the shared border, so the stick
                            // keeps its width up to its middle.
                            let along = (r / (g / 2.).tan()).min(MITER * r).min(2. * h.length);
                            let (dx, dy) = (h.angle.cos(), h.angle.sin());
                            let side = h.angle + sign * half_pi;
                            let edge = h
                                .start
                                .offset(dx * along + side.cos() * r, dy * along + side.sin() * r);
                            vec![edge, point(border, d)]
                        } else {
                            vec![point(border, d)]
                        }
                    } else {
                        let from = h.angle + sign * half_pi;
                        let steps = ((border - from).abs() / ARC_STEP).ceil().max(1.) as usize;
                        (0..=steps)
                            .map(|k| point(from + (border - from) * k as f32 / steps as f32, r))
                            .collect()
                    };
                    let toward = border + sign * half_pi;
                    let shift = if other.is_some() {
                        Point::new(toward.cos() * r * OVERLAP, toward.sin() * r * OVERLAP)
                    } else {
                        Point::default()
                    };
                    if let Some(last) = points.last_mut() {
                        *last = last.offset(shift.x, shift.y);
                    }
                    (points, shift)
                };
                let (mut chain, left) = side(neighbour(1), 1.);
                let (mut right, shift) = side(neighbour(n - 1), -1.);
                chain.push(h.start.offset(left.x, left.y));
                chain.push(h.start.offset(shift.x, shift.y));
                right.reverse();
                chain.extend(right);
                joints.insert(h.key, chain);
            }
        }
        joints
    }
}

/// A stick half shorter on screen than this fraction of its half width is
/// seen end-on and left to its joint.
const END_ON: f32 = 0.25;
/// How far each half reaches past a shared border, in stick half widths.
const OVERLAP: f32 = 0.12;
/// The farthest a joint's shared border reaches, in stick half widths.
const MITER: f32 = 6.;
/// The angle per segment of a round cap.
const ARC_STEP: f32 = std::f32::consts::PI / 16.;

/// The model as drawn: each ball's centre and radius, and each half-bond as
/// its end points and half width, in drawing units. `None` while an atom is
/// missing.
#[allow(clippy::type_complexity)]
pub fn balls_and_sticks(
    doc: &Document,
    model: &Model3d,
) -> Option<(Vec<(Point, f32)>, Vec<(Point, Point, f32)>)> {
    let view = View::new(doc, model)?;
    let sticks = sticks(doc, model, &view.shown)?
        .into_iter()
        .flat_map(|(a, b)| [(a, b), (b, a)])
        .map(|(s, o)| {
            let (a, e, start, middle) = view.half_bond(s, o)?;
            Some((a, e, start.max(middle)))
        })
        .collect::<Option<_>>()?;
    let balls = view.balls().map(|(_, q, r)| (q, r)).collect();
    Some((balls, sticks))
}

fn push_model(doc: &Document, model: &Model3d, out: &mut Vec<Primitive>) -> Option<()> {
    let mut view = View::new(doc, model)?;
    // The 2D → 3D morph, eased as in the prototype; hydrogens fade in.
    let grown = model.morph.filter(|m| *m < 1.).map_or(1., |m| {
        let m = m.max(0.);
        let e = if m < 0.5 {
            4. * m * m * m
        } else {
            1. - (2. - 2. * m).powi(3) / 2.
        };
        view.morph(doc, model, e);
        e
    });
    let bonds = sticks(doc, model, &view.shown)?;
    let (rotated, reach, style) = (&view.rotated, view.reach, model.style);
    let paper = doc.canvas_theme.background();
    let dark = doc.canvas_theme.is_dark();
    // On a dark canvas the card is the paper lightened a little.
    let (card, card_edge) = if dark {
        let lighter = |t| rgb(mix(float(paper), WHITE, t));
        (lighter(0.08), lighter(0.16))
    } else {
        (CARD, CARD_EDGE)
    };
    let backdrop = match model.background {
        Background::None => paper,
        Background::Card => card,
    };
    let fade = |z: f32| model.depth_fade * ((reach - z) / (2. * reach)).clamp(0., 1.);
    let line = (model.size / 400.).clamp(0.6, 1.1);
    let glossy = model.look == Look::Glossy;
    let outline = |width| {
        (!glossy)
            .then_some(if dark { RIM } else { OUTLINE })
            .map(|(color, alpha)| Outline {
                color,
                alpha,
                width,
            })
    };
    if model.background == Background::Card {
        let (lo, hi) = view.bounds();
        let width = model.size / 340.;
        let inset = width / 2.;
        out.push(Primitive::Card {
            lo: lo.offset(inset, inset),
            hi: hi.offset(-inset, -inset),
            radius: (model.size * 10. / 340.).min((hi.x - lo.x).min(hi.y - lo.y) / 2.),
            fill: card,
            outline: Outline {
                color: card_edge,
                alpha: 1.,
                width,
            },
        });
    }
    enum Item {
        /// The half-bond from a point toward another.
        Bond(usize, usize),
        Atom(usize),
    }
    // Each half-bond sorts at its own depth but never after its atom, so the
    // ball covers the half that points toward the viewer yet a half-bond
    // still passes in front of a farther atom. Ties draw the half first.
    let mut items = vec![];
    // Stick draws each half-bond as its share of the joints; an atom with
    // none on screen is a disc.
    let joints = if style == Style::Stick {
        view.joints(
            &bonds
                .iter()
                .flat_map(|&(a, b)| [(a, b), (b, a)])
                .collect::<Vec<_>>(),
        )
    } else {
        HashMap::new()
    };
    for &(a, b) in &bonds {
        let (pa, pb) = (*rotated.get(a)?, *rotated.get(b)?);
        let middle = zip3(pa, pb, |a, b| (a + b) / 2.);
        for (s, o) in [(a, b), (b, a)] {
            if style == Style::Stick && !joints.contains_key(&(s, o)) {
                continue;
            }
            let ps = *rotated.get(s)?;
            let z = (ps[2] + middle[2]) / 2. - 0.05;
            items.push((z.min(ps[2]), 0, z, Item::Bond(s, o)));
        }
    }
    // Wireframe and stick draw only the atoms without shown bonds, as dots
    // and discs.
    let bonded: HashSet<usize> = if style == Style::Stick {
        joints.keys().map(|&(s, _)| s).collect()
    } else {
        bonds.iter().flat_map(|&(a, b)| [a, b]).collect()
    };
    items.extend(
        view.balls()
            .filter(|(i, ..)| {
                !matches!(style, Style::Wireframe | Style::Stick) || !bonded.contains(i)
            })
            .filter_map(|(i, ..)| {
                let z = rotated.get(i)?[2];
                Some((z, 1, z, Item::Atom(i)))
            }),
    );
    items.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    // Where each half-bond is drawn, so the first of a pair can reach under
    // the second.
    let order: HashMap<(usize, usize), usize> = items
        .iter()
        .enumerate()
        .filter_map(|(n, (.., item))| match item {
            Item::Bond(s, o) => Some(((*s, *o), n)),
            Item::Atom(_) => None,
        })
        .collect();
    let shade = |base: [f32; 3], stops: &[(f32, [f32; 3], f32)]| {
        stops
            .iter()
            .map(|&(offset, toward, t)| (offset, rgb(mix(base, toward, t))))
            .collect::<Vec<_>>()
    };
    let colors: HashMap<&str, [u8; 3]> = view
        .symbols
        .iter()
        .map(|s| (*s, element_color(doc, s)))
        .collect();
    // Stick style's outline: one stroke of every shape, behind all of them.
    let mut rims: (Vec<Vec<Point>>, Vec<Vec<Point>>) = Default::default();
    let mut drawn = vec![];
    for (n, (_, _, z, item)) in items.into_iter().enumerate() {
        let (Item::Bond(i, _) | Item::Atom(i)) = item;
        let fading = view.symbols.get(i) == Some(&"H") && grown < 1.;
        let symbol = *view.symbols.get(i)?;
        let base = mix(
            float(*colors.get(symbol).unwrap_or(&GREY)),
            float(backdrop),
            fade(z),
        );
        let wire = rgb(mix(base, BLACK, if symbol == "C" { 0.35 } else { 0.1 }));
        let primitive = match item {
            Item::Atom(i) if style == Style::Wireframe => {
                let center = view.project(*rotated.get(i)?).0;
                Primitive::Sphere {
                    center,
                    radius: view.wire,
                    focal: center,
                    stops: vec![(0., wire), (1., wire)],
                    outline: None,
                }
            }
            Item::Atom(i) => {
                let (q, f) = view.project(*rotated.get(i)?);
                let r = style.atom_radius(symbol) * view.scale * f;
                if style == Style::Stick {
                    let steps = 48;
                    if fading { &mut rims.1 } else { &mut rims.0 }.push(
                        (0..steps)
                            .map(|k| {
                                let t = std::f32::consts::TAU * k as f32 / steps as f32;
                                q.offset(r * t.cos(), r * t.sin())
                            })
                            .collect(),
                    );
                }
                Primitive::Sphere {
                    center: q,
                    radius: r,
                    focal: q.offset(-r * 0.38, -r * 0.42),
                    stops: if glossy {
                        shade(
                            base,
                            &[
                                (0., WHITE, 0.85),
                                (0.18, WHITE, 0.35),
                                (0.75, base, 0.),
                                (1., BLACK, 0.45),
                            ],
                        )
                    } else {
                        shade(
                            base,
                            &[(0., WHITE, 0.45), (0.6, base, 0.), (1., BLACK, 0.18)],
                        )
                    },
                    outline: if style == Style::Stick {
                        None
                    } else {
                        outline(line)
                    },
                }
            }
            Item::Bond(s, o) => {
                let (a, e, start, half) = view.half_bond(s, o)?;
                if style == Style::Wireframe {
                    Primitive::Wire {
                        from: a,
                        to: e,
                        width: half * 2.,
                        color: wire,
                    }
                } else {
                    let (dx, dy) = (e.x - a.x, e.y - a.y);
                    let len = dx.hypot(dy);
                    // Stick's joints already left out the halves seen end-on.
                    if len < line * 0.5 && style != Style::Stick {
                        continue;
                    }
                    // The lit side faces up and left like the balls' highlight,
                    // the same for both halves, so the shading runs on across
                    // the colour seam.
                    let side = if 0.38 * dy - 0.42 * dx < 0. { -1. } else { 1. };
                    let (nx, ny) = (-dy / len * side, dx / len * side);
                    // The half drawn first runs on under the other, so their
                    // anti-aliased edges never meet in a pale seam; the
                    // colour change stays at the middle.
                    let (end, width) = view
                        .half_bond(o, s)
                        .filter(|_| order.get(&(o, s)).is_some_and(|&m| m > n))
                        .filter(|_| *view.symbols.get(o).unwrap_or(&"") != "H" || grown >= 1.)
                        .map(|(b, _, far, _)| (b.distance(e), far))
                        .filter(|(other, _)| *other >= line * 0.5)
                        .map_or((e, half), |(other, far)| {
                            let reach = half.min(other / 2.);
                            let t = reach / other;
                            (
                                e.offset(dx / len * reach, dy / len * reach),
                                half + (far - half) * t,
                            )
                        });
                    let stops = if glossy {
                        shade(
                            base,
                            &[(0., BLACK, 0.4), (0.3, WHITE, 0.55), (1., BLACK, 0.45)],
                        )
                    } else {
                        shade(
                            base,
                            &[(0., BLACK, 0.2), (0.4, WHITE, 0.3), (1., BLACK, 0.22)],
                        )
                    };
                    if let Some(joint) = joints.get(&(s, o)) {
                        // From the middle round the joint and back, the
                        // left edge first.
                        let (lx, ly) = (-dy / len, dx / len);
                        let mut points = vec![end.offset(lx * width, ly * width)];
                        points.extend(joint);
                        points.push(end.offset(-lx * width, -ly * width));
                        if fading { &mut rims.1 } else { &mut rims.0 }.push(points.clone());
                        drawn.push((
                            fading,
                            Primitive::ShadedPolygon {
                                points,
                                from: e.offset(nx * half, ny * half),
                                to: e.offset(-nx * half, -ny * half),
                                stops,
                            },
                        ));
                        continue;
                    }
                    Primitive::ShadedQuad {
                        corners: [
                            a.offset(nx * start, ny * start),
                            end.offset(nx * width, ny * width),
                            end.offset(-nx * width, -ny * width),
                            a.offset(-nx * start, -ny * start),
                        ],
                        from: e.offset(nx * half, ny * half),
                        to: e.offset(-nx * half, -ny * half),
                        stops,
                        outline: if style == Style::BallStick {
                            outline(line * 0.8)
                        } else {
                            None
                        },
                    }
                }
            }
        };
        drawn.push((fading, primitive));
    }
    // Hydrogens fade in as a whole, as the prototype's globalAlpha does.
    let fade_in = |fading: bool, primitive| {
        if fading {
            Primitive::Opacity {
                alpha: grown,
                primitive: Box::new(primitive),
            }
        } else {
            primitive
        }
    };
    if let Some(outline) = outline(line * 2.) {
        for (polygons, fading) in [(rims.0, false), (rims.1, true)] {
            if !polygons.is_empty() {
                out.push(fade_in(fading, Primitive::Contour { polygons, outline }));
            }
        }
    }
    out.extend(drawn.into_iter().map(|(fading, p)| fade_in(fading, p)));
    Some(())
}

#[cfg(test)]
mod tests;
