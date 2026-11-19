//! Pieces placed in a drawing. A graphic of kind `Piece` holds a reference to
//! a definition the drawing embeds, its parameter values, and a similarity
//! frame in world units per millimetre. Drawings carry their pieces, so they
//! open the same way without the library they came from.
use super::eval::{Values, resolve};
use super::format::{Definition, Material, StyleKind, content_hash};
use super::geometry::{Affine, Seg};
use super::plan::{Ink, Instance, Pass, PlanPath, Settings, Weight, paint_plan};
use crate::canvas_theme::CanvasTheme;
use crate::document::{Document, Point};
use crate::graphics::{Graphic, GraphicKind, GraphicStyle, PathCommand};
use crate::palette::Color;
use crate::scene::Primitive;
use crate::scientific::Part;
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeSet;
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
    /// Joints with a partner seated in them, kept from the drawing's seats.
    #[serde(skip)]
    pub seated: BTreeSet<String>,
}

/// The key names the definition completely, so the link adds nothing; the
/// drawing's seats say which joints are seated.
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
            seated: BTreeSet::new(),
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
    g.piece = Some(Box::new(PieceRef::new(definition)));
    // Centre the drawn piece, not its millimetre origin, on the point.
    let mid = middle(&g);
    g.origin = Point::new(2. * center.x - mid.x, 2. * center.y - mid.y);
    g
}

