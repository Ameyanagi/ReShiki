use crate::{
    generic::Generic,
    typography::{TextFormat, TextStyle},
};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
impl Point {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn distance(self, other: Self) -> f32 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
    pub fn offset(self, dx: f32, dy: f32) -> Self {
        Self::new(self.x + dx, self.y + dy)
    }
    /// Linearly interpolates from this point to another.
    pub fn lerp(self, other: Self, t: f32) -> Self {
        Self::new(
            self.x + (other.x - self.x) * t,
            self.y + (other.y - self.y) * t,
        )
    }
    /// Returns whether both coordinates are finite.
    pub fn is_finite(self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

fn is_zero_u64(value: &u64) -> bool {
    *value == 0
}

/// Winding is relative to the explicit neighbor order, not a toolkit index.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AtomStereo {
    pub winding: String,
    pub neighbors: Vec<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Atom {
    #[serde(
        default,
        skip_serializing_if = "crate::atom_labels::AtomDisplay::is_default"
    )]
    pub display: crate::atom_labels::AtomDisplay,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cip_label: Option<String>,
    pub id: u64,
    pub element: String,
    pub position: Point,
    /// Projection depth in drawing units, retained when tilting back.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub depth: f32,
    /// Target atom IDs. Without `attachment` this is a nonchemical centroid.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub centroid: Vec<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attachment: Option<crate::attachments::Kind>,
    #[serde(default)]
    pub charge: i32,
    #[serde(default)]
    pub radical_electrons: u8,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub marks: Vec<crate::scientific::AtomMark>,
    /// High-water mark: deleting an annotation never reuses its attachment ID.
    #[serde(default, skip_serializing_if = "is_zero_u64")]
    pub mark_serial: u64,
    #[serde(default)]
    pub isotope: u32,
    #[serde(default)]
    pub explicit_h: u32,
    #[serde(default)]
    pub no_implicit: bool,
    #[serde(default)]
    pub aromatic: bool,
    #[serde(default)]
    pub stereo: Option<AtomStereo>,
    #[serde(default)]
    pub map_num: u32,
    #[serde(default)]
    pub label_h: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text_style: Option<TextStyle>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Bond {
    /// Part of an imported crystal distance graph whose chemical bond
    /// assignments remain unvalidated. Kept after cell removal and edge edits;
    /// compound edits within a live cell conservatively retain that context.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub distance_derived: bool,
    /// Persistent paint behind the bond, independent of its ordinary ink.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub highlight: Option<crate::palette::Color>,
    /// Draw the inner component along its ring; chemical order stays unchanged.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub ring_arc: bool,
    /// Bond appearance describes projection depth, not tetrahedral stereochemistry.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub projection: bool,
    /// Captured chemical stereo is independent of this bond's projected XY.
    /// An authoritative `None` keeps unspecified double-bond stereo unspecified.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stereo_authoritative: bool,
    #[serde(default)]
    pub z_order: i16,
    #[serde(
        default,
        skip_serializing_if = "crate::atom_labels::StereoDisplay::is_default"
    )]
    pub indicator: crate::atom_labels::StereoDisplay,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cip_label: Option<String>,
    pub a: u64,
    pub b: u64,
    /// 0 = hydrogen interaction; 1–3 = order; 4 = aromatic; 5 = dative;
    /// 6 = quadruple; 7 = nonaromatic partial order 1.5.
    pub order: u8,
    #[serde(default = "plain")]
    pub display: String,
    #[serde(default)]
    pub stereo: Option<String>,
    #[serde(default)]
    pub stereo_atoms: Vec<u64>,
    #[serde(default)]
    pub double_position: crate::bonds::DoublePosition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secondary_display: Option<String>,
    #[serde(default)]
    pub color: crate::palette::Color,
}
fn is_zero(value: &f32) -> bool {
    *value == 0.
}
fn plain() -> String {
    "plain".into()
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Annotation {
    pub id: u64,
    pub position: Point,
    pub text: String,
    #[serde(default)]
    pub format: TextFormat,
}
impl Annotation {
    pub fn size(&self) -> (f32, f32) {
        let layout = crate::typography::layout(&self.text, &self.format);
        (layout.width, layout.height)
    }
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Arrow {
    pub id: u64,
    pub start: Point,
    pub end: Point,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_anchor: Option<crate::arrow_anchors::Anchor>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_anchor: Option<crate::arrow_anchors::Anchor>,
    #[serde(default = "forward")]
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control: Option<Point>,
    /// Independently editable departure and arrival controls. Absent in legacy
    /// quadratic arrows; these controls take precedence over the implicit bend.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cubic: Option<[Point; 2]>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub style: Option<crate::arrows::ArrowStyle>,
}
fn forward() -> String {
    "forward".into()
}

/// Collapsed abbreviations; older drawings carrying them are rejected.
pub const ABBREVIATIONS: u32 = 10;
/// Version that creating or appending abbreviations raises a drawing to: the
/// format version (per-bond crossing depth) when abbreviation editing landed.
pub const ABBREVIATION_EDITING: u32 = 11;
/// Reaction membership and roles.
pub const REACTIONS: u32 = 15;
/// Lowest version the editor's write paths give a drawing: new drawings, open,
/// recovery, page setup, printing, cleanup, styles, abbreviation detection and
/// chemistry output. Each stamped the then-current version when reactions
/// landed; saved files are now marked with [`VERSION`].
pub const EDITOR_BASELINE: u32 = REACTIONS;
/// Embedded custom themes and palette hues.
pub const CUSTOM_THEMES: u32 = 16;
/// Custom colors stored exactly as shown on either canvas.
pub const EXACT_COLORS: u32 = 17;
/// Persisted rear-side opacity for editable 3D molecules.
pub const REAR_OPACITY: u32 = 22;
/// 3D models of structures; their validation requires this version.
pub const MODELS_3D: u32 = 28;
/// Crystal unit cells, bonds leaving the cell and atom sites on 3D models.
pub const CRYSTALS: u32 = 29;
/// Persistent provenance preventing distance bonds from becoming chemical input.
pub const DISTANCE_BONDS: u32 = 30;

/// The newest document format this build reads. Saved files are marked with it.
///
/// | Version | Added |
/// | --- | --- |
/// | 1 | Original native drawings. |
/// | 2 | Arrow styles. |
/// | 10 | Collapsed abbreviations. |
/// | 11 | Per-bond crossing depth. |
/// | 12 | Physical publication page layouts. |
/// | 13 | Embedded pictures. |
/// | 14 | Document drawing styles. |
/// | 15 | Reaction membership and roles. |
/// | 16 | Embedded custom themes (later also palette hues). |
/// | 17 | Exact custom colors on either canvas; newer-version probing. |
/// | 18 | Persistent atom and bond highlights. |
/// | 19 | Editable 3D depth appearance and captured projection stereo (#146; 0.11.0). |
/// | 20 | Retained EMF picture sources (#296); cubic arrow controls on a parallel branch. |
/// | 21 | Arrow target links and attachment IDs (cubic-arrow branch). |
/// | 22 | Persisted rear-side opacity (parallel branch). |
/// | 23 | Not used. |
/// | 24 | EMF sources, cubic arrows, target links and rear opacity integrated (#297). |
/// | 25 | Linked molecule-name captions and reaction atom-map labels (#298). |
/// | 26 | Generic structures and coordination joining (#300; 0.12.0). |
/// | 27 | Condition text attached to reaction arrows. |
/// | 28 | 3D models of structures (#386). |
/// | 29 | Crystal unit cells and atom sites on 3D models (#417; 0.13.0). |
/// | 30 | Persistent distance-derived bond provenance (#517). |
pub const VERSION: u32 = 30;

fn newer_version(version: u64) -> String {
    format!(
        "This drawing was made with a newer version of ReShiki (document version {version}). Update ReShiki to open it."
    )
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    #[serde(default, skip_serializing_if = "crate::generic::Generic::is_empty")]
    pub generic: crate::generic::Generic,
    #[serde(
        default,
        skip_serializing_if = "crate::canvas_theme::CanvasTheme::is_light"
    )]
    pub canvas_theme: crate::canvas_theme::CanvasTheme,
    #[serde(
        default,
        skip_serializing_if = "crate::canvas_theme::ColorTheme::is_publication"
    )]
    pub color_theme: crate::canvas_theme::ColorTheme,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_theme: Option<Box<crate::theme_files::ThemeFile>>,
    #[serde(
        default,
        skip_serializing_if = "crate::style::DrawingStyle::is_default"
    )]
    pub drawing_style: crate::style::DrawingStyle,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_layout: Option<crate::pages::Layout>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub abbreviations: Vec<crate::abbreviations::Abbreviation>,
    #[serde(default)]
    pub atom_labels: crate::atom_labels::Settings,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ring_fills: Vec<crate::ring_fills::RingFill>,
    /// Editable projection paint; ordinary foreground and fill colors remain
    /// the base colors restored by clearing the depth appearance.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depth_appearance: Vec<crate::depth_appearance::Scope>,
    pub version: u32,
    pub atoms: Vec<Atom>,
    pub bonds: Vec<Bond>,
    #[serde(default)]
    pub annotations: Vec<Annotation>,
    /// Optional native links; older readers retain visible annotation text but
    /// cannot preserve its molecule association.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub molecule_names: Vec<crate::molecule_names::MoleculeName>,
    #[serde(default)]
    pub arrows: Vec<Arrow>,
    /// Text owned by an arrow; older readers keep the text but not the link.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub arrow_labels: Vec<crate::arrow_labels::ArrowLabel>,
    #[serde(default)]
    pub graphics: Vec<crate::graphics::Graphic>,
    #[serde(default)]
    pub groups: Vec<crate::grouping::Group>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<crate::reactions::Reaction>,
    /// 3D models linked to whole molecules by atom ID.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub models: Vec<crate::models3d::Model3d>,
    /// Recent custom colors for the color picker, newest first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub recent_colors: Vec<crate::color_contrast::Rgb>,
}
impl Default for Document {
    fn default() -> Self {
        Self {
            generic: Default::default(),
            ring_fills: vec![],
            depth_appearance: vec![],
            version: EDITOR_BASELINE,
            drawing_style: Default::default(),
            canvas_theme: Default::default(),
            color_theme: Default::default(),
            custom_theme: None,
            page_layout: None,
            abbreviations: vec![],
            atom_labels: Default::default(),
            atoms: vec![],
            bonds: vec![],
            annotations: vec![],
            molecule_names: vec![],
            arrows: vec![],
            arrow_labels: vec![],
            graphics: vec![],
            groups: vec![],
            reactions: vec![],
            models: vec![],
            recent_colors: vec![],
        }
    }
}
impl Document {
    /// Raise the in-memory document version to at least this feature's floor.
    pub fn require_version(&mut self, floor: u32) {
        self.version = self.version.max(floor);
    }
    /// Read a native drawing. The version is read first, so a drawing from a
    /// newer ReShiki is reported as such instead of as a parse error.
    pub fn from_json(bytes: &[u8]) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Probe {
            version: Option<u64>,
        }
        if let Ok(Probe {
            version: Some(version),
        }) = serde_json::from_slice(bytes)
            && version > u64::from(VERSION)
        {
            return Err(newer_version(version));
        }
        let mut doc: Self = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        doc.validate()?;
        doc.migrate();
        crate::models3d::retain_whole(&mut doc);
        crate::arrow_anchors::reconcile(&mut doc);
        doc.validate()?;
        Ok(doc)
    }
    /// Bring a drawing read from an earlier document version to the current
    /// color form. Before version 17 custom colors were light-canvas bytes that
    /// the dark canvas showed lightness-flipped; they are now exact on both
    /// canvases, so a dark drawing stores what it showed. Ink and palette colors
    /// already follow the canvas.
    pub fn migrate(&mut self) {
        if self.version >= EXACT_COLORS || self.canvas_theme.is_light() {
            return;
        }
        let canvas = self.canvas_theme;
        crate::palette::for_each_color_mut(self, |color| {
            if let crate::palette::Color::Custom(rgb) = *color {
                *color = crate::palette::Color::Custom(canvas.color(rgb));
            }
        });
        self.version = EXACT_COLORS;
    }
    /// Open a native drawing file as the editor does: [`Self::from_json`],
    /// the version raised to at least 15, and computed labels cleared.
    pub fn from_native_file(bytes: &[u8]) -> Result<Self, String> {
        let mut doc = Self::from_json(bytes)?;
        doc.require_version(EDITOR_BASELINE);
        crate::atom_labels::clear_computed(&mut doc);
        Ok(doc)
    }
    /// Native file contents, marked with this build's document version.
    pub fn file_json(&self) -> Result<Vec<u8>, String> {
        serde_json::to_vec_pretty(&self.current()).map_err(|e| e.to_string())
    }
    /// A copy marked with this build's document version, for files, clipboard
    /// data and recovery drafts, so reading them back never migrates again.
    pub fn current(&self) -> Self {
        Self {
            version: VERSION,
            ..self.clone()
        }
    }
    /// The next free ID: the largest object or group ID plus one, saturating.
    /// Objects and groups share this counter, and a deleted maximum is reused.
    pub fn next_id(&self) -> u64 {
        self.object_ids()
            .chain(self.groups.iter().map(|a| a.id))
            .max()
            .unwrap_or(0)
            .saturating_add(1)
    }
    pub fn add_atom(&mut self, element: &str, position: Point) -> u64 {
        let id = self.next_id();
        self.atoms.push(Atom {
            display: Default::default(),
            cip_label: None,
            id,
            element: element.into(),
            position,
            depth: 0.,
            centroid: vec![],
            attachment: None,
            charge: 0,
            radical_electrons: 0,
            marks: vec![],
            mark_serial: 0,
            isotope: 0,
            explicit_h: 0,
            no_implicit: element == "*",
            aromatic: false,
            stereo: None,
            map_num: 0,
            label_h: 0,
            text_style: None,
        });
        id
    }
    pub fn atom(&self, id: u64) -> Option<&Atom> {
        self.atoms.iter().find(|a| a.id == id)
    }
    pub fn atom_mut(&mut self, id: u64) -> Option<&mut Atom> {
        self.atoms.iter_mut().find(|a| a.id == id)
    }
    /// Return the first bond connecting either endpoint order, with its index.
    pub fn bond_between(&self, a: u64, b: u64) -> Option<(usize, &Bond)> {
        self.bonds
            .iter()
            .enumerate()
            .find(|(_, bond)| bond.connects(a, b))
    }
    /// Return the index of the first bond connecting either endpoint order.
    pub fn bond_position(&self, a: u64, b: u64) -> Option<usize> {
        self.bond_between(a, b).map(|(i, _)| i)
    }
    pub fn nearest(&self, point: Point, radius: f32) -> Option<u64> {
        self.atoms
            .iter()
            .filter(|a| self.atom_drawn(a.id) && a.position.distance(point) < radius)
            .min_by(|a, b| {
                a.position
                    .distance(point)
                    .total_cmp(&b.position.distance(point))
            })
            .map(|a| a.id)
            .or_else(|| crate::abbreviations::label_hit(self, point, radius))
    }
    pub fn add_bond(&mut self, a: u64, b: u64, order: u8, display: &str) {
        if a == b || self.atom(a).is_none() || self.atom(b).is_none() {
            return;
        }
        self.invalidate_chemistry(&[a, b]);
        let previous = self
            .bonds
            .iter_mut()
            .find(|x| (x.a == a && x.b == b) || (x.a == b && x.b == a));
        let retained = previous.as_deref();
        let mut fresh = Bond::plain(a, b, order, display);
        if let Some(retained) = retained {
            fresh.distance_derived = retained.distance_derived;
            fresh.highlight = retained.highlight;
            fresh.z_order = retained.z_order;
            fresh.indicator = retained.indicator.clone();
            fresh.double_position = retained.double_position;
            fresh.color = retained.color;
        }
        if let Some(bond) = previous {
            *bond = fresh;
        } else {
            self.bonds.push(fresh);
        }
    }
    pub fn invalidate_chemistry(&mut self, affected: &[u64]) {
        if !self.atoms.iter().any(|a| affected.contains(&a.id)) {
            return;
        }
        crate::atom_labels::clear_computed(self);
        // Even an unspecified captured alkene can become a new chemical case
        // when a substituent is edited. Coordinate-only edits do not enter here.
        let neighbors: HashSet<_> = self
            .bonds
            .iter()
            .filter_map(|bond| {
                if affected.contains(&bond.a) {
                    Some(bond.b)
                } else if affected.contains(&bond.b) {
                    Some(bond.a)
                } else {
                    None
                }
            })
            .collect();
        for atom in &mut self.atoms {
            atom.label_h = 0;
            if affected.contains(&atom.id)
                || atom
                    .stereo
                    .as_ref()
                    .is_some_and(|s| s.neighbors.iter().any(|n| affected.contains(n)))
            {
                atom.stereo = None;
            }
        }
        for bond in &mut self.bonds {
            if affected.contains(&bond.a)
                || affected.contains(&bond.b)
                || bond.stereo_atoms.iter().any(|a| affected.contains(a))
                || bond.stereo_authoritative
                    && (neighbors.contains(&bond.a) || neighbors.contains(&bond.b))
            {
                bond.stereo = None;
                bond.stereo_atoms.clear();
                bond.stereo_authoritative = false;
            }
        }
    }
    pub fn delete(&mut self, ids: &[u64]) {
        crate::models3d::crystal::mark_distance_bonds(self);
        let ids = self.expand_abbreviation_selection(ids);
        let ids = ids.as_slice();
        self.expand_abbreviations(ids);
        self.invalidate_chemistry(ids);
        self.atoms.retain(|a| !ids.contains(&a.id));
        self.bonds
            .retain(|b| !ids.contains(&b.a) && !ids.contains(&b.b));
        self.annotations.retain(|a| !ids.contains(&a.id));
        crate::molecule_names::prune(self);
        crate::models3d::prune(self);
        self.arrows.retain(|a| !ids.contains(&a.id));
        self.graphics.retain(|a| !ids.contains(&a.id));
        crate::arrow_labels::prune(self);
        crate::projection::prune_centroids(self);
        crate::ring_fills::prune(self);
        crate::depth_appearance::prune(self);
        self.prune_groups();
        crate::reactions::prune(self);
        crate::generic::prune(self);
        crate::arrow_anchors::reconcile(self);
    }
    pub fn translate(&mut self, ids: &[u64], dx: f32, dy: f32) {
        let ids = self.expand_abbreviation_selection(ids);
        let ids = ids.as_slice();
        for graphic in &mut self.graphics {
            if ids.contains(&graphic.id) {
                graphic.origin = graphic.origin.offset(dx, dy);
            }
        }
        for a in &mut self.atoms {
            if ids.contains(&a.id) {
                a.position = a.position.offset(dx, dy);
            }
        }
        for a in &mut self.annotations {
            if ids.contains(&a.id) {
                a.position = a.position.offset(dx, dy);
            }
        }
        for a in &mut self.arrows {
            if ids.contains(&a.id) {
                a.map_points(|p| p.offset(dx, dy));
            }
        }
        crate::projection::sync_centroids(self);
        crate::arrow_anchors::transformed(self, ids, |v| v);
    }
    /// Every object ID, in [`Document::object_ids`] order.
    pub fn all_ids(&self) -> Vec<u64> {
        self.object_ids().collect()
    }
    /// Atom, annotation, arrow and graphic IDs, in that order. They share one ID
    /// space; groups use the same counter but are not objects, and bonds have
    /// no ID.
    pub fn object_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.atoms
            .iter()
            .map(|a| a.id)
            .chain(self.annotations.iter().map(|a| a.id))
            .chain(self.arrows.iter().map(|a| a.id))
            .chain(self.graphics.iter().map(|a| a.id))
    }
    /// Clear drawing content while retaining document settings and preferences.
    pub fn clear_content(&mut self) {
        let Self {
            generic,
            canvas_theme: _,
            color_theme: _,
            custom_theme: _,
            drawing_style: _,
            page_layout: _,
            abbreviations,
            atom_labels: _,
            ring_fills,
            depth_appearance,
            version: _,
            atoms,
            bonds,
            annotations,
            molecule_names,
            arrows,
            arrow_labels,
            graphics,
            groups,
            reactions,
            models,
            recent_colors: _,
        } = self;
        atoms.clear();
        models.clear();
        arrow_labels.clear();
        bonds.clear();
        annotations.clear();
        arrows.clear();
        graphics.clear();
        groups.clear();
        abbreviations.clear();
        reactions.clear();
        ring_fills.clear();
        depth_appearance.clear();
        molecule_names.clear();
        *generic = Generic::default();
    }
    /// Whether the document contains any drawing content.
    pub fn has_content(&self) -> bool {
        let Self {
            generic,
            canvas_theme: _,
            color_theme: _,
            custom_theme: _,
            drawing_style: _,
            page_layout: _,
            abbreviations,
            atom_labels: _,
            ring_fills,
            depth_appearance,
            version: _,
            atoms,
            bonds,
            annotations,
            molecule_names,
            arrows,
            arrow_labels,
            graphics,
            groups,
            reactions,
            models,
            recent_colors: _,
        } = self;
        !atoms.is_empty()
            || !models.is_empty()
            || !arrow_labels.is_empty()
            || !bonds.is_empty()
            || !annotations.is_empty()
            || !arrows.is_empty()
            || !graphics.is_empty()
            || !groups.is_empty()
            || !abbreviations.is_empty()
            || !reactions.is_empty()
            || !ring_fills.is_empty()
            || !depth_appearance.is_empty()
            || !molecule_names.is_empty()
            || !generic.is_empty()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.version < DISTANCE_BONDS && self.bonds.iter().any(|b| b.distance_derived) {
            return Err(format!(
                "Distance-derived bonds need document version {DISTANCE_BONDS} or later"
            ));
        }
        crate::projection::validate(self)?;
        crate::depth_appearance::validate(self)?;
        crate::ring_fills::validate(self)?;
        crate::attachments::validate(self)?;
        crate::generic::validate(self)?;
        crate::models3d::validate(self)?;
        self.drawing_style.validate()?;
        if let Some(theme) = &self.custom_theme {
            theme.validate()?;
        }
        let mut picture_bytes = 0_usize;
        let mut picture_pixels = 0_u64;
        for picture in self.graphics.iter().filter_map(|g| g.picture.as_ref()) {
            picture_bytes = picture_bytes.saturating_add(picture.stored_bytes());
            picture_pixels = picture_pixels
                .saturating_add(u64::from(picture.width()) * u64::from(picture.height()));
            if picture_bytes > 64 * 1024 * 1024 || picture_pixels > 64_000_000 {
                return Err("Drawing pictures exceed 64 MB or 64 million pixels".into());
            }
        }
        if self.version > VERSION {
            return Err(newer_version(self.version.into()));
        }
        if self.version == 0 {
            return Err("Unsupported document version 0".into());
        }
        if let Some(layout) = &self.page_layout {
            layout.validate()?;
        }
        if self.recent_colors.len() > crate::palette::RECENT_LIMIT {
            return Err("Too many recent colors".into());
        }
        self.validate_groups()?;
        crate::reactions::validate(self)?;
        self.validate_abbreviations()?;
        let mut ids = HashSet::new();
        for id in self.object_ids() {
            if id == 0 || id == u64::MAX || !ids.insert(id) {
                return Err("Duplicate or zero object ID".into());
            }
        }
        let atom_ids: HashSet<_> = self.atoms.iter().map(|a| a.id).collect();
        let mut neighbors: std::collections::HashMap<_, HashSet<_>> =
            std::collections::HashMap::new();
        for bond in &self.bonds {
            neighbors.entry(bond.a).or_default().insert(bond.b);
            neighbors.entry(bond.b).or_default().insert(bond.a);
        }
        let empty_neighbors = HashSet::new();
        for a in &self.atoms {
            a.display.validate()?;
            if a.element != "*" && !a.display.label_scripts.is_empty() {
                return Err("Atom label scripts require wildcard atoms".into());
            }
            if a.map_num > i32::MAX as u32 {
                return Err("Atom maps must be from 1 to 2147483647, or 0 to clear".into());
            }
            if a.display.variable.is_some()
                && a.element != "*"
                && !self.generic.links.iter().any(|link| link.atom == a.id)
            {
                return Err("Variable labels require wildcard atoms".into());
            }
            if a.cip_label
                .as_ref()
                .is_some_and(|s| !["R", "S", "r", "s"].contains(&s.as_str()))
            {
                return Err("Unsupported atom CIP label".into());
            }
            if a.radical_electrons > 2
                || a.marks.len() > 12
                || a.marks.iter().any(|m| {
                    !m.offset.x.is_finite()
                        || !m.offset.y.is_finite()
                        || !m.angle.is_finite()
                        || m.size_pt
                            .is_some_and(|n| !n.is_finite() || !(0.5..=96.).contains(&n))
                })
            {
                return Err("Invalid atom radical count or mark position".into());
            }
            if let Some(style) = &a.text_style {
                style.validate()?;
            }
            if !a.position.x.is_finite() || !a.position.y.is_finite() {
                return Err("Non-finite atom position".into());
            }
            if a.element.is_empty() || a.element.len() > 3 {
                return Err("Invalid element symbol".into());
            }
            if let Some(s) = &a.stereo {
                let actual = neighbors.get(&a.id).unwrap_or(&empty_neighbors);
                if !["cw", "ccw"].contains(&s.winding.as_str())
                    || actual != &s.neighbors.iter().copied().collect()
                    || actual.len() != s.neighbors.len()
                {
                    return Err("Invalid stereocenter neighbor mapping".into());
                }
            }
        }
        let mut pairs = HashSet::new();
        for b in &self.bonds {
            b.indicator.validate()?;
            if b.cip_label
                .as_ref()
                .is_some_and(|s| !["E", "Z"].contains(&s.as_str()))
            {
                return Err("Unsupported bond CIP label".into());
            }
            if b.a == b.b || !atom_ids.contains(&b.a) || !atom_ids.contains(&b.b) || b.order > 7 {
                return Err("Invalid bond endpoints or order".into());
            }
            if !pairs.insert((b.a.min(b.b), b.a.max(b.b))) {
                return Err("Duplicate bond".into());
            }
            b.validate_appearance()?;
            if b.stereo_authoritative && b.order != 2 {
                return Err("Captured double-bond stereo requires a double bond".into());
            }
            if b.stereo.is_some()
                && ((b.stereo_atoms.len() != 2
                    && !(b.stereo.as_deref() == Some("any") && b.stereo_atoms.is_empty()))
                    || b.stereo_atoms.iter().any(|id| !atom_ids.contains(id)))
            {
                return Err("Invalid bond stereo references".into());
            }
        }
        for p in self
            .annotations
            .iter()
            .map(|a| a.position)
            .chain(self.arrows.iter().flat_map(|a| [a.start, a.end]))
        {
            if !p.x.is_finite() || !p.y.is_finite() {
                return Err("Non-finite drawing position".into());
            }
        }
        for a in &self.annotations {
            a.format.validate(&a.text)?;
        }
        crate::molecule_names::validate(self)?;
        crate::arrow_labels::validate(self)?;
        for graphic in &self.graphics {
            graphic.validate()?;
        }
        for arrow in &self.arrows {
            arrow.validate()?;
        }
        crate::arrow_anchors::validate(self)?;
        Ok(())
    }
    pub fn bounds(&self) -> (Point, Point) {
        match crate::scene::selection_bounds(self, &self.all_ids()) {
            Some((lo, hi)) => (lo.offset(-30., -30.), hi.offset(80., 30.)),
            None => (Point::new(-100., -75.), Point::new(100., 75.)),
        }
    }
}

