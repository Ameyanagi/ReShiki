//! Pieces placed in a drawing. A graphic of kind `Piece` holds a reference to
//! a definition the drawing embeds, its parameter values, and a similarity
//! frame in world units per millimetre. Drawings carry their pieces, so they
//! open the same way without the library they came from.
use super::eval::{Values, resolve};
use super::format::{Definition, Material, StyleKind, content_hash};
use super::geometry::{Affine, Seg};
use super::plan::{Ink, Instance, Pass, PlanPath, Settings, Weight, paint_plan};
use crate::document::{Document, Point};
use crate::graphics::{Graphic, GraphicKind, GraphicStyle, PathCommand};
use crate::palette::Color;
use crate::scene::Primitive;
use crate::scientific::Part;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// Embedded definitions per drawing.
pub const MAX_DEFINITIONS: usize = 512;
/// New pieces are drawn at 0.4 pt per millimetre: a 250 mL flask is about 34 pt wide.
pub const DEFAULT_PT_PER_MM: f32 = 0.4;
pub const MIN_PT_PER_MM: f32 = 0.05;
pub const MAX_PT_PER_MM: f32 = 5.;

/// A placed piece: which definition, and its parameter values.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PieceRef {
    /// `id@revision#hash`, naming one of the drawing's embedded definitions.
    pub definition: String,
    #[serde(default, skip_serializing_if = "Values::is_empty")]
    pub params: Values,
    /// The definition itself, linked by key when the drawing is read or the
    /// piece is placed.
    #[serde(skip)]
    pub linked: Option<Arc<Definition>>,
}

/// The key names the definition completely, so the link adds nothing.
impl PartialEq for PieceRef {
    fn eq(&self, other: &Self) -> bool {
        self.definition == other.definition && self.params == other.params
    }
}

impl PieceRef {
    pub fn new(definition: Arc<Definition>) -> Self {
        Self {
            definition: key(&definition),
            params: Values::new(),
            linked: Some(definition),
        }
    }
}

/// `id@revision#` and the first 16 hex digits of the content hash.
pub fn key(definition: &Definition) -> String {
    format!(
        "{}@{}#{}",
        definition.id,
        definition.revision,
        &content_hash(definition)[..16]
    )
}

/// World units per millimetre for `pt_per_mm` in the drawing's style.
pub fn world_per_mm(doc: &Document, pt_per_mm: f32) -> f32 {
    pt_per_mm / doc.drawing_style.points_per_world()
}

/// A new piece graphic with its frame centred on `center`.
pub fn graphic(id: u64, definition: Arc<Definition>, center: Point, world_per_mm: f32) -> Graphic {
    let mut g = Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        center,
        center,
        GraphicStyle::default(),
        Default::default(),
        false,
    );
    g.kind = GraphicKind::Piece;
    g.origin = center;
    g.axis_x = Point::new(world_per_mm, 0.);
    g.axis_y = Point::new(0., world_per_mm);
    g.piece = Some(PieceRef::new(definition));
    // Centre the drawn piece, not its millimetre origin, on the point.
    let (lo, hi) = g.shape_bounds();
    let mid = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    g.origin = Point::new(2. * center.x - mid.x, 2. * center.y - mid.y);
    g
}

/// Link every placed piece to its embedded definition, after checking each one.
pub fn link(doc: &mut Document) -> Result<(), String> {
    if doc.piece_definitions.len() > MAX_DEFINITIONS {
        return Err(format!(
            "The drawing embeds {} equipment pieces; the limit is {MAX_DEFINITIONS}.",
            doc.piece_definitions.len()
        ));
    }
    let mut keyed = Vec::with_capacity(doc.piece_definitions.len());
    for d in &doc.piece_definitions {
        super::validate::validate(d).map_err(|e| format!("Equipment piece {}: {e}", d.id))?;
        keyed.push((key(d), d.clone()));
    }
    for g in &mut doc.graphics {
        if let Some(piece) = g.piece.as_mut() {
            let Some((_, d)) = keyed.iter().find(|(k, _)| *k == piece.definition) else {
                return Err(format!(
                    "The drawing uses the equipment piece {} but does not include it.",
                    piece.definition
                ));
            };
            piece.linked = Some(d.clone());
        }
    }
    Ok(())
}

