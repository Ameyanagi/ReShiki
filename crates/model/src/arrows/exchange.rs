//! Arrow looks that ChemDraw cannot store, written as ordinary grouped shapes
//! for editable exchange. Block, hollow and gradient bodies on straight arrows
//! and circular arcs have native ChemDraw forms and stay arrows.
use super::{Arrow, ArrowPath, Body};
use crate::{
    document::{Annotation, Document},
    graphics::{Graphic, GraphicStyle, PathCommand},
    grouping::Group,
};
use std::borrow::Cow;

/// What the exchange copy turns into shapes, for the export notice.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Lowered {
    /// Chevron bodies and broad bodies on curved arrows.
    pub bodies: usize,
    pub badges: usize,
}

impl Arrow {
    /// A broad body ChemDraw has no arrow for: chevrons, or a body on a curve.
    fn shape_body(&self) -> bool {
        let body = self.drawn_body();
        body == Body::Chevron || !body.is_line() && self.bezier_controls().is_some()
    }
}

/// How many arrow looks an editable exchange copy writes as shapes.
pub fn lowered_count(doc: &Document) -> Lowered {
    Lowered {
        bodies: doc.arrows.iter().filter(|a| a.shape_body()).count(),
        badges: doc
            .arrows
            .iter()
            .filter(|a| a.badge.is_some() && !a.shape_body())
            .count(),
    }
}

/// What an editable CDXML/CDX copy writes as shapes instead of arrows, and
/// how its catalytic cycles travel.
pub fn export_notice(doc: &Document) -> Option<String> {
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let cycles = doc.graphics.iter().filter(|g| g.cycle.is_some()).count();
    let cycles = (cycles > 0).then(|| {
        format!(
            "ChemDraw gets {} as plain arrows, text and reaction steps. ReShiki reopens {} as a cycle while the file is unchanged.",
            plural(cycles, "catalytic cycle", "catalytic cycles"),
            if cycles == 1 { "it" } else { "each" }
        )
    });
    match (shapes_notice(doc), cycles) {
        (Some(a), Some(b)) => Some(format!("{a} {b}")),
        (a, b) => a.or(b),
    }
}

/// What an editable CDXML/CDX copy writes as shapes instead of arrows and
/// callouts, which no reader gets back.
pub fn shapes_notice(doc: &Document) -> Option<String> {
    let Lowered { bodies, badges } = lowered_count(doc);
    let leaders = doc.graphics.iter().filter(|g| g.leader.is_some()).count();
    let plural =
        |n: usize, one: &str, many: &str| format!("{n} {}", if n == 1 { one } else { many });
    let mut parts = Vec::new();
    if bodies > 0 {
        parts.push(plural(
            bodies,
            "chevron or curved block arrow",
            "chevron or curved block arrows",
        ));
    }
    if badges > 0 {
        parts.push(plural(badges, "arrow badge", "arrow badges"));
    }
    if leaders > 0 {
        parts.push(plural(leaders, "callout leader", "callout leaders"));
    }
    (!parts.is_empty()).then(|| {
        format!(
            "ChemDraw gets {} as grouped shapes that look the same. Save as a .rsk file to keep them editable as arrows and callouts.",
            parts.join(", ")
        )
    })
}

/// One shape per subpath, so every stroke stays an ordinary open or closed path.
fn shapes(paths: Vec<ArrowPath>, next: &mut u64) -> Vec<Graphic> {
    let mut graphics = Vec::new();
    for path in paths {
        let mut pieces: Vec<Vec<PathCommand>> = Vec::new();
        for command in path.commands {
            if matches!(command, PathCommand::Move(_)) || pieces.is_empty() {
                pieces.push(Vec::new());
            }
            if let Some(piece) = pieces.last_mut() {
                piece.push(command);
            }
        }
        // ChemDraw keeps a gradient body as its own Shaded fill.
        let mut style = path.style;
        if style.effects.gradient.take().is_some() {
            style.effects.shaded = true;
        }
        for piece in pieces.into_iter().filter(|p| p.len() > 1) {
            graphics.push(shape(*next, piece, &style, path.filled));
            *next += 1;
        }
    }
    graphics
}