pub struct History {
    undo: VecDeque<Document>,
    redo: VecDeque<Document>,
    limit: usize,
    /// Estimated undo bytes ([`frame_bytes`]) above which the oldest frames
    /// are dropped, down to [`History::MIN_STEPS`].
    max_bytes: usize,
}
/// The editor keeps 100 undo frames, fewer on large drawings: about 128 MB of
/// frames, but always at least [`History::MIN_STEPS`].
impl Default for History {
    fn default() -> Self {
        Self {
            max_bytes: 128 << 20,
            ..Self::with_limit(100)
        }
    }
}
/// Rough heap bytes of one history frame, from object counts. Pictures are
/// shared between frames, so their data is not counted.
fn frame_bytes(doc: &Document) -> usize {
    use std::mem::{size_of, size_of_val as bytes};
    // Small strings and lists each object owns, such as an element symbol.
    const OWNED: usize = 16;
    let objects = doc.atoms.len()
        + doc.bonds.len()
        + doc.annotations.len()
        + doc.arrows.len()
        + doc.arrow_labels.len()
        + doc.molecule_names.len()
        + doc.graphics.len()
        + doc.groups.len()
        + doc.reactions.len()
        + doc.abbreviations.len();
    let text: usize = doc.annotations.iter().map(|a| a.text.len()).sum();
    let paths: usize = doc.graphics.iter().map(|g| bytes(&g.path[..])).sum();
    let models: usize = doc
        .models
        .iter()
        .map(|m| bytes(&m.atoms[..]) + bytes(&m.hydrogens[..]) + bytes(&m.sites[..]))
        .sum();
    size_of::<Document>()
        + bytes(&doc.atoms[..])
        + bytes(&doc.bonds[..])
        + bytes(&doc.annotations[..])
        + bytes(&doc.arrows[..])
        + bytes(&doc.arrow_labels[..])
        + bytes(&doc.molecule_names[..])
        + bytes(&doc.graphics[..])
        + bytes(&doc.groups[..])
        + bytes(&doc.reactions[..])
        + bytes(&doc.abbreviations[..])
        + bytes(&doc.models[..])
        + objects * OWNED
        + text
        + paths
        + models
}
impl History {
    /// Undo steps kept however large the drawing.
    pub const MIN_STEPS: usize = 20;
    /// An empty history that keeps at most `limit` undo frames (at least one),
    /// whatever their size.
    pub fn with_limit(limit: usize) -> Self {
        Self {
            undo: VecDeque::new(),
            redo: VecDeque::new(),
            limit: limit.max(1),
            max_bytes: usize::MAX,
        }
    }
    pub fn limit(&self) -> usize {
        self.limit
    }
    /// Undo plus redo frames.
    pub fn frames(&self) -> usize {
        self.undo.len() + self.redo.len()
    }
    /// The drawing the next undo (or, with `redo`, the next redo) restores.
    pub fn peek(&self, redo: bool) -> Option<&Document> {
        if redo {
            self.redo.back()
        } else {
            self.undo.back()
        }
    }
    /// Undo frames, oldest first; the last is the next undo.
    pub fn undo_frames(&self) -> std::collections::vec_deque::Iter<'_, Document> {
        self.undo.iter()
    }
    /// Redo frames, oldest first; the last is the next redo.
    pub fn redo_frames(&self) -> std::collections::vec_deque::Iter<'_, Document> {
        self.redo.iter()
    }
    pub fn commit(&mut self, before: Document, after: &Document) -> bool {
        self.commit_continuing(before, after, false)
    }
    /// A continuous gesture keeps its first undo snapshot while updating the drawing live.
    pub fn commit_continuing(
        &mut self,
        before: Document,
        after: &Document,
        continuing: bool,
    ) -> bool {
        if before == *after {
            return false;
        }
        if !continuing || self.undo.is_empty() {
            self.undo.push_back(before);
        }
        self.redo.clear();
        if self.undo.len() > self.limit {
            let _ = self.undo.pop_front();
        }
        if self.max_bytes != usize::MAX {
            let mut total: usize = self.undo.iter().map(frame_bytes).sum();
            while total > self.max_bytes && self.undo.len() > Self::MIN_STEPS {
                total -= self.undo.pop_front().as_ref().map_or(0, frame_bytes);
            }
        }
        true
    }
    pub fn undo(&mut self, doc: &mut Document) -> bool {
        if let Some(prev) = self.undo.pop_back() {
            self.redo.push_back(std::mem::replace(doc, prev));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, doc: &mut Document) -> bool {
        if let Some(next) = self.redo.pop_back() {
            self.undo.push_back(std::mem::replace(doc, next));
            true
        } else {
            false
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}

#[cfg(test)]
mod tests;