/// The definitions the drawing's pieces use, once each, in drawing order.
pub fn used(doc: &Document) -> Vec<Arc<Definition>> {
    let mut out: Vec<Arc<Definition>> = Vec::new();
    for d in doc
        .graphics
        .iter()
        .filter_map(|g| g.piece.as_ref()?.linked.as_ref())
    {
        if !out.iter().any(|o| Arc::ptr_eq(o, d) || **o == **d) {
            out.push(d.clone());
        }
    }
    out
}

/// A piece graphic is a linked piece in a similarity frame within the scale limits.
pub(crate) fn validate(g: &Graphic) -> Result<(), String> {
    let Some(piece) = &g.piece else {
        return Ok(());
    };
    let Some(d) = &piece.linked else {
        return Err(format!(
            "The equipment piece {} is not in the drawing.",
            piece.definition
        ));
    };
    resolve(d, &piece.params)?;
    let (x, y) = (g.axis_x, g.axis_y);
    let (sx, sy) = (x.distance(Point::default()), y.distance(Point::default()));
    let dot = x.x * y.x + x.y * y.y;
    let pt = crate::style::DEFAULT.points_per_world();
    if (sx - sy).abs() > sx * 1e-3 || dot.abs() > sx * sy * 1e-3 {
        return Err(
            "An equipment piece can be moved, turned, scaled and mirrored, not stretched or skewed"
                .into(),
        );
    }
    if !(MIN_PT_PER_MM * 0.999..=MAX_PT_PER_MM * 1.001).contains(&(sx * pt)) {
        return Err(format!(
            "An equipment piece is drawn at {MIN_PT_PER_MM}–{MAX_PT_PER_MM} pt per mm"
        ));
    }
    if !g.path.is_empty()
        || g.heads.is_some()
        || g.panel.is_some()
        || g.arc.is_some()
        || g.picture.is_some()
        || g.leader.is_some()
        || g.cycle.is_some()
        || g.superscript.is_some()
    {
        return Err("An equipment piece holds no other shape settings".into());
    }
    Ok(())
}

fn transform(g: &Graphic) -> Affine {
    let f = |v: f32| f64::from(v);
    Affine([
        f(g.axis_x.x),
        f(g.axis_x.y),
        f(g.axis_y.x),
        f(g.axis_y.y),
        f(g.origin.x),
        f(g.origin.y),
    ])
}

type CacheKey = (String, String, [u32; 6]);
const CACHE_SIZE: usize = 64;
thread_local! {
    /// Recent paint plans: hovering and hit testing ask for the same pieces again and again.
    static PLANS: RefCell<Vec<(CacheKey, Rc<Vec<PlanPath>>)>> = const { RefCell::new(Vec::new()) };
}

/// The piece's paint plan in world units, in line style; empty for any other graphic.
pub fn plan(g: &Graphic) -> Rc<Vec<PlanPath>> {
    let Some((piece, d)) = g.piece.as_ref().and_then(|p| Some((p, p.linked.as_ref()?))) else {
        return Rc::default();
    };
    let frame = [g.axis_x, g.axis_y, g.origin];
    let bits = [0, 1, 2]
        .map(|i| frame[i])
        .map(|p| [p.x.to_bits(), p.y.to_bits()]);
    let key: CacheKey = (
        piece.definition.clone(),
        serde_json::to_string(&piece.params).unwrap_or_default(),
        [
            bits[0][0], bits[0][1], bits[1][0], bits[1][1], bits[2][0], bits[2][1],
        ],
    );
    if let Some(hit) = PLANS.with_borrow_mut(|cache| {
        let i = cache.iter().position(|(k, _)| *k == key)?;
        let entry = cache.remove(i);
        let plan = entry.1.clone();
        cache.insert(0, entry);
        Some(plan)
    }) {
        return hit;
    }
    let mut instance = Instance::new(d, transform(g));
    instance.values = piece.params.clone();
    let plan: Vec<PlanPath> = paint_plan(&[instance], Settings::default())
        .unwrap_or_default()
        .into_iter()
        .filter(|p| p.style != Some(StyleKind::Shaded))
        .collect();
    let plan = Rc::new(plan);
    PLANS.with_borrow_mut(|cache| {
        cache.insert(0, (key, plan.clone()));
        cache.truncate(CACHE_SIZE);
    });
    plan
}

