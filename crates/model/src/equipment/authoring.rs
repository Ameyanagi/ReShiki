//! A piece-editor draft. Creating, calibrating and previewing it never changes
//! the drawing. The app can clone a draft for its own Undo history and commit
//! [`Draft::accept`] as one drawing edit.
use super::{
    eval::{self, Values},
    format::*,
    geometry::{self as geom, Affine, Seg},
    placed, plan, revise, validate,
};
use crate::{
    document::{Document, Point},
    graphics::{Graphic, GraphicKind, LinePattern, PathCommand},
};
use std::{collections::BTreeMap, sync::Arc};

#[cfg(test)]
#[path = "authoring_tests.rs"]
mod tests;

#[path = "authoring_artwork.rs"]
mod artwork;

/// Two rim clicks measured in the draft's millimetres. An unambiguous normal
/// points away from filled shapes, independently of which edge was clicked first.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rim {
    pub at: Xy,
    pub dir: Xy,
    pub gap_mm: f64,
    /// Neither 1 mm probe enters a filled shape. Show the arrow and offer Flip.
    pub ambiguous: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JointKind {
    Taper {
        size: String,
        gender: Gender,
        draw: JointDraw,
    },
    Plug {
        family: PlugFamily,
        gender: Gender,
        size_mm: f64,
    },
}

impl JointKind {
    /// Physical distance between the two clicked edges. A socket's outer rim
    /// includes its walls; a cone's shoulder uses the nominal diameter.
    fn opening_mm(&self) -> Result<f64, String> {
        let width = match self {
            Self::Taper { size, gender, .. } => {
                let t = taper(size).ok_or_else(|| format!("Unknown taper size \"{size}\"."))?;
                match gender {
                    Gender::M => t.d,
                    Gender::F => t.d + 2. * t.socket_wall,
                }
            }
            Self::Plug { size_mm, .. } => *size_mm,
        };
        if !width.is_finite() || width <= 0. {
            return Err("Choose a positive opening size.".into());
        }
        Ok(width)
    }