/// A pen path with the arrow part's outline and look.
pub(crate) fn shape(
    id: u64,
    commands: Vec<PathCommand>,
    style: &GraphicStyle,
    filled: bool,
) -> Graphic {
    let mut style = style.clone();
    if filled {
        style.fill = Some(style.stroke);
        style.width_pt = style.width_pt.max(0.1);
    }
    let mut graphic = Graphic::pen_curve(
        id,
        Default::default(),
        crate::document::Point::new(1., 0.),
        style.clone(),
    );
    // A new pen path starts plain; keep the part's own look.
    graphic.style = style;
    graphic.path = commands;
    graphic.node_types.clear();
    graphic.layer = 1;
    graphic
}

/// The drawing as ChemDraw can keep it: chevron and curved broad bodies
/// become filled shapes, badges become a disc, strokes and text grouped with
/// their arrow, and callout leaders become a line with a dot.
pub fn lowered(doc: &Document) -> Cow<'_, Document> {
    let leaders = doc.graphics.iter().any(|g| g.leader.is_some());
    if !leaders
        && doc
            .arrows
            .iter()
            .all(|a| a.badge.is_none() && !a.shape_body())
    {
        return Cow::Borrowed(doc);
    }
    let mut out = doc.clone();
    let mut next = doc.next_id();
    let mut removed = Vec::new();
    for arrow in &mut out.arrows {
        let whole = arrow.shape_body();
        if !whole && arrow.badge.is_none() {
            continue;
        }
        let members = shapes(
            if whole {
                arrow.paths()
            } else {
                let style = Arrow::badge_style(&arrow.appearance());
                arrow
                    .badge_paths()
                    .into_iter()
                    .map(|commands| ArrowPath {
                        commands,
                        style: style.clone(),
                        filled: false,
                    })
                    .collect()
            },
            &mut next,
        );
        let mut ids: Vec<u64> = members.iter().map(|g| g.id).collect();
        if let Some((position, text, format)) = arrow.badge_text() {
            out.annotations.push(Annotation {
                id: next,
                position,
                text,
                format,
            });
            ids.push(next);
            next += 1;
        }
        out.graphics.extend(members);
        arrow.badge = None;
        if whole {
            removed.push(arrow.id);
        }
        // Every group that holds the arrow holds its shapes, so nested
        // groups stay nested.
        let mut grouped = false;
        for group in out
            .groups
            .iter_mut()
            .filter(|g| g.members.contains(&arrow.id))
        {
            group.members.extend(ids.iter().copied());
            grouped = true;
        }
        if !grouped {
            if !whole {
                ids.push(arrow.id);
            }
            out.groups.push(Group {
                id: next,
                members: ids,
                integral: false,
                layout: None,
            });
            next += 1;
        }
    }
    out.arrows.retain(|a| !removed.contains(&a.id));
    out.arrow_labels.retain(|l| !removed.contains(&l.arrow));
    out.reactions.retain(|r| !removed.contains(&r.arrow));
    for group in &mut out.groups {
        group.members.retain(|id| !removed.contains(id));
    }
    out.groups.retain(|g| g.members.len() > 1);
    // A callout travels as its line, dot and text in one group.
    let mut dots = Vec::new();
    let mut callouts = Vec::new();
    for g in &mut out.graphics {
        let Some(leader) = g.leader.take() else {
            continue;
        };
        let mut members = vec![g.id];
        let mut dot = None;
        g.leader = Some(leader.clone());
        if let Some((commands, style)) = g.leader_dot() {
            dots.push(shape(next, commands, &style, true));
            members.push(next);
            dot = Some(next);
            next += 1;
        }
        g.leader = None;
        if let Some(crate::leaders::Target::Object { id }) = leader.start {
            members.push(id);
        }
        callouts.push((members, dot));
    }
    out.graphics.extend(dots);
    for (members, dot) in callouts {
        // A callout already in groups adds its dot to every group holding
        // its line, so nested groups stay nested and move it along.
        let line = members.first().copied();
        let mut grouped = false;
        for group in out.groups.iter_mut() {
            if group.members.iter().any(|id| members.contains(id)) {
                grouped = true;
                if let Some(dot) =
                    dot.filter(|_| line.is_some_and(|id| group.members.contains(&id)))
                {
                    group.members.push(dot);
                }
            }
        }
        if !grouped && members.len() > 1 {
            out.groups.push(Group {
                id: next,
                members,
                integral: false,
                layout: None,
            });
            next += 1;
        }
    }
    Cow::Owned(out)
}