fn commands(path: &[Seg]) -> Vec<PathCommand> {
    let p = |x: [f64; 2]| Point::new(x[0] as f32, x[1] as f32);
    path.iter()
        .map(|s| match *s {
            Seg::Move(a) => PathCommand::Move(p(a)),
            Seg::Line(a) => PathCommand::Line(p(a)),
            Seg::Cubic(a, b, c) => PathCommand::Cubic(p(a), p(b), p(c)),
            Seg::Close => PathCommand::Close,
        })
        .collect()
}

/// Stroke widths in points: outlines stay 0.6 pt however a setup is scaled.
fn width_pt(weight: Weight, pass: Pass) -> f32 {
    match weight {
        Weight::Outline if pass == Pass::Outline => 1.2,
        Weight::Outline => 0.6,
        Weight::Detail => 0.4,
        Weight::Hatch => 0.25,
    }
}

/// The colour a material is filled with in line style, on the light canvas.
/// The dark canvas shows it through the usual lightness change.
fn fill_rgb(material: Material, color: Option<[u8; 3]>) -> Color {
    if let Some(c) = color {
        return Color::Custom(c);
    }
    Color::Custom(match material {
        Material::Glass | Material::Frosted | Material::Paper => [255; 3],
        Material::Ptfe => [251, 251, 248],
        Material::Rubber => [138, 47, 42],
        Material::Plastic => [236, 238, 240],
        Material::Metal => [215, 220, 224],
        Material::Porcelain => [247, 243, 234],
        Material::Cork => [201, 164, 107],
        Material::Silica => [239, 233, 220],
        Material::Sand => [230, 213, 168],
        Material::Liquid => super::eval::DEFAULT_LIQUID,
        Material::Ink => return Color::Ink,
    })
}

/// Hit-test and bounds geometry: the plan's paths with their stroke widths.
pub(crate) fn parts(g: &Graphic) -> Vec<Part> {
    plan(g)
        .iter()
        .map(|p| {
            let (width, filled) = match p.ink {
                Ink::Fill => (0., true),
                Ink::Stroke(w) => (width_pt(w, p.pass), false),
                Ink::FillStroke(w) => (width_pt(w, p.pass), true),
            };
            Part {
                commands: commands(&p.path),
                style: GraphicStyle {
                    width_pt: width.max(0.1),
                    fill: filled.then_some(Color::Ink),
                    ..GraphicStyle::default()
                },
                filled,
            }
        })
        .collect()
}

/// What the piece paints, in light-canvas colours like every other primitive.
pub(crate) fn primitives(g: &Graphic) -> Vec<Primitive> {
    plan(g)
        .iter()
        .map(|p| {
            let fill = fill_rgb(p.material, p.color);
            let stroke = if p.material == Material::Liquid {
                fill
            } else {
                Color::Ink
            };
            let (width_pt, filled) = match p.ink {
                Ink::Fill => (0., true),
                Ink::Stroke(w) => (width_pt(w, p.pass), false),
                Ink::FillStroke(w) => (width_pt(w, p.pass), true),
            };
            let path = Primitive::Path {
                commands: commands(&p.path),
                style: GraphicStyle {
                    stroke,
                    fill: filled.then_some(fill),
                    width_pt,
                    ..GraphicStyle::default()
                },
                filled,
            };
            let alpha = match (p.material, p.ink) {
                (Material::Liquid, Ink::Fill) => Some(0.28),
                (_, Ink::Stroke(Weight::Hatch)) => Some(0.75),
                _ => None,
            };
            match alpha {
                Some(alpha) => Primitive::Opacity {
                    alpha,
                    primitive: Box::new(path),
                },
                None => path,
            }
        })
        .collect()
}