/// The middle of the drawn piece, fixed to the piece as it turns, so turning
/// one way and back returns it to where it was.
pub fn middle(g: &Graphic) -> Point {
    let mut upright = g.clone();
    upright.origin = Point::default();
    upright.axis_x = Point::new(1., 0.);
    upright.axis_y = Point::new(0., 1.);
    let (lo, hi) = upright.shape_bounds();
    let (x, y) = ((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
    g.origin.offset(
        g.axis_x.x * x + g.axis_y.x * y,
        g.axis_x.y * x + g.axis_y.y * y,
    )
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

/// After a transform a piece cannot follow: it never stretches, so it keeps
/// its frame, and it stops at the scale limits; either way its middle goes
/// where the transform takes it.
pub(crate) fn settle(g: &mut Graphic, before: Graphic, convert: impl Fn(Point) -> Point) {
    let to = convert(middle(&before));
    let (x, y) = (g.axis_x, g.axis_y);
    let (sx, sy) = (x.distance(Point::default()), y.distance(Point::default()));
    let similar = (sx - sy).abs() <= sx * 1e-3 && (x.x * y.x + x.y * y.y).abs() <= sx * sy * 1e-3;
    let pt = crate::style::DEFAULT.points_per_world();
    let (axis_x, axis_y) = if similar && sx > 0. {
        let k = (sx * pt).clamp(MIN_PT_PER_MM, MAX_PT_PER_MM) / (sx * pt);
        (Point::new(x.x * k, x.y * k), Point::new(y.x * k, y.y * k))
    } else {
        (before.axis_x, before.axis_y)
    };
    *g = before;
    g.axis_x = axis_x;
    g.axis_y = axis_y;
    let mid = middle(g);
    g.origin = g.origin.offset(to.x - mid.x, to.y - mid.y);
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

type CacheKey = (String, String, [u32; 6], BTreeSet<String>);
const CACHE_SIZE: usize = 256;
thread_local! {
    /// Recent paint plans: hovering and hit testing ask for the same pieces again and again.
    static PLANS: RefCell<Vec<(CacheKey, Rc<Vec<PlanPath>>)>> = const { RefCell::new(Vec::new()) };
}

/// The piece's paint plan in world units, in line style; empty for any other graphic.
pub fn plan(g: &Graphic) -> Rc<Vec<PlanPath>> {
    let Some((piece, d)) = g.piece.as_ref().and_then(|p| Some((p, p.linked.as_ref()?))) else {
        return Rc::default();
    };
    let [x, y, o] = [g.axis_x, g.axis_y, g.origin];
    let key: CacheKey = (
        piece.definition.clone(),
        serde_json::to_string(&piece.params).unwrap_or_default(),
        [x.x, x.y, y.x, y.y, o.x, o.y].map(f32::to_bits),
        piece.seated.clone(),
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
    instance.seated = piece.seated.clone();
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

/// The colour a material is filled with in line style, in the light-canvas
/// form primitives use. Glass is paper and flips with the canvas; rubber,
/// plastic and other coloured parts keep their colour on the dark canvas.
fn fill_rgb(material: Material, color: Option<[u8; 3]>, theme: CanvasTheme) -> Color {
    let kept = |c: [u8; 3]| Color::Custom(theme.color(c));
    match material {
        Material::Glass | Material::Frosted | Material::Paper => {
            Color::Custom(color.unwrap_or([255; 3]))
        }
        Material::Liquid => Color::Custom(color.unwrap_or(super::eval::DEFAULT_LIQUID)),
        Material::Ink => Color::Ink,
        Material::Ptfe => kept(color.unwrap_or([251, 251, 248])),
        Material::Rubber => kept(color.unwrap_or([138, 47, 42])),
        Material::Plastic => kept(color.unwrap_or([236, 238, 240])),
        Material::Metal => kept(color.unwrap_or([215, 220, 224])),
        Material::Porcelain => kept(color.unwrap_or([247, 243, 234])),
        Material::Cork => kept(color.unwrap_or([201, 164, 107])),
        Material::Silica => kept(color.unwrap_or([239, 233, 220])),
        Material::Sand => kept(color.unwrap_or([230, 213, 168])),
    }
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

/// What the piece paints on `theme`, in light-canvas colours like every other primitive.
pub fn primitives(g: &Graphic, theme: CanvasTheme) -> Vec<Primitive> {
    plan(g)
        .iter()
        .map(|p| {
            let fill = fill_rgb(p.material, p.color, theme);
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
            // A see-through piece, such as the placement ghost.
            let alpha = match (alpha, g.style.effects.opacity) {
                (Some(a), Some(b)) => Some(a * b),
                (a, b) => a.or(b),
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

/// The drawing as ChemDraw can keep it: each piece becomes a group of
/// filled and stroked curves in paint order, in the colours the canvas shows.
pub fn materialized(doc: &Document) -> std::borrow::Cow<'_, Document> {
    if !doc.graphics.iter().any(|g| g.piece.is_some()) {
        return std::borrow::Cow::Borrowed(doc);
    }
    let shown = |c: Color| match c {
        Color::Custom(rgb) => Color::Custom(doc.canvas_theme.color(rgb)),
        other => other,
    };
    let mut out = doc.clone();
    let mut next = doc.next_id();
    out.graphics.clear();
    for g in &doc.graphics {
        if g.piece.is_none() {
            out.graphics.push(g.clone());
            continue;
        }
        let mut members = Vec::new();
        for primitive in primitives(g, doc.canvas_theme) {
            let (alpha, primitive) = match primitive {
                Primitive::Opacity { alpha, primitive } => (Some(alpha), *primitive),
                p => (None, p),
            };
            let Primitive::Path {
                commands,
                mut style,
                filled,
            } = primitive
            else {
                continue;
            };
            style.stroke = shown(style.stroke);
            style.fill = style.fill.map(shown);
            if style.width_pt == 0. {
                // ChemDraw strokes every curve: a hairline in the fill's colour.
                style.width_pt = 0.1;
                style.stroke = style.fill.unwrap_or(style.stroke);
            }
            style.effects.opacity = alpha;
            let mut shape =
                Graphic::pen_curve(next, Point::default(), Point::new(1., 0.), style.clone());
            shape.style = style;
            shape.path = commands;
            shape.node_types.clear();
            shape.layer = g.layer;
            if !filled {
                shape.style.fill = None;
            }
            out.graphics.push(shape);
            members.push(next);
            next += 1;
        }
        for group in out
            .groups
            .iter_mut()
            .filter(|group| group.members.contains(&g.id))
        {
            group.members.retain(|id| *id != g.id);
            group.members.extend(&members);
        }
        out.groups.push(crate::grouping::Group {
            id: next,
            members,
            integral: false,
            layout: None,
        });
        next += 1;
    }
    out.prune_groups();
    std::borrow::Cow::Owned(out)
}
