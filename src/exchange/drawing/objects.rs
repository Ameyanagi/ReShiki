use super::*;
use crate::{
    arrows::{Head, HeadShape, NoGo},
    graphics::{GraphicKind, LinePattern},
    scientific::MarkKind,
};

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
            let control = a.control.map(P::from).or_else(|| {
                matches!(a.kind.as_str(), "curved" | "fishhook" | "bent").then_some(P {
                    x: (start.x + end.x - end.y + start.y) / 2.,
                    y: (start.y + end.y + end.x - start.x) / 2.,
                })
            });
            if s.pattern == LinePattern::Dotted || a.kind == "retro" {
                return Err(invalid(
                    "Dotted and retrosynthesis arrows require native or image export",
                ));
            }
            if a.kind == "equilibrium" && (control.is_some() || s.equilibrium_ratio != 1.) {
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
            if control.is_some()
                && (s.shape != HeadShape::Solid
                    || s.dipole
                    || s.no_go != NoGo::None
                    || s.head == Head::None && s.tail == Head::None)
            {
                return Err(invalid(
                    "This curved arrow decoration requires native or image export",
                ));
            }
            let head = |h| match h {
                Head::Full => "Full",
                Head::None => "None",
                Head::Left => "HalfLeft",
                Head::Right => "HalfRight",
            };
            let integer = |v: f64| format!("{:.0}", v.round_ties_even());
            let id = self.id()?;
            let color = self.color(s.color)?;
            let n = self.tree.add(
                Some(self.page),
                if control.is_some() { "curve" } else { "arrow" },
                [
                    ("id", id),
                    ("ArrowheadHead", head(s.head).into()),
                    ("ArrowheadTail", head(s.tail).into()),
                    (
                        "ArrowheadType",
                        match s.shape {
                            HeadShape::Solid => "Solid",
                            HeadShape::Hollow => "Hollow",
                            HeadShape::Open => "Angle",
                        }
                        .into(),
                    ),
                    ("HeadSize", integer(style_real(s.head_length_pt) * 100.)),
                    (
                        "ArrowheadCenterSize",
                        integer(
                            style_real(s.head_length_pt) * (1. - style_real(s.head_notch)) * 100.,
                        ),
                    ),
                    (
                        "ArrowheadWidth",
                        integer(style_real(s.head_width_pt) * 100.),
                    ),
                    ("LineWidth", number(style_real(s.width_pt))),
                    (
                        "LineType",
                        if s.pattern == LinePattern::Dashed {
                            "Dashed"
                        } else {
                            "Solid"
                        }
                        .into(),
                    ),
                    ("FillType", "None".into()),
                    ("color", color),
                ],
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
            if a.kind == "equilibrium" {
                self.tree
                    .set(n, "ArrowShaftSpacing", integer(style_real(s.gap_pt) * 100.))?;
            }
            if let Some(c) = control {
                let points = if a.kind == "bent" {
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
            }
            self.objects.push((a.id, n));
        }
        Ok(())
    }
    pub(super) fn marks(&mut self) -> Result<()> {
        let doc = self.doc;
        for atom in &doc.atoms {
            for mark in &atom.marks {
                self.spend(doc.atoms.len())?;
                let (symbol, attribute) = match mark.kind {
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
                                if mark.kind == MarkKind::CircledCharge {
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
                if atom.text_style.as_ref().is_some_and(|s| s.color != [0; 3]) {
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
                if distance * self.scale > label_size * 1.05 {
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
            if fill.commands(self.doc).is_empty() {
                continue;
            }
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
            let color = self.color(fill.color)?;
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
            if g.kind == GraphicKind::Picture {
                let n = self.picture(g, z)?;
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
                vec![crate::scientific::Part {
                    commands,
                    style: g.style.clone(),
                    filled: false,
                }]
            };
            let mut parsed = Vec::new();
            for part in parts {
                part.style.validate().map_err(invalid)?;
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
                parsed.push((part.style, paths));
            }
            let parent = if parsed.len() > 1
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
                    if let Some(fill) = paint.fill {
                        self.curve(target, points, *closed, fill, true, 0., false, z)?;
                    }
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
                    object = Some(if parent != self.page { parent } else { n });
                }
            }
            if let Some(n) = object {
                self.objects.push((g.id, n));
            }
        }
        Ok(middle)
    }
    #[allow(clippy::too_many_arguments)]
    pub(super) fn curve(
        &mut self,
        parent: Key,
        points: &[P],
        closed: bool,
        color: [u8; 3],
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
