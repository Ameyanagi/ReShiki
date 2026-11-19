use super::*;
use crate::{
    arrows::{Body, Head, HeadShape, NoGo},
    graphics::{GraphicKind, GraphicStyle, LinePattern, PathHeads},
    ring_fills::RingEffect,
    scientific::MarkKind,
};

fn head_attributes(heads: &PathHeads, scalar: fn(f32) -> f64) -> [(&'static str, String); 6] {
    let head = |h| match h {
        Head::Full => "Full",
        Head::None => "None",
        Head::Left => "HalfLeft",
        Head::Right => "HalfRight",
    };
    let integer = |v: f64| format!("{:.0}", v.round_ties_even());
    let length = (scalar(heads.head_length_pt) * 100.).round_ties_even();
    // Keep the rounded center within the importer's 0–0.9 notch range.
    let center = (length * (1. - scalar(heads.head_notch)))
        .round_ties_even()
        .clamp((length / 10.).ceil(), length);
    [
        ("ArrowheadHead", head(heads.head).into()),
        ("ArrowheadTail", head(heads.tail).into()),
        (
            "ArrowheadType",
            match heads.shape {
                HeadShape::Solid => "Solid",
                HeadShape::Hollow => "Hollow",
                HeadShape::Open => "Angle",
            }
            .into(),
        ),
        ("HeadSize", integer(length)),
        ("ArrowheadCenterSize", integer(center)),
        (
            "ArrowheadWidth",
            integer(scalar(heads.head_width_pt) * 100.),
        ),
    ]
}

impl Writer<'_> {
    pub(super) fn arrows(&mut self) -> Result<()> {
        let doc = self.doc;
        for a in &doc.arrows {
            let s = a.appearance();
            let style_real = if a.style.is_none() {
                default_real
            } else {
                real
            };
            let start = P::from(a.start);
            let end = P::from(a.end);
            let cubic = a.cubic.map(|[a, b]| [P::from(a), P::from(b)]);
            let control = a
                .control
                .map(P::from)
                .filter(|p| {
                    if a.kind != "retro" {
                        return true;
                    }
                    let dx = end.x - start.x;
                    let dy = end.y - start.y;
                    let length = dx * dx + dy * dy;
                    let t = if length > 0. {
                        ((p.x - start.x) * dx + (p.y - start.y) * dy) / length
                    } else {
                        0.
                    }
                    .clamp(0., 1.);
                    (p.x - start.x - t * dx).hypot(p.y - start.y - t * dy) > 0.001
                })
                .or_else(|| {
                    if cubic.is_some() {
                        return None;
                    }
                    matches!(a.kind.as_str(), "curved" | "fishhook" | "bent").then_some(P {
                        x: (start.x + end.x - end.y + start.y) / 2.,
                        y: (start.y + end.y + end.x - start.x) / 2.,
                    })
                });
            // ChemDraw draws a circular arc arrow from its centre and sweep.
            let native_arc = a
                .arc_circle()
                .filter(|_| !matches!(a.kind.as_str(), "equilibrium" | "retro"));
            let curved = native_arc.is_none()
                && (!a.nodes.is_empty() || control.is_some() || cubic.is_some());
            if s.pattern == LinePattern::Dotted {
                return Err(invalid("Dotted arrows require native or image export"));
            }
            if a.kind == "retro" && curved {
                return Err(invalid(
                    "Curved retrosynthesis arrows require native or image export",
                ));
            }
            if a.kind == "equilibrium" && (curved || s.equilibrium_ratio != 1.) {
                return Err(invalid(
                    "Bent or unequal equilibrium arrows require native or image export",
                ));
            }
            if s.dipole && s.tail != Head::None {
                return Err(invalid(
                    "A dipole tail head cannot be preserved in editable interchange",
                ));
            }
            if a.kind == "equilibrium"
                && (!matches!(s.head, Head::Left | Head::Right)
                    || !matches!(s.tail, Head::Left | Head::Right))
            {
                return Err(invalid("Equilibrium exchange requires half heads"));
            }
            if curved
                && (s.shape != HeadShape::Solid
                    || s.dipole
                    || s.no_go != NoGo::None
                    || s.head == Head::None && s.tail == Head::None)
            {
                return Err(invalid(
                    "This curved arrow decoration requires native or image export",
                ));
            }
            let integer = |v: f64| format!("{:.0}", v.round_ties_even());
            let id = self.id()?;
            let color = self.color(s.color)?;
            let n = self.tree.add(
                Some(self.page),
                if curved { "curve" } else { "arrow" },
                [("id", id)]
                    .into_iter()
                    .chain(head_attributes(
                        &PathHeads {
                            head: s.head,
                            tail: s.tail,
                            shape: s.shape,
                            head_length_pt: s.head_length_pt,
                            head_width_pt: s.head_width_pt,
                            head_notch: s.head_notch,
                        },
                        style_real,
                    ))
                    .chain([
                        ("LineWidth", number(style_real(s.width_pt))),
                        (
                            "LineType",
                            // ChemDraw has no wavy curve; its curves stay solid.
                            match s.pattern {
                                LinePattern::Dashed => "Dashed",
                                LinePattern::Wavy if !curved => "Wavy",
                                _ => "Solid",
                            }
                            .into(),
                        ),
                        ("FillType", "None".into()),
                        ("color", color),
                    ]),
            )?;
            if s.no_go != NoGo::None {
                self.tree.set(
                    n,
                    "NoGo",
                    if s.no_go == NoGo::Cross {
                        "Cross"
                    } else {
                        "Hash"
                    },
                )?;
            }
            if s.dipole {
                self.tree.set(n, "Dipole", "yes")?;
            }
            // ChemDraw's block arrows: a hollow head on a shaft as wide as
            // the shaft spacing, filled, outlined or shaded, all sized in
            // hundredths of the line width.
            let body = a.drawn_body();
            if !body.is_line() {
                let length = style_real(s.head_length_pt);
                let (gap, width) = (style_real(s.gap_pt), style_real(s.head_width_pt));
                // A filled body has no visible outline, so a 1 pt line keeps
                // the sizes in points. An outline keeps its width while every
                // size stays within what ChemDraw files allow.
                let outline = style_real(s.width_pt);
                let fits = |pt: f64, lo: f64, hi: f64| (lo..=hi).contains(&(pt / outline));
                let line = if body == Body::Hollow
                    && fits(length, 0.5, 24.)
                    && fits((width - gap).max(0.25), 0.25, 16.)
                    && fits(gap, 0.5, 16.)
                {
                    outline
                } else {
                    1.
                };
                self.tree.set(n, "LineWidth", number(line))?;
                let size = |pt: f64| integer(pt / line * 100.);
                // Round the length first and keep the center within the
                // reader's notch range, as for line arrows.
                let head = (length / line * 100.).round_ties_even();
                let center = (head * (1. - style_real(s.head_notch)))
                    .round_ties_even()
                    .clamp((head / 10.).ceil(), head);
                self.tree.set(n, "HeadSize", integer(head))?;
                self.tree.set(n, "ArrowheadCenterSize", integer(center))?;
                self.tree.set(
                    n,
                    "FillType",
                    match body {
                        Body::Hollow => "None",
                        Body::Gradient => "Shaded",
                        _ => "Solid",
                    },
                )?;
                self.tree.set(n, "ArrowheadType", "Hollow")?;
                self.tree.set(n, "ArrowShaftSpacing", size(gap))?;
                self.tree
                    .set(n, "ArrowheadWidth", size((width - gap).max(0.25)))?;
            }
            if matches!(a.kind.as_str(), "equilibrium" | "retro") {
                self.tree
                    .set(n, "ArrowShaftSpacing", integer(style_real(s.gap_pt) * 100.))?;
            }
            if curved {
                let points = if !a.nodes.is_empty() {
                    let [departure, arrival] =
                        cubic.ok_or_else(|| invalid("Missing curved arrow controls"))?;
                    let mut points = Vec::with_capacity(3 * (a.nodes.len() + 2));
                    points.extend([start, start, departure]);
                    for node in &a.nodes {
                        points.extend([
                            P::from(node.controls[0]),
                            P::from(node.point),
                            P::from(node.controls[1]),
                        ]);
                    }
                    points.extend([arrival, end, end]);
                    points
                } else if let Some([a, b]) = cubic {
                    vec![start, start, a, b, end, end]
                } else if let Some(c) = control {
                    if a.kind == "bent" {
                        vec![
                            start,
                            start,
                            start.lerp(c, 1. / 3.),
                            start.lerp(c, 2. / 3.),
                            c,
                            c.lerp(end, 1. / 3.),
                            c.lerp(end, 2. / 3.),
                            end,
                            end,
                        ]
                    } else {
                        vec![
                            start,
                            start,
                            start.add(2. * (c.x - start.x) / 3., 2. * (c.y - start.y) / 3.),
                            end.add(2. * (c.x - end.x) / 3., 2. * (c.y - end.y) / 3.),
                            end,
                            end,
                        ]
                    }
                } else {
                    return Err(invalid("Missing curved arrow controls"));
                };
                self.tree.set(
                    n,
                    "CurvePoints",
                    points
                        .iter()
                        .map(|p| self.position(*p))
                        .collect::<Vec<_>>()
                        .join(" "),
                )?;
                self.tree.set(
                    n,
                    "CurveType",
                    if s.pattern == LinePattern::Dashed {
                        "2"
                    } else {
                        "0"
                    },
                )?;
                self.tree.set(n, "Closed", "no")?;
            } else {
                self.tree
                    .set(n, "Tail3D", format!("{} 0", self.position(start)))?;
                self.tree
                    .set(n, "Head3D", format!("{} 0", self.position(end)))?;
                if let (Some((center, radius, _)), Some(sweep)) = (native_arc, a.arc) {
                    // ChemDraw runs from the tail through -AngularSize degrees:
                    // the sign alone sets the direction, so a clockwise arc
                    // has a negative size.
                    for (key, p) in [
                        ("Center3D", center),
                        ("MajorAxisEnd3D", center.offset(radius, 0.)),
                        ("MinorAxisEnd3D", center.offset(0., radius)),
                    ] {
                        self.tree.set(n, key, format!("{} 0", self.position(p)))?;
                    }
                    self.tree.set(n, "AngularSize", number(f64::from(-sweep)))?;
                }
            }
            if a.kind == "retro" {
                let id = self.id()?;
                let legacy = self.tree.add(
                    Some(self.page),
                    "graphic",
                    [
                        ("id", id),
                        ("SupersededBy", self.tree.value(n, "id")?),
                        (
                            "BoundingBox",
                            format!("{} {}", self.position(end), self.position(start)),
                        ),
                        ("GraphicType", "Line".into()),
                        ("ArrowType", "RetroSynthetic".into()),
                        ("HeadSize", self.tree.value(n, "HeadSize")?),
                        ("color", self.tree.value(n, "color")?),
                    ],
                )?;
                // Both representations share group ownership; the legacy one
                // also needs the arrow's stacking order, like ChemDraw's files.
                self.objects.push((a.id, legacy));
            }
            self.objects.push((a.id, n));
        }
        Ok(())
    }
    pub(super) fn marks(&mut self) -> Result<()> {
        let doc = self.doc;
        let drawn = doc.abbreviation_render_copy();
        for atom in &doc.atoms {
            for mark in &atom.marks {
                self.spend(doc.atoms.len())?;
                let kind = match mark.kind {
                    MarkKind::Charge | MarkKind::CircledCharge if atom.display.hide_charge => {
                        continue;
                    }
                    MarkKind::RadicalIon if atom.display.hide_charge => MarkKind::Radical,
                    kind => kind,
                };
                if let Some(text) = match kind {
                    MarkKind::PartialPositive => Some("δ+"),
                    MarkKind::PartialNegative => Some("δ−"),
                    _ => None,
                } {
                    let size = crate::style::DEFAULT.world(mark.size_pt.unwrap_or_else(|| {
                        atom.text_style
                            .as_ref()
                            .map_or(doc.drawing_style.font_size_pt, |s| s.size_pt)
                            * 0.75
                    }));
                    self.partial_charge_text(
                        text,
                        P::from(atom.position.offset(mark.offset.x, mark.offset.y)),
                        real(size),
                        atom.text_style
                            .as_ref()
                            .map_or(Default::default(), |s| s.color),
                        0,
                    )?;
                    continue;
                }
                let (symbol, attribute) = match kind {
                    MarkKind::Charge | MarkKind::CircledCharge => {
                        if atom.charge == 0 {
                            continue;
                        }
                        if atom.charge.unsigned_abs() != 1 {
                            return Err(invalid(
                                "Positioned multiple-charge marks require native or image export",
                            ));
                        }
                        (
                            format!(
                                "{}{}",
                                if kind == MarkKind::CircledCharge {
                                    "Circle"
                                } else {
                                    ""
                                },
                                if atom.charge > 0 { "Plus" } else { "Minus" }
                            ),
                            "Charge",
                        )
                    }
                    MarkKind::Radical => {
                        if atom.radical_electrons == 0 {
                            continue;
                        }
                        (
                            if atom.radical_electrons == 1 {
                                "Electron"
                            } else {
                                "LonePair"
                            }
                            .into(),
                            "Radical",
                        )
                    }
                    MarkKind::LonePair => {
                        if atom.element == "C" {
                            return Err(invalid(
                                "A carbon lone-pair mark cannot be preserved in editable interchange",
                            ));
                        }
                        ("LonePair".into(), "Radical")
                    }
                    _ => {
                        return Err(invalid(
                            "Lone-pair bars and combined radical-ion marks require native or image export",
                        ));
                    }
                };
                if atom.text_style.as_ref().is_some_and(|s| {
                    self.palette.rgb(s.color) != self.palette.rgb(crate::palette::Color::Ink)
                }) {
                    return Err(invalid("Colored atom marks require native or image export"));
                }
                let label_size = real(
                    atom.text_style
                        .as_ref()
                        .map(|s| s.size_pt)
                        .unwrap_or(doc.drawing_style.font_size_pt),
                );
                let offset = P::from(mark.offset);
                let distance = offset.x.hypot(offset.y);
                // Marks orbit outside the label ink, so measure from its far corner.
                let ink =
                    crate::scene::atom_label_ink_bounds(atom, &drawn).map_or(0., |(lo, hi)| {
                        let (x, y) = (real(atom.position.x), real(atom.position.y));
                        (x - real(lo.x))
                            .abs()
                            .max((real(hi.x) - x).abs())
                            .hypot((y - real(lo.y)).abs().max((real(hi.y) - y).abs()))
                    });
                if (distance - ink) * self.scale > label_size * 1.05 {
                    return Err(invalid(
                        "Move the mark closer to its atom for editable interchange",
                    ));
                }
                let mut size = mark.size_pt.map(real).unwrap_or(label_size * 0.75);
                if matches!(symbol.as_str(), "Electron" | "LonePair") {
                    size /= 2.;
                }
                let center = P::from(atom.position).add(offset.x, offset.y);
                if doc.atoms.iter().any(|other| {
                    other.id != atom.id
                        && (center.x - real(other.position.x))
                            .hypot(center.y - real(other.position.y))
                            <= distance
                }) {
                    return Err(invalid("Mark attachment is ambiguous near another atom"));
                }
                let angle = real(mark.angle).to_radians();
                let end = center.add(
                    -angle.cos() * size / self.scale,
                    -angle.sin() * size / self.scale,
                );
                let id = self.id()?;
                let n = self.tree.add(
                    Some(self.fragment),
                    "graphic",
                    [
                        ("id", id),
                        ("GraphicType", "Symbol".into()),
                        ("SymbolType", symbol),
                        (
                            "BoundingBox",
                            format!("{} {}", self.position(center), self.position(end)),
                        ),
                    ],
                )?;
                self.tree.add(
                    Some(n),
                    "represent",
                    [
                        ("attribute", attribute.into()),
                        ("object", self.atom_xml(atom.id)?),
                    ],
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn ring_fills(&mut self) -> Result<()> {
        let edges: HashMap<_, _> = self
            .doc
            .bonds
            .iter()
            .zip(&self.bond_nodes)
            .map(|(b, &node)| ((b.a.min(b.b), b.a.max(b.b)), node))
            .collect();
        for fill in &self.doc.ring_fills {
            // Serialize validated ownership, independent of canvas visibility.
            let mut basis = Vec::new();
            let mut parent = None;
            for (&a, &b) in fill.atoms.iter().zip(fill.atoms.iter().cycle().skip(1)) {
                let node = *edges
                    .get(&(a.min(b), a.max(b)))
                    .ok_or_else(|| invalid("Missing ring bond"))?;
                let fragment = self
                    .tree
                    .node(node)?
                    .parent
                    .ok_or_else(|| invalid("Missing ring fragment"))?;
                if parent.is_some_and(|p| p != fragment) {
                    return Err(invalid("Ring fill spans molecular fragments"));
                }
                parent = Some(fragment);
                basis.push(self.tree.value(node, "id")?);
            }
            // ChemDraw's own ring-fill representation: the area belongs to the
            // fragment and references bonds, so atom edits reshape its fill.
            let id = self.id()?;
            // ChemDraw paints a ring fill flat: a gradient as the color halfway
            // along it, a see-through one as it shows over white paper, and a
            // glow or disc as the ring's own fill.
            let rgb = self.palette.rgb(fill.color);
            let color = self.color(match fill.effect {
                RingEffect::Flat | RingEffect::Glow | RingEffect::Disc => fill.color,
                RingEffect::Gradient => mix([255; 3], rgb, 0.5),
                RingEffect::SeeThrough => mix([255; 3], rgb, crate::graphics::Effects::SEE_THROUGH),
            })?;
            self.tree.add(
                parent,
                "ColoredMolecularArea",
                [
                    ("id", id),
                    ("bgcolor", color),
                    ("BasisObjects", basis.join(" ")),
                ],
            )?;
        }
        Ok(())
    }
    pub(super) fn graphics(&mut self) -> Result<usize> {
        let doc = self.doc;
        let mut ordered: Vec<_> = doc.graphics.iter().collect();
        ordered.sort_by_key(|g| g.layer);
        let fill_layer = usize::from(!doc.ring_fills.is_empty());
        let middle = ordered.iter().filter(|g| g.layer < 0).count() + 1 + fill_layer;
        for (index, g) in ordered.into_iter().enumerate() {
            let z = if g.layer < 0 {
                index + 1
            } else {
                index + 2 + fill_layer
            };
            if self.generic_bracket(g, z)? {
                continue;
            }
            if g.kind == GraphicKind::Picture {
                let n = self.picture(g, z)?;
                self.objects.push((g.id, n));
                continue;
            }
            if let Some(n) = self.native_shape(g, z)? {
                self.objects.push((g.id, n));
                continue;
            }
            if let GraphicKind::Symbol(kind) = g.kind
                && let Some(text) = kind.partial_charge_text()
            {
                let x = P::from(g.axis_x);
                let n = self.partial_charge_text(
                    text,
                    P::from(g.origin),
                    x.x.hypot(x.y),
                    g.style.stroke,
                    z,
                )?;
                self.objects.push((g.id, n));
                continue;
            }
            let parts = if let Some(parts) = self.options.graphic_parts.and_then(|p| p.get(&g.id)) {
                parts.clone()
            } else {
                if matches!(g.kind, GraphicKind::Symbol(_) | GraphicKind::Orbital(_)) {
                    return Err(invalid(
                        "Scientific drawing export requires styled vector parts",
                    ));
                }
                let commands = self
                    .options
                    .graphic_paths
                    .and_then(|p| p.get(&g.id))
                    .ok_or_else(|| invalid("Graphic export requires vector geometry"))?
                    .clone();
                // A borderless panel keeps only its fill.
                let borderless = g.panel.as_ref().is_some_and(|p| !p.border);
                let mut style = g.style.clone();
                if borderless {
                    style.width_pt = 0.;
                }
                vec![crate::scientific::Part {
                    commands,
                    style,
                    filled: borderless,
                }]
            };
            let text_style = doc.drawing_style.text_style();
            // Title backings take the panel's fill, and titles on a band the
            // color that reads on the band as exported.
            let border = g
                .panel
                .as_ref()
                .is_some_and(|p| p.place == crate::graphics::TitlePlace::Border);
            let palette = crate::palette::Palette::of(doc);
            let paper = doc.canvas_theme.background();
            let backing = crate::palette::Color::Custom(
                g.backing(|c| palette.rgb(c), paper).unwrap_or(paper),
            );
            let panel_parts: Vec<_> = g
                .panel_parts(&text_style)
                .into_iter()
                .map(|mut part| {
                    if border && part.filled {
                        part.style.fill = Some(backing);
                    }
                    part
                })
                .collect();
            let mut panel_titles = g.panel_titles(&text_style);
            if border {
                let outline = palette.rgb(g.style.stroke);
                let rgb = crate::graphics::panel::border_title_rgb(outline, palette.rgb(backing));
                if rgb != outline {
                    for label in &mut panel_titles {
                        label.style.color = crate::palette::Color::Custom(rgb);
                    }
                }
            } else {
                let band = palette.rgb(g.style.stroke);
                let ink = if crate::color_contrast::luminance(band) > 0.45 {
                    [0; 3]
                } else {
                    [255; 3]
                };
                for label in &mut panel_titles {
                    label.style.color = crate::palette::Color::Custom(ink);
                }
            }
            let mut parsed = Vec::new();
            for part in parts {
                // Scene-created fill pieces have no stroke. Their original
                // orbital boundary is exported separately, so clipping edges
                // must not acquire an artificial outline.
                let mut checked_style = part.style.clone();
                if part.filled && checked_style.width_pt == 0. {
                    checked_style.width_pt = 0.6;
                }
                checked_style.validate().map_err(invalid)?;
                if part.style.pattern == LinePattern::Dotted {
                    return Err(invalid("Dotted graphics require native or image export"));
                }
                self.spend(part.commands.len())?;
                let paths = curve_points(&part.commands)?;
                if paths.is_empty() || paths.iter().any(|(points, _)| points.len() < 6) {
                    return Err(invalid(
                        "A graphic needs at least two anchors for editable export",
                    ));
                }
                if part.style.fill.is_some()
                    && paths.iter().any(|(_, closed)| *closed)
                    && !paths.iter().all(|(_, closed)| *closed)
                {
                    return Err(invalid(
                        "Mixed open and closed filled paths cannot be preserved",
                    ));
                }
                let closed = paths.iter().all(|(_, closed)| *closed);
                parsed.push((self.chemdraw_style(&part.style, closed), paths));
            }
            // Closed and compound paths draw no heads.
            let heads = match parsed.as_slice() {
                [(_, paths)] if matches!(paths.as_slice(), [(_, false)]) => g.heads.as_ref(),
                _ => None,
            };
            let parent = if g
                .superscript_label(&doc.drawing_style.text_style())
                .is_some()
                || !panel_parts.is_empty()
                || !panel_titles.is_empty()
                || parsed.len() > 1
                || parsed
                    .iter()
                    .any(|(paint, paths)| paths.len() > 1 || paint.fill.is_some())
            {
                let id = self.id()?;
                self.tree
                    .add(Some(self.page), "group", [("id", id), ("Z", z.to_string())])?
            } else {
                self.page
            };
            let mut object = None;
            for (paint, paths) in &parsed {
                for (points, closed) in paths {
                    let target = if parsed.len() > 1 && paint.fill.is_some() {
                        let id = self.id()?;
                        self.tree
                            .add(Some(parent), "group", [("id", id), ("Z", z.to_string())])?
                    } else {
                        parent
                    };
                    // A faded fill is the outline's own: one curve draws both.
                    let fade = paint.effects.opacity.filter(|_| *closed);
                    if let Some(fill) = paint.fill.filter(|_| fade.is_none()) {
                        let n = self.curve(target, points, *closed, fill, true, 0., false, z)?;
                        if paint.effects.shaded && *closed {
                            self.tree.set(n, "CurveType", "257")?;
                            self.tree.set(n, "FillType", "Shaded")?;
                        }
                        object = Some(if parent != self.page { parent } else { n });
                    }
                    if paint.width_pt > 0. {
                        let n = self.curve(
                            target,
                            points,
                            *closed,
                            paint.stroke,
                            false,
                            real(paint.width_pt),
                            paint.pattern == LinePattern::Dashed,
                            z,
                        )?;
                        if let Some(heads) = heads {
                            for (name, value) in head_attributes(heads, real) {
                                self.tree.set(n, name, value)?;
                            }
                        }
                        if let Some(alpha) = fade {
                            self.tree.set(n, "FadePercent", fade_percent(alpha))?;
                        }
                        object = Some(if parent != self.page { parent } else { n });
                    }
                }
            }
            self.bracket_superscript(g, parent, z)?;
            if !panel_parts.is_empty() || !panel_titles.is_empty() {
                self.panel(panel_parts, &panel_titles, parent, z)?;
                object = Some(parent);
            }
            if let Some(n) = object {
                self.objects.push((g.id, n));
            }
        }
        Ok(middle)
    }
    /// A shaded or shadowed rectangle or oval as ChemDraw's own shape, so
    /// ChemDraw keeps its style editable. Its one color is the fill's; an
    /// outline in another color follows as a curve in the same group. Rotated
    /// rectangles and wavy or dotted outlines stay curves; a wavy line without
    /// heads becomes ChemDraw's line, the one shape ChemDraw draws wavy.
    fn native_shape(&mut self, g: &crate::graphics::Graphic, z: usize) -> Result<Option<Key>> {
        let style = &self.chemdraw_style(&g.style, g.kind != GraphicKind::Line);
        let (o, x, y) = (P::from(g.origin), P::from(g.axis_x), P::from(g.axis_y));
        let ellipse = g.kind == GraphicKind::Ellipse;
        let line = g.kind == GraphicKind::Line
            && style.pattern == LinePattern::Wavy
            && g.heads
                .is_none_or(|h| h.head == Head::None && h.tail == Head::None);
        if !line
            && (style.effects.is_plain()
                // A panel's titles and dividers go with its curves in one group.
                || g.panel.is_some()
                || !matches!(
                    g.kind,
                    GraphicKind::Rectangle | GraphicKind::RoundedRectangle | GraphicKind::Ellipse
                )
                || matches!(style.pattern, LinePattern::Dotted | LinePattern::Wavy)
                || !ellipse && (x.y != 0. || y.x != 0.)
                // CDX keeps CornerRadius in a signed 16-bit field.
                || g.kind == GraphicKind::RoundedRectangle
                    && real(g.corner_radius()) * self.scale * 100. > f64::from(i16::MAX))
        {
            return Ok(None);
        }
        // ChemDraw paints a shadowed outline white inside, so white is no fill.
        let fill = style.fill.filter(|f| {
            style.effects.shaded
                || style.effects.opacity.is_some()
                || self.palette.rgb(*f) != [255; 3]
        });
        // A line keeps any fill it inherited unused; it is drawn in its stroke.
        let fill = fill.filter(|_| !line);
        let outline = fill.is_some_and(|f| self.palette.rgb(f) != self.palette.rgb(style.stroke));
        let color = self.color(fill.unwrap_or(style.stroke))?;
        let parent = if outline {
            let id = self.id()?;
            self.tree
                .add(Some(self.page), "group", [("id", id), ("Z", z.to_string())])?
        } else {
            self.page
        };
        let mut attributes: Vec<(&str, String)> = vec![
            ("id", self.id()?),
            ("Z", z.to_string()),
            ("LineWidth", number(real(style.width_pt))),
            ("color", color),
        ];
        if line {
            // Start first, so the waves come back on the same side.
            attributes.extend([
                ("GraphicType", "Line".into()),
                ("LineType", "Wavy".into()),
                (
                    "BoundingBox",
                    format!("{} {}", self.position(o), self.position(o.add(x.x, x.y))),
                ),
            ]);
            return Ok(Some(self.tree.add(Some(parent), "graphic", attributes)?));
        }
        let (width, height) = (x.x.hypot(x.y), y.x.hypot(y.y));
        let mut flags = vec![];
        if g.kind == GraphicKind::RoundedRectangle {
            flags.push("RoundEdge");
        }
        if ellipse && (width - height).abs() < 1e-6 && (x.x * y.x + x.y * y.y).abs() < 1e-6 {
            flags.push("Circle");
        }
        if let Some(alpha) = style.effects.opacity.filter(|_| fill.is_some()) {
            // ChemDraw fades a plain shape's inside with its own color.
            attributes.push(("FadePercent", fade_percent(alpha)));
        } else if fill.is_some() {
            flags.push(if style.effects.shaded {
                "Shaded"
            } else {
                "Filled"
            });
        }
        if style.pattern == LinePattern::Dashed {
            flags.push("Dashed");
        }
        if let Some(d) = style.effects.shadow_pt {
            flags.push(if ellipse { "Shadowed" } else { "Shadow" });
            attributes.push(("ShadowSize", format!("{:.0}", real(d) * 100.)));
        }
        // ChemDraw's plain oval has no flag; a plain rectangle is Plain.
        if flags.is_empty() && !ellipse {
            flags.push("Plain");
        }
        if ellipse {
            let center = o.add((x.x + y.x) / 2., (x.y + y.y) / 2.);
            let major = center.add(x.x / 2., x.y / 2.);
            let minor = center.add(y.x / 2., y.y / 2.);
            attributes.extend([
                ("GraphicType", "Oval".into()),
                ("OvalType", flags.join(" ")),
                (
                    "BoundingBox",
                    format!("{} {}", self.position(major), self.position(center)),
                ),
                ("Center3D", format!("{} 0", self.position(center))),
                ("MajorAxisEnd3D", format!("{} 0", self.position(major))),
                ("MinorAxisEnd3D", format!("{} 0", self.position(minor))),
            ]);
        } else {
            attributes.extend([
                ("GraphicType", "Rectangle".into()),
                ("RectangleType", flags.join(" ")),
                // ChemDraw writes the far corner first; reversed, it casts the
                // shadow and the shading toward the top left.
                (
                    "BoundingBox",
                    format!("{} {}", self.position(o.add(x.x, y.y)), self.position(o)),
                ),
            ]);
            if g.kind == GraphicKind::RoundedRectangle {
                let radius = real(g.corner_radius()) * self.scale;
                attributes.push(("CornerRadius", format!("{:.0}", radius * 100.)));
            }
        }
        let shape = self.tree.add(Some(parent), "graphic", attributes)?;
        if !outline {
            return Ok(Some(shape));
        }
        let commands = self
            .options
            .graphic_paths
            .and_then(|p| p.get(&g.id))
            .ok_or_else(|| invalid("Graphic export requires vector geometry"))?
            .clone();
        for (points, closed) in curve_points(&commands)? {
            self.curve(
                parent,
                &points,
                closed,
                style.stroke,
                false,
                real(style.width_pt),
                style.pattern == LinePattern::Dashed,
                z,
            )?;
        }
        Ok(Some(parent))
    }
    /// The nearest style ChemDraw draws, which has neither gradients nor
    /// see-through color: a gradient becomes the flat color halfway along it,
    /// and a see-through color is blended over white paper, except a closed
    /// fill in its outline's color, which keeps its opacity for ChemDraw's
    /// faded fill (FadePercent).
    pub(super) fn chemdraw_style(&self, style: &GraphicStyle, closed: bool) -> GraphicStyle {
        let mut out = style.clone();
        if let (Some(gradient), Some(fill)) = (out.effects.gradient.take(), out.fill) {
            out.fill = Some(mix(
                self.palette.rgb(gradient.from),
                self.palette.rgb(fill),
                0.5,
            ));
        }
        // As the scene draws it: a closed shape fades its fill, others their lines.
        if let Some(alpha) = out.effects.opacity {
            match out.fill.filter(|_| closed) {
                Some(fill)
                    if style.effects.gradient.is_none()
                        && out.width_pt > 0.
                        && self.palette.rgb(fill) == self.palette.rgb(out.stroke) => {}
                Some(fill) => {
                    out.fill = Some(mix([255; 3], self.palette.rgb(fill), alpha));
                    out.effects.opacity = None;
                }
                None => {
                    out.stroke = mix([255; 3], self.palette.rgb(out.stroke), alpha);
                    out.effects.opacity = None;
                }
            }
        }
        out
    }
    pub(super) fn bracket_superscript(
        &mut self,
        graphic: &crate::graphics::Graphic,
        parent: Key,
        z: usize,
    ) -> Result<()> {
        let Some(label) = graphic.superscript_label(&self.doc.drawing_style.text_style()) else {
            return Ok(());
        };
        // Scripted CDXML text is anchored on the unraised nominal baseline.
        let baseline = label.position.offset(
            0.,
            crate::style::text_ascent(&label.style)
                + crate::style::DEFAULT.world(label.style.size_pt) * 0.3,
        );
        let far = label.position.offset(
            crate::style::styled_text_width(label.text, label.size, &label.style),
            label.size,
        );
        let id = self.id()?;
        self.text(
            parent,
            label.text,
            &TextFormat {
                style: label.style,
                ..Default::default()
            },
            [
                ("id", id),
                ("Z", z.to_string()),
                ("InterpretChemically", "no".into()),
                ("CaptionJustification", "Left".into()),
                ("p", self.position(baseline)),
                (
                    "BoundingBox",
                    format!("{} {}", self.position(label.position), self.position(far)),
                ),
            ],
        )?;
        Ok(())
    }
    /// A panel's title band, title backings, dividers and titles, as plain
    /// filled curves, lines and text inside the panel's group.
    fn panel(
        &mut self,
        parts: Vec<crate::scientific::Part>,
        titles: &[crate::graphics::GraphicLabel<'_>],
        parent: Key,
        z: usize,
    ) -> Result<()> {
        for part in parts {
            self.spend(part.commands.len())?;
            for (points, closed) in curve_points(&part.commands)? {
                if part.filled
                    && let Some(fill) = part.style.fill
                {
                    self.curve(parent, &points, closed, fill, true, 0., false, z)?;
                } else {
                    self.curve(
                        parent,
                        &points,
                        closed,
                        part.style.stroke,
                        false,
                        real(part.style.width_pt),
                        part.style.pattern == LinePattern::Dashed,
                        z,
                    )?;
                }
            }
        }
        for label in titles {
            let baseline = label
                .position
                .offset(0., crate::style::text_ascent(&label.style));
            let far = label.position.offset(
                crate::style::styled_text_width(label.text, label.size, &label.style),
                label.size,
            );
            let id = self.id()?;
            self.text(
                parent,
                label.text,
                &TextFormat {
                    style: label.style.clone(),
                    ..Default::default()
                },
                [
                    ("id", id),
                    ("Z", z.to_string()),
                    ("InterpretChemically", "no".into()),
                    ("CaptionJustification", "Left".into()),
                    ("p", self.position(baseline)),
                    (
                        "BoundingBox",
                        format!("{} {}", self.position(label.position), self.position(far)),
                    ),
                ],
            )?;
        }
        Ok(())
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn curve(
        &mut self,
        parent: Key,
        points: &[P],
        closed: bool,
        color: crate::palette::Color,
        fill: bool,
        width: f64,
        dashed: bool,
        z: usize,
    ) -> Result<Key> {
        let flags = u8::from(closed) | if dashed { 2 } else { 0 } | if fill { 128 } else { 0 };
        let id = self.id()?;
        let color = self.color(color)?;
        self.tree.add(
            Some(parent),
            "curve",
            [
                ("id", id),
                ("Z", z.to_string()),
                (
                    "CurvePoints",
                    points
                        .iter()
                        .map(|p| self.position(*p))
                        .collect::<Vec<_>>()
                        .join(" "),
                ),
                ("CurveType", flags.to_string()),
                ("Closed", yes(closed).into()),
                ("FillType", if fill { "Solid" } else { "None" }.into()),
                ("LineType", if dashed { "Dashed" } else { "Solid" }.into()),
                ("LineWidth", if fill { "0".into() } else { number(width) }),
                ("color", color),
            ],
        )
    }
    fn picture(&mut self, g: &crate::graphics::Graphic, z: usize) -> Result<Key> {
        let x = P::from(g.axis_x);
        let y = P::from(g.axis_y);
        let o = P::from(g.origin);
        let width = x.x.hypot(x.y);
        let height = y.x.hypot(y.y);
        if !(0.01..=1_000_000.).contains(&width)
            || !(0.01..=1_000_000.).contains(&height)
            || (x.x * y.x + x.y * y.y).abs() > width * height * 0.0001
        {
            return Err(invalid("Picture exchange requires a rectangular frame"));
        }
        let picture = g
            .picture
            .as_ref()
            .ok_or_else(|| invalid("Missing picture"))?;
        let bytes = crate::pictures::exchange::export(picture, x.x * y.y - x.y * y.x < 0.)
            .map_err(invalid)?;
        if bytes.len() > super::super::LIMIT / 2 {
            return Err(Error::Limit);
        }
        let hex = bytes.iter().map(|b| format!("{b:02x}")).collect::<String>();
        let center = o.add((x.x + y.x) / 2., (x.y + y.y) / 2.);
        let id = self.id()?;
        self.tree.add(
            Some(self.page),
            "embeddedobject",
            [
                ("id", id),
                ("Z", z.to_string()),
                (
                    "BoundingBox",
                    format!(
                        "{} {}",
                        self.position(center.add(-width / 2., -height / 2.)),
                        self.position(center.add(width / 2., height / 2.))
                    ),
                ),
                (
                    "RotationAngle",
                    format!(
                        "{:.0}",
                        (x.y.atan2(x.x).to_degrees() * 65536.).round_ties_even()
                    ),
                ),
                ("PNG", hex),
            ],
        )
    }
}

pub(super) fn curve_points(commands: &[PathCommand]) -> Result<Vec<(Vec<P>, bool)>> {
    let mut groups = Vec::new();
    let mut current = Vec::new();
    let mut closed = false;
    let mut anchor = P::default();
    for command in commands {
        match command {
            PathCommand::Move(p) => {
                if !current.is_empty() {
                    groups.push((std::mem::take(&mut current), closed));
                }
                anchor = (*p).into();
                current.extend([anchor; 3]);
                closed = false;
            }
            PathCommand::Line(p) => {
                let end = P::from(*p);
                *current
                    .last_mut()
                    .ok_or_else(|| invalid("Graphic path does not start at an anchor"))? = anchor;
                current.extend([end; 3]);
                anchor = end;
            }
            PathCommand::Cubic(a, b, p) => {
                let end = P::from(*p);
                *current
                    .last_mut()
                    .ok_or_else(|| invalid("Graphic path does not start at an anchor"))? =
                    (*a).into();
                current.extend([(*b).into(), end, end]);
                anchor = end;
            }
            PathCommand::Close => closed = true,
        }
    }
    if !current.is_empty() {
        groups.push((current, closed));
    }
    // A closed CDX curve stores one incoming/anchor/outgoing triple per
    // anchor. An explicit final cubic back to the first anchor must wrap its
    // incoming control into the first triple, not duplicate that anchor.
    // Native readers use this canonical form to recognize delocalized rings.
    for (points, closed) in &mut groups {
        if *closed && points.len() >= 9 {
            let first = points
                .get(1)
                .copied()
                .ok_or_else(|| invalid("Missing curve anchor"))?;
            let last = points
                .get(points.len() - 2)
                .copied()
                .ok_or_else(|| invalid("Missing curve anchor"))?;
            if (first.x - last.x).abs() < 0.000001 && (first.y - last.y).abs() < 0.000001 {
                let incoming = points
                    .get(points.len() - 3)
                    .copied()
                    .ok_or_else(|| invalid("Missing curve control"))?;
                if let Some(start) = points.first_mut() {
                    *start = incoming;
                }
                points.truncate(points.len() - 3);
            }
        }
    }
    if groups
        .iter()
        .flat_map(|(points, _)| points)
        .any(|p| !p.x.is_finite() || !p.y.is_finite())
    {
        return Err(invalid("Invalid graphic coordinate"));
    }
    Ok(groups)
}

/// The color `t` of the way from `a` to `b`.
pub(super) fn mix(a: [u8; 3], b: [u8; 3], t: f32) -> crate::palette::Color {
    let mut out = a;
    for (c, b) in out.iter_mut().zip(b) {
        *c = (f32::from(*c) + (f32::from(b) - f32::from(*c)) * t).round() as u8;
    }
    crate::palette::Color::Custom(out)
}

/// ChemDraw's FadePercent, in tenths of a percent of the color's strength.
fn fade_percent(alpha: f32) -> String {
    format!("{:.0}", (alpha * 1000.).clamp(1., 999.))
}