    fn joint(&self, id: &str, rim: Rim, flip: bool) -> Joint {
        let dir = geom::mul(rim.dir, if flip { -1. } else { 1. });
        match self {
            Self::Taper { size, gender, draw } => Joint::Taper {
                id: id.into(),
                size: size.clone(),
                gender: *gender,
                at: rim.at,
                dir,
                part: None,
                poses: BTreeMap::new(),
                draw: *draw,
            },
            Self::Plug {
                family,
                gender,
                size_mm,
            } => Joint::Plug {
                id: id.into(),
                family: *family,
                gender: *gender,
                size_mm: *size_mm,
                at: rim.at,
                dir,
                part: None,
                poses: BTreeMap::new(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Draft {
    /// Editable geometry and metadata. `finish` chooses the saved identity,
    /// revision and preview format; placed copies elsewhere keep their revision.
    pub definition: Definition,
    frame: Affine,
    calibrated: bool,
    source: Vec<Graphic>,
    original: Option<Arc<Definition>>,
}

#[derive(Debug)]
pub struct Accepted {
    pub document: Document,
    pub piece_id: u64,
    pub definition: Arc<Definition>,
    /// Definition edits can release seats or reset parameter values. The app
    /// can show the same information as an ordinary library revision update.
    pub revision: revise::Outcome,
}

impl Draft {
    /// Selected native vector graphics, in drawing order, each become one
    /// shape. Groups are expanded; mixed selections and graphics with attached
    /// content are refused rather than silently dropping their content.
    pub fn from_selection(doc: &Document, selection: &[u64]) -> Result<Self, String> {
        let known = doc.all_ids();
        if selection.is_empty() || selection.iter().any(|id| !known.contains(id)) {
            return Err("Select the vector shapes for the equipment piece.".into());
        }
        let ids = doc.expand_groups(selection);
        if ids.len() > validate::MAX_SHAPES {
            return Err(format!(
                "Select at most {} shapes for one equipment piece.",
                validate::MAX_SHAPES
            ));
        }
        let source: Vec<_> = doc
            .graphics
            .iter()
            .filter(|g| ids.contains(&g.id))
            .collect();
        if source.len() != ids.len() {
            return Err("Select only vector shapes for an equipment piece.".into());
        }
        let mut count = 0;
        for g in &source {
            supported(g)?;
            count += g.commands().len();
            if count > validate::MAX_PATH_COMMANDS {
                return Err(format!(
                    "The selected shapes exceed {} path commands. Simplify them first.",
                    validate::MAX_PATH_COMMANDS
                ));
            }
        }
        let Some(first) = source.first() else {
            return Err("Select the vector shapes for the equipment piece.".into());
        };
        if source.iter().any(|g| g.layer != first.layer) {
            return Err("Put the selected shapes on the same drawing layer first.".into());
        }
        let points: Vec<_> = source
            .iter()
            .flat_map(|g| g.commands())
            .flat_map(|c| c.points())
            .collect();
        let lo = points.iter().fold([f64::INFINITY; 2], |a, p| {
            [a[0].min(f64::from(p.x)), a[1].min(f64::from(p.y))]
        });
        let hi = points.iter().fold([f64::NEG_INFINITY; 2], |a, p| {
            [a[0].max(f64::from(p.x)), a[1].max(f64::from(p.y))]
        });
        if !lo.iter().chain(&hi).all(|v| v.is_finite()) || lo == hi {
            return Err("Draw shapes with some size before defining a piece.".into());
        }
        let scale = f64::from(placed::world_per_mm(doc, placed::DEFAULT_PT_PER_MM));
        let frame = Affine([
            scale,
            0.,
            0.,
            scale,
            (lo[0] + hi[0]) / 2.,
            (lo[1] + hi[1]) / 2.,
        ]);
        let inverse = frame
            .inverse()
            .ok_or("The piece needs a positive drawing scale.")?;
        let shapes = source
            .iter()
            .map(|g| {
                let closed = g.closed_shape() && g.style.fill.is_some();
                Shape {
                    id: format!("shape-{}", g.id),
                    layer: Layer::Body,
                    depth: 0,
                    material: if closed {
                        Material::Glass
                    } else {
                        Material::Ink
                    },
                    color: None,
                    role: if closed {
                        Role::Glass
                    } else if (g.style.width_pt - 0.6).abs() < 1e-6 {
                        Role::Outline
                    } else {
                        Role::Detail
                    },
                    style: None,
                    path: path(&inverse.map(&commands(&g.commands()))),
                    part: None,
                    when: BTreeMap::new(),
                }
            })
            .collect();
        let definition = Definition {
            format: FORMAT.into(),
            format_version: PREVIEW_VERSION,
            id: "user/new-piece".into(),
            revision: 1,
            family: "user-piece".into(),
            variant: BTreeMap::new(),
            names: BTreeMap::from([("en".into(), "New equipment piece".into())]),
            category: Category::Other,
            keywords: vec![],
            credits: Credits::default(),
            units: Units::Mm,
            grounded: false,
            clip: None,
            tube: None,
            shapes,
            joints: vec![],
            holds: vec![],
            params: vec![],
            parts: vec![],
            liquids: vec![],
            labels: vec![],
        };
        validate::validate(&definition)?;
        Ok(Self {
            definition,
            frame,
            calibrated: false,
            source: source.into_iter().cloned().collect(),
            original: None,
        })
    }

    /// Edit one placed definition on a copy. Built-ins must be saved under a
    /// user ID; `finish` bumps an existing user ID's revision exactly once.
    pub fn from_piece(doc: &Document, id: u64) -> Result<Self, String> {
        let g = doc
            .graphics
            .iter()
            .find(|g| g.id == id)
            .ok_or("Select an equipment piece to edit.")?;
        g.validate()?;
        let original = g
            .piece
            .as_ref()
            .and_then(|p| p.linked.clone())
            .ok_or("The equipment definition is missing.")?;
        validate::validate(&original)?;
        Ok(Self {
            definition: (*original).clone(),
            frame: frame(g),
            calibrated: true,
            source: vec![g.clone()],
            original: Some(original),
        })
    }

    pub fn frame(&self) -> Affine {
        self.frame
    }
    pub fn calibrated(&self) -> bool {
        self.calibrated
    }

    /// The first confirmed measurement sets the physical scale, preserving
    /// every source curve's world coordinates. Calibrate before defining
    /// joints, holds or movable parts, whose sizes already have meaning in mm.
    pub fn calibrate(&mut self, edges: [Point; 2], width_mm: f64) -> Result<(), String> {
        if !(width_mm.is_finite() && width_mm > 0.) {
            return Err("Choose a positive opening size.".into());
        }
        if self.original.is_some() {
            return Err("An existing definition already uses millimeters. Edit its geometry to change its physical size.".into());
        }
        if !self.definition.joints.is_empty()
            || !self.definition.holds.is_empty()
            || !self.definition.params.is_empty()
            || !self.definition.parts.is_empty()
            || self.definition.clip.is_some()
            || self.definition.tube.is_some()
        {
            return Err("Set the scale before adding joints, holds or movable parts.".into());
        }
        let (a, b) = self.edges(edges)?;
        let gap = geom::len(geom::sub(b, a));
        if self.calibrated && (gap / width_mm - 1.).abs() <= 1e-12 {
            return Ok(());
        }
        let k = width_mm / gap;
        let scale = Affine::scale(k, [0., 0.]);
        let mut next = self.clone();
        for s in &mut next.definition.shapes {
            s.path = path(&scale.map(&geom::parse_path(&s.path)?));
        }
        for l in &mut next.definition.liquids {
            l.region = path(&scale.map(&geom::parse_path(&l.region)?));
        }
        for l in &mut next.definition.labels {
            l.at = scale.apply(l.at);
        }
        next.frame = self.frame.then(Affine::scale(1. / k, [0., 0.]));
        let pt_per_mm =
            next.frame.scale_factor() * f64::from(crate::style::DEFAULT.points_per_world());
        if !(f64::from(placed::MIN_PT_PER_MM)..=f64::from(placed::MAX_PT_PER_MM))
            .contains(&pt_per_mm)
        {
            return Err(format!(
                "Choose a scale between {} and {} pt per mm.",
                placed::MIN_PT_PER_MM,
                placed::MAX_PT_PER_MM
            ));
        }
        next.calibrated = true;
        validate::validate(&next.definition)?;
        *self = next;
        Ok(())
    }

    fn edges(&self, edges: [Point; 2]) -> Result<(Xy, Xy), String> {
        let inverse = self
            .frame
            .inverse()
            .ok_or("The piece needs a positive drawing scale.")?;
        let [a, b] = edges.map(|p| inverse.apply([f64::from(p.x), f64::from(p.y)]));
        if !a.iter().chain(&b).all(|n| n.is_finite()) || geom::len(geom::sub(a, b)) < 1e-6 {
            return Err("Click two different edges of the opening.".into());
        }
        Ok((a, b))
    }

    pub fn rim(&self, edges: [Point; 2]) -> Result<Rim, String> {
        let (a, b) = self.edges(edges)?;
        let at = geom::mul(geom::add(a, b), 0.5);
        let mut dir = geom::normalize(geom::perp(geom::sub(b, a)));
        let evaluated = eval::evaluate(&self.definition, &Values::new())?;
        let filled: Vec<_> = evaluated
            .shapes
            .iter()
            .filter(|s| s.shape.role.filled())
            .map(|s| geom::flatten(&s.path))
            .collect();
        let inside = |p| filled.iter().any(|outline| geom::inside(p, outline));
        let forward = inside(geom::add(at, dir));
        let back = inside(geom::sub(at, dir));
        if forward && back {
            return Err(
                "Both directions point into the body. Click the rim of the opening.".into(),
            );
        }
        if forward {
            dir = geom::mul(dir, -1.);
        }
        Ok(Rim {
            at,
            dir,
            gap_mm: geom::len(geom::sub(b, a)),
            ambiguous: !forward && !back,
        })
    }

    /// Later openings suggest sizes without changing the calibration. Ties
    /// keep the standard size table's order (e.g. the two 14 mm lengths).
    pub fn taper_sizes(
        &self,
        edges: [Point; 2],
        gender: Gender,
    ) -> Result<Vec<&'static TaperSize>, String> {
        if !self.calibrated {
            return Err("Confirm an opening size to set the scale first.".into());
        }
        let gap = self.rim(edges)?.gap_mm;
        let mut sizes: Vec<_> = TAPER_SIZES
            .iter()
            .filter_map(|t| {
                let width = if gender == Gender::F {
                    t.d + 2. * t.socket_wall
                } else {
                    t.d
                };
                let error = (gap / width - 1.).abs();
                (error <= 0.15).then_some((t, error))
            })
            .collect();
        sizes.sort_by(|a, b| a.1.total_cmp(&b.1));
        Ok(sizes.into_iter().map(|(t, _)| t).collect())
    }

    /// Confirm an opening and append a real joint. The first confirmation
    /// calibrates the whole draft; subsequent gaps must agree within 15%.
    /// Any geometry/ID/attachment error leaves the draft unchanged.
    pub fn add_joint(
        &mut self,
        id: &str,
        edges: [Point; 2],
        kind: JointKind,
        flip: bool,
    ) -> Result<Joint, String> {
        let mut next = self.clone();
        let width = kind.opening_mm()?;
        if !next.calibrated {
            next.calibrate(edges, width)?;
        }
        let rim = next.rim(edges)?;
        if (rim.gap_mm / width - 1.).abs() > 0.15 {
            return Err(format!(
                "The opening is {:.1} mm wide; this joint needs {:.1} mm. Choose another size or redraw the opening.",
                rim.gap_mm, width
            ));
        }
        let joint = kind.joint(id, rim, flip);
        next.definition.joints.push(joint.clone());
        validate::validate(&next.definition)?;
        *self = next;
        Ok(joint)
    }

    /// A live ghost using the same evaluator and paint plan as placed pieces.
    pub fn preview(&self) -> Result<Vec<plan::PlanPath>, String> {
        validate::validate(&self.definition)?;
        let mut instance = plan::Instance::new(&self.definition, self.frame);
        if let Some(piece) = self.source.first().and_then(|g| g.piece.as_ref()) {
            instance.centre_line = piece.centre_line.clone();
        }
        plan::paint_plan(&[instance], plan::Settings::default())
    }

    /// A temporary piece in the draft's default pose, for ordinary hit tests,
    /// joint overlays and `solve::ports` / `solve::snap` in a preview drawing.
    pub fn preview_graphic(&self, id: u64) -> Result<Graphic, String> {
        validate::validate(&self.definition)?;
        let mut graphic = self.graphic_with(id, Arc::new(self.definition.clone()));
        if let (Some(piece), Some(draft)) = (
            self.source.first().and_then(|g| g.piece.as_ref()),
            graphic.piece.as_mut(),
        ) {
            draft.centre_line = piece.centre_line.clone();
        }
        graphic.validate()?;
        Ok(graphic)
    }

    fn graphic_with(&self, id: u64, definition: Arc<Definition>) -> Graphic {
        let mut graphic = placed::graphic(id, definition, Point::default(), 1.);
        let [a, b, c, d, e, f] = self.frame.0;
        graphic.origin = Point::new(e as f32, f as f32);
        graphic.axis_x = Point::new(a as f32, b as f32);
        graphic.axis_y = Point::new(c as f32, d as f32);
        if let Some(source) = self.source.first() {
            graphic.layer = source.layer;
        }
        graphic
    }

    /// A new user identity starts at revision 1. Saving the same user identity
    /// advances its original revision, not a previous preview's revision.
    pub fn finish(&self, id: &str) -> Result<Arc<Definition>, String> {
        if !self.calibrated {
            return Err(
                "Confirm an opening size or a measured length to set the scale first.".into(),
            );
        }
        if id.starts_with("reshiki/") {
            return Err("Save a built-in piece as a new piece in My pieces.".into());
        }
        let mut definition = self.definition.clone();
        definition.id = id.into();
        definition.revision = match self.original.as_ref().filter(|d| d.id == id) {
            Some(d) => d
                .revision
                .checked_add(1)
                .ok_or("The piece revision cannot be increased further.")?,
            None => 1,
        };
        definition.format_version = PREVIEW_VERSION;
        validate::validate(&definition)?;
        Ok(Arc::new(definition))
    }

    /// Produce the one document edit to accept. Reject a changed source; keep
    /// unrelated edits made while the draft was open. Saving a library file
    /// and recording one Undo frame belong to the app.
    pub fn accept(&self, doc: &Document, id: &str) -> Result<Accepted, String> {
        if self
            .source
            .iter()
            .any(|source| doc.graphics.iter().find(|g| g.id == source.id) != Some(source))
        {
            return Err(
                "The source shapes changed. Close the editor and select them again.".into(),
            );
        }
        let definition = self.finish(id)?;
        let mut document = doc.clone();
        let (piece_id, revision) = if self.original.is_some() {
            let source = self.source.first().ok_or("The source piece is missing.")?;
            let revision = revise::update(&mut document, &[source.id], &definition);
            (source.id, revision)
        } else {
            let ids: Vec<_> = self.source.iter().map(|g| g.id).collect();
            let index = document
                .graphics
                .iter()
                .position(|g| ids.contains(&g.id))
                .ok_or("The source shapes are missing.")?;
            let piece_id = document.next_id();
            let graphic = self.graphic_with(piece_id, definition.clone());
            document.graphics.insert(index, graphic);
            // Keep object links and enclosing groups on the replacement before
            // deletion prunes the consumed shapes. Chemical targets keep their
            // atom and bond meanings.
            for graphic in &mut document.graphics {
                if let Some(leader) = &mut graphic.leader {
                    for target in [&mut leader.start, &mut leader.end].into_iter().flatten() {
                        if let crate::leaders::Target::Object { id } = target
                            && ids.contains(id)
                        {
                            *id = piece_id;
                        }
                    }
                }
            }
            for group in &mut document.groups {
                for member in &mut group.members {
                    if ids.contains(member) {
                        *member = piece_id;
                    }
                }
                group.members.sort_unstable();
                group.members.dedup();
            }
            document.delete(&ids);
            (piece_id, revise::Outcome::default())
        };
        document.piece_definitions = placed::used(&document);
        document.validate()?;
        Ok(Accepted {
            document,
            piece_id,
            definition,
            revision,
        })
    }
}

fn supported(g: &Graphic) -> Result<(), String> {
    if g.kind == GraphicKind::Picture {
        return Err("Equipment pieces need vector shapes. A picture cannot become a piece.".into());
    }
    if !GraphicKind::DRAWABLE.contains(&g.kind) {
        return Err(
            "Select native vector shapes, or use Edit definition for an equipment piece.".into(),
        );
    }
    g.validate()?;
    if g.depth != [0.; 3]
        || g.panel.is_some()
        || g.cycle.is_some()
        || g.leader.is_some()
        || g.text.is_some()
        || g.superscript.is_some()
        || g.heads.is_some()
    {
        return Err("Use plain 2D shapes without attached text, leaders or arrowheads.".into());
    }
    if g.style.pattern != LinePattern::Solid || !g.style.effects.is_plain() {
        return Err("Use plain fills and solid strokes before defining an equipment piece.".into());
    }
    if let Some(problem) = stroke_problem(g) {
        return Err(problem.into());
    }
    Ok(())
}

/// A visible stroke the fixed-piece paint model cannot preserve. Filled SVG
/// artwork uses a minimum-width stroke in its fill color, not a separate outline.
pub fn stroke_problem(g: &Graphic) -> Option<&'static str> {
    if g.kind == GraphicKind::Path
        && g.style.fill == Some(g.style.stroke)
        && (g.style.width_pt - 0.1).abs() < 1e-6
    {
        return (!artwork::certified(&g.commands())).then_some(
            "Use simpler filled paths, or Ink strokes: 0.6 pt outlines / 0.4 pt details.",
        );
    }
    if g.style.stroke != crate::palette::Color::Ink {
        return Some("Use Ink strokes before defining an equipment piece.");
    }
    if g.closed_shape() && g.style.fill.is_some() {
        ((g.style.width_pt - 0.6).abs() > 1e-6)
            .then_some("Use 0.6 pt outlines on filled equipment shapes.")
    } else {
        ((g.style.width_pt - 0.4).abs() > 1e-6 && (g.style.width_pt - 0.6).abs() > 1e-6)
            .then_some("Use 0.4 pt details or 0.6 pt outlines for equipment.")
    }
}

fn frame(g: &Graphic) -> Affine {
    Affine([
        f64::from(g.axis_x.x),
        f64::from(g.axis_x.y),
        f64::from(g.axis_y.x),
        f64::from(g.axis_y.y),
        f64::from(g.origin.x),
        f64::from(g.origin.y),
    ])
}

fn commands(commands: &[PathCommand]) -> Vec<Seg> {
    let xy = |p: Point| [f64::from(p.x), f64::from(p.y)];
    commands
        .iter()
        .map(|c| match *c {
            PathCommand::Move(p) => Seg::Move(xy(p)),
            PathCommand::Line(p) => Seg::Line(xy(p)),
            PathCommand::Cubic(a, b, p) => Seg::Cubic(xy(a), xy(b), xy(p)),
            PathCommand::Close => Seg::Close,
        })
        .collect()
}

fn path(commands: &[Seg]) -> String {
    commands
        .iter()
        .map(|c| match *c {
            Seg::Move([x, y]) => format!("M{x} {y}"),
            Seg::Line([x, y]) => format!("L{x} {y}"),
            Seg::Cubic([a, b], [c, d], [x, y]) => format!("C{a} {b} {c} {d} {x} {y}"),
            Seg::Close => "Z".into(),
        })
        .collect()
}
