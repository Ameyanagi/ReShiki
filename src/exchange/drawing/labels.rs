use super::*;
use crate::{
    atom_labels::{Carbons, HydrogenPosition, Owner},
    typography::{Script, TextAlign, TextSpan},
};

fn face(s: &TextStyle) -> u8 {
    u8::from(s.bold) + 2 * u8::from(s.italic) + 4 * u8::from(s.underline)
}
fn carbon_attributes(mode: Carbons) -> [(&'static str, String); 2] {
    [
        (
            "ShowTerminalCarbonLabels",
            yes(matches!(mode, Carbons::Terminal | Carbons::All)).into(),
        ),
        (
            "ShowNonTerminalCarbonLabels",
            yes(matches!(mode, Carbons::Internal | Carbons::All)).into(),
        ),
    ]
}

impl Writer<'_> {
    pub(super) fn text(
        &mut self,
        parent: Key,
        text: &str,
        format: &TextFormat,
        attrs: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Result<Key> {
        let node = self.tree.add(Some(parent), "t", attrs)?;
        let mut end = 0;
        for span in &format.spans {
            if !(end <= span.start && span.start < span.end && span.end <= text.len()) {
                return Err(invalid("Invalid text style range"));
            }
            if span.start > end {
                self.text_run(
                    node,
                    text.get(end..span.start)
                        .ok_or_else(|| invalid("Invalid UTF-8 style boundary"))?,
                    &format.style,
                )?;
            }
            self.text_run(
                node,
                text.get(span.start..span.end)
                    .ok_or_else(|| invalid("Invalid UTF-8 style boundary"))?,
                &span.style,
            )?;
            end = span.end;
        }
        if end < text.len() || text.is_empty() {
            self.text_run(
                node,
                text.get(end..)
                    .ok_or_else(|| invalid("Invalid UTF-8 style boundary"))?,
                &format.style,
            )?;
        }
        Ok(node)
    }
    fn text_run(&mut self, parent: Key, text: &str, s: &TextStyle) -> Result<()> {
        let font = self.font(&s.family)?;
        let color = self.color(s.color)?;
        let face = face(s)
            | match s.script {
                Script::Subscript => 32,
                Script::Superscript => 64,
                Script::Normal => {
                    if s.formula {
                        96
                    } else {
                        0
                    }
                }
            };
        let node = self.tree.add(
            Some(parent),
            "s",
            [
                ("font", font),
                ("size", number(real(s.size_pt))),
                ("face", face.to_string()),
                ("color", color),
            ],
        )?;
        self.tree.node_mut(node)?.text = Some(text.into());
        Ok(())
    }
    pub(super) fn atoms(&mut self, graph: &crate::chemistry::graph::Graph) -> Result<()> {
        let doc = self.doc;
        let valences = graph.provisional_valences().map_err(invalid)?;
        let mut degrees: HashMap<u64, usize> = HashMap::new();
        for b in &doc.bonds {
            for id in [b.a, b.b] {
                *degrees.entry(id).or_default() += 1;
            }
        }
        for (i, a) in doc.atoms.iter().enumerate() {
            let label_h = u32::from(
                graph
                    .atoms
                    .get(i)
                    .ok_or_else(|| invalid("Missing chemical atom"))?
                    .explicit_hydrogens,
            ) + valences
                .get(i)
                .ok_or_else(|| invalid("Missing chemical valence"))?
                .implicit_hydrogens;
            let atomic_number = graph
                .atoms
                .get(i)
                .ok_or_else(|| invalid("Missing chemical atom"))?
                .atomic_number;
            let node = self.tree.add(
                Some(self.fragment),
                "n",
                [
                    ("id", (i + 3).to_string()),
                    ("p", self.position(a.position)),
                    ("Element", atomic_number.to_string()),
                ],
            )?;
            self.atom_nodes.push(node);
            self.objects.push((a.id, node));
            for (key, value) in [
                ("Charge", i64::from(a.charge)),
                ("Isotope", i64::from(a.isotope)),
            ] {
                if value != 0 {
                    self.tree.set(node, key, value.to_string())?;
                }
            }
            if a.radical_electrons != 0 {
                self.tree.set(
                    node,
                    "Radical",
                    match a.radical_electrons {
                        1 => "Doublet",
                        2 => "Triplet",
                        _ => return Err(invalid("Unsupported radical count")),
                    },
                )?;
            }
            if let Some(kind) = a.attachment {
                self.tree.set(node, "NodeType", kind.cdxml())?;
                let members = a
                    .centroid
                    .iter()
                    .map(|id| self.atom_xml(*id))
                    .collect::<Result<Vec<_>>>()?
                    .join(" ");
                self.tree.set(node, "Attachments", members)?;
                // Semantic nodes have no element or text label in ChemDraw.
                self.tree
                    .node_mut(node)?
                    .attrs
                    .retain(|(key, _)| *key != "Element");
                continue;
            }
            if self.variable_labels
                && let Some(label) = &a.display.variable
            {
                // A display label on an unspecified atom is not a query atom
                // or an abbreviation with an invented molecular definition.
                self.tree.set(node, "NodeType", "Unspecified")?;
                self.tree.set(node, "NumHydrogens", "0")?;
                self.text(
                    node,
                    label,
                    &TextFormat {
                        style: a
                            .text_style
                            .clone()
                            .unwrap_or_else(|| doc.drawing_style.text_style()),
                        ..Default::default()
                    },
                    [("p", self.position(a.position))],
                )?;
                continue;
            }
            if crate::attachments::hidden(a, doc) {
                // Keep the chemical wildcard and its bonds for editable copy,
                // but omit its editing handle from destination applications.
                self.tree.set(node, "Visible", "no")?;
                self.tree.set(node, "NodeType", "Unspecified")?;
                self.tree.set(node, "NumHydrogens", "0")?;
                // ChemDraw converts a textless Element=0 to carbon on save.
                // The hidden sentinel preserves its unspecified identity.
                self.text(
                    node,
                    "*",
                    &TextFormat::default(),
                    [("p", self.position(a.position))],
                )?;
                continue;
            }
            if super::ligands::implicit_carbon(a, doc) {
                self.tree.set(node, "IgnoreWarnings", "yes")?;
            }
            if a.explicit_h != 0 && !super::ligands::implicit_carbon(a, doc) {
                self.tree
                    .set(node, "NumHydrogens", a.explicit_h.to_string())?;
            }
            if !a.marks.is_empty() {
                self.tree.set(node, "NumHydrogens", label_h.to_string())?;
            }
            let mut s = a
                .text_style
                .clone()
                .unwrap_or_else(|| doc.drawing_style.text_style());
            s.script = Script::Normal;
            s.formula = false;
            let isotope = if a.isotope != 0 {
                a.isotope.to_string()
            } else {
                String::new()
            };
            let mut label = format!("{isotope}{}", a.element);
            let mut spans = Vec::new();
            if !isotope.is_empty() {
                spans.push(TextSpan {
                    start: 0,
                    end: isotope.len(),
                    style: TextStyle {
                        script: Script::Superscript,
                        ..s.clone()
                    },
                });
            }
            let show = a.display.hydrogens.unwrap_or(doc.atom_labels.hydrogens);
            if label_h != 0 && show && a.element != "H" {
                label.push('H');
                if label_h > 1 {
                    let start = label.len();
                    label.push_str(&label_h.to_string());
                    spans.push(TextSpan {
                        start,
                        end: label.len(),
                        style: TextStyle {
                            script: Script::Subscript,
                            ..s.clone()
                        },
                    });
                }
            }
            if a.charge != 0 && !a.display.hide_charge && !a.marks.iter().any(|m| m.kind.charge()) {
                let start = label.len();
                if a.charge.unsigned_abs() > 1 {
                    label.push_str(&a.charge.unsigned_abs().to_string());
                }
                label.push(if a.charge > 0 { '+' } else { '-' });
                spans.push(TextSpan {
                    start,
                    end: label.len(),
                    style: TextStyle {
                        script: Script::Superscript,
                        ..s.clone()
                    },
                });
            }
            let degree = degrees.get(&a.id).copied().unwrap_or(0);
            let mode = a.display.carbons.unwrap_or(doc.atom_labels.carbons);
            let visible = a.element != "C"
                || a.isotope != 0
                || a.radical_electrons != 0
                || degree == 0
                || mode == Carbons::All
                || mode == Carbons::Terminal && degree == 1
                || mode == Carbons::Internal && degree > 1;
            if !visible {
                let font = self.font(&s.family)?;
                let color = self.color(s.color)?;
                for (key, value) in [
                    ("LabelFont", font),
                    ("LabelSize", number(real(s.size_pt))),
                    ("LabelFace", face(&s).to_string()),
                    ("LabelColor", color),
                ] {
                    self.tree.set(node, key, value)?;
                }
            } else {
                self.text(
                    node,
                    &label,
                    &TextFormat {
                        style: s,
                        spans,
                        ..Default::default()
                    },
                    [
                        ("p", self.position(a.position)),
                        ("LabelAlignment", "Auto".into()),
                    ],
                )?;
            }
        }
        Ok(())
    }
    pub(super) fn bonds(&mut self) -> Result<()> {
        let doc = self.doc;
        for b in &doc.bonds {
            let id = self.id()?;
            let order = match b.order {
                0 => "hydrogen".into(),
                4 | 7 => "1.5".into(),
                5 => "dative".into(),
                6 => "4".into(),
                _ => b.order.to_string(),
            };
            let display = |s: &str| -> Result<&'static str> {
                Ok(match s {
                    "plain" => "Solid",
                    "dashed" => "Dash",
                    "dotted" => "Dot",
                    "bold" => "Bold",
                    "hashed" => "Hash",
                    "wedge" => "WedgeBegin",
                    "hash" => "WedgedHashBegin",
                    "hollow_wedge" => "HollowWedgeBegin",
                    "wavy" => "Wavy",
                    _ => return Err(invalid("Unsupported bond display")),
                })
            };
            let color = self.color(b.color)?;
            let n = self.tree.add(
                Some(self.fragment),
                "b",
                [
                    ("id", id),
                    ("B", self.atom_xml(b.a)?),
                    ("E", self.atom_xml(b.b)?),
                    ("Order", order),
                    ("Display", display(&b.display)?.into()),
                    ("color", color),
                ],
            )?;
            if b.order == 4 && b.projection {
                self.tree.set(n, "IgnoreWarnings", "yes")?;
            }
            if let Some(second) = &b.secondary_display {
                self.tree.set(n, "Display2", display(second)?)?;
            }
            use crate::bonds::DoublePosition;
            if b.double_position != DoublePosition::Auto && matches!(b.order, 2 | 7) {
                self.tree.set(
                    n,
                    "DoublePosition",
                    match b.double_position {
                        DoublePosition::Auto => "Auto",
                        DoublePosition::Left => "Left",
                        DoublePosition::Right => "Right",
                        DoublePosition::Center => "Center",
                    },
                )?;
            }
            if b.order == 0 {
                self.tree.set(n, "Order", "1")?;
                self.tree.set(n, "Display", "Dash")?;
                self.tree.set(n, "Display2", "DottedHydrogen")?;
            }
            if b.order == 5 && b.display == "dashed" {
                self.tree.set(n, "Order", "1")?;
                self.tree.set(n, "Display", "Dash")?;
            }
            if b.order == 7 && b.display == "dashed" {
                self.tree.set(n, "Display2", "Dash")?;
            }
            self.bond_nodes.push(n);
        }
        Ok(())
    }
    pub(super) fn circles(&mut self, rings: &[Vec<usize>]) -> Result<()> {
        let doc = self.doc;
        let edges: HashMap<_, _> = doc
            .bonds
            .iter()
            .map(|b| ((b.a.min(b.b), b.a.max(b.b)), b))
            .collect();
        let projected = crate::aromatic::circles(doc);
        for ring in rings {
            self.spend(ring.len())?;
            let atoms = ring
                .iter()
                .map(|i| {
                    doc.atoms
                        .get(*i)
                        .ok_or_else(|| invalid("Invalid aromatic ring"))
                })
                .collect::<Result<Vec<_>>>()?;
            if atoms.len() < 3 {
                return Err(invalid("Invalid aromatic ring"));
            }
            let bonds = atoms
                .iter()
                .zip(atoms.iter().cycle().skip(1))
                .map(|(a, b)| {
                    edges
                        .get(&(a.id.min(b.id), a.id.max(b.id)))
                        .copied()
                        .ok_or_else(|| invalid("Missing aromatic edge"))
                })
                .collect::<Result<Vec<_>>>()?;
            if bonds.iter().any(|b| b.order != 4) {
                continue;
            }
            if let Some(circle) = projected.iter().find(|c| {
                c.projected_axes.is_some()
                    && c.atoms.len() == atoms.len()
                    && atoms.iter().all(|a| c.atoms.contains(&a.id))
            }) {
                self.projected_circle(circle)?;
                continue;
            }
            let points: Vec<P> = atoms.iter().map(|a| a.position.into()).collect();
            let center = P {
                x: points.iter().map(|p| p.x).sum::<f64>() / points.len() as f64,
                y: points.iter().map(|p| p.y).sum::<f64>() / points.len() as f64,
            };
            let mut distances = Vec::new();
            let mut lengths = Vec::new();
            for (a, b) in points.iter().zip(points.iter().cycle().skip(1)) {
                let (dx, dy) = (b.x - a.x, b.y - a.y);
                let length = dx.hypot(dy);
                if length < 0.1 {
                    break;
                }
                let t = (((center.x - a.x) * dx + (center.y - a.y) * dy) / length.powi(2))
                    .clamp(0., 1.);
                distances.push((center.x - a.x - t * dx).hypot(center.y - a.y - t * dy));
                lengths.push(length);
            }
            if distances.len() != points.len() {
                continue;
            }
            let style_real = if doc.drawing_style.is_default() {
                default_real
            } else {
                real
            };
            let radius = distances
                .into_iter()
                .reduce(f64::min)
                .ok_or_else(|| invalid("Empty ring"))?
                - lengths.iter().sum::<f64>() / lengths.len() as f64
                    * style_real(doc.drawing_style.bond_spacing_ratio);
            if radius <= style_real(doc.drawing_style.line_width_pt) * 42. / 14.4 * 2. {
                continue;
            }
            let color = self.color(bonds.first().ok_or_else(|| invalid("Empty ring"))?.color)?;
            let id = self.id()?;
            let major = center.add(radius, 0.);
            let minor = center.add(0., radius);
            self.tree.add(
                Some(self.fragment),
                "graphic",
                [
                    ("id", id),
                    ("GraphicType", "Oval".into()),
                    ("OvalType", "Circle".into()),
                    (
                        "BoundingBox",
                        format!("{} {}", self.position(major), self.position(center)),
                    ),
                    ("Center3D", format!("{} 0", self.position(center))),
                    ("MajorAxisEnd3D", format!("{} 0", self.position(major))),
                    ("MinorAxisEnd3D", format!("{} 0", self.position(minor))),
                    ("color", color),
                ],
            )?;
        }
        Ok(())
    }
    pub(super) fn labels(&mut self) -> Result<()> {
        let doc = self.doc;
        let settings = &doc.atom_labels;
        for (k, v) in carbon_attributes(settings.carbons) {
            self.tree.set(0, k, v)?;
        }
        for (k, v) in [
            ("HideImplicitHydrogens", !settings.hydrogens),
            ("ShowAtomStereo", settings.stereo),
            ("ShowBondStereo", settings.stereo),
        ] {
            self.tree.set(0, k, yes(v))?;
        }
        for a in &doc.atoms {
            let n = self.atom_node(a.id)?;
            let d = &a.display;
            if let Some(mode) = d.carbons {
                for (k, v) in carbon_attributes(mode) {
                    self.tree.set(n, k, v)?;
                }
            }
            if let Some(show) = d.hydrogens {
                self.tree.set(n, "HideImplicitHydrogens", yes(!show))?;
            }
            self.tree.set(
                n,
                "LabelDisplay",
                match d.hydrogen_position {
                    HydrogenPosition::Auto => "Auto",
                    HydrogenPosition::Left => "Left",
                    HydrogenPosition::Right => "Right",
                    HydrogenPosition::Above => "Above",
                    HydrogenPosition::Below => "Below",
                },
            )?;
            if let Some(number) = &d.number {
                self.tree.set(n, "AtomNumber", &number.text)?;
                self.tree.set(n, "ShowAtomNumber", "yes")?;
                self.indicator(
                    n,
                    Owner::Number(a.id),
                    &number.text,
                    &number.style,
                    number.offset,
                    a.position.into(),
                )?;
            }
            let show = d.stereo.show.unwrap_or(settings.stereo);
            self.tree.set(n, "ShowAtomStereo", yes(show))?;
            if show && let Some(cip) = &a.cip_label {
                self.indicator(
                    n,
                    Owner::AtomStereo(a.id),
                    &format!("({cip})"),
                    &d.stereo.style,
                    d.stereo.offset,
                    a.position.into(),
                )?;
            }
        }
        for (i, b) in doc.bonds.iter().enumerate() {
            let n = *self
                .bond_nodes
                .get(i)
                .ok_or_else(|| invalid("Missing bond node"))?;
            let d = &b.indicator;
            let show = d.show.unwrap_or(settings.stereo);
            self.tree.set(n, "ShowBondStereo", yes(show))?;
            if show && let Some(cip) = &b.cip_label {
                let a = P::from(self.atom(b.a)?.position);
                let c = P::from(self.atom(b.b)?.position);
                self.indicator(
                    n,
                    Owner::BondStereo(b.a, b.b),
                    &format!("({cip})"),
                    &d.style,
                    d.offset,
                    P {
                        x: (a.x + c.x) / 2.,
                        y: (a.y + c.y) / 2.,
                    },
                )?;
            }
        }
        Ok(())
    }
    fn indicator(
        &mut self,
        node: Key,
        owner: Owner,
        text: &str,
        style: &TextStyle,
        offset: Option<Point>,
        anchor: P,
    ) -> Result<()> {
        let metrics = self
            .options
            .atom_indicators
            .unwrap_or_default()
            .iter()
            .find(|i| i.owner == owner && i.text == text);
        let (width, height, origin, style) = if let Some(m) = metrics {
            (
                real(m.width),
                real(m.height),
                P::from(m.origin),
                m.style.clone(),
            )
        } else {
            let width = text.chars().count() as f64 * real(style.size_pt) * 0.6 / self.scale;
            let height = real(style.size_pt) / self.scale;
            let offset = offset.map(P::from).unwrap_or(P {
                x: 0.,
                y: -height * 1.3,
            });
            (
                width,
                height,
                anchor.add(offset.x - width / 2., offset.y - height / 2.),
                style.clone(),
            )
        };
        let tag = self.tree.add(
            Some(node),
            "objecttag",
            [
                (
                    "Name",
                    if matches!(owner, Owner::Number(_)) {
                        "number"
                    } else {
                        "stereo"
                    }
                    .into(),
                ),
                ("TagType", "Unknown".into()),
            ],
        )?;
        self.text(
            tag,
            text,
            &TextFormat {
                style,
                ..Default::default()
            },
            [
                ("p", self.position(origin.add(0., height * 0.9))),
                (
                    "BoundingBox",
                    format!(
                        "{} {}",
                        self.position(origin),
                        self.position(origin.add(width, height))
                    ),
                ),
                ("CaptionLineHeight", "variable".into()),
            ],
        )?;
        Ok(())
    }
    pub(super) fn annotations(&mut self) -> Result<()> {
        let doc = self.doc;
        for a in &doc.annotations {
            let f = &a.format;
            let s = &f.style;
            let (width, height, baseline) =
                if let Some(m) = self.options.text_layout.and_then(|t| t.get(&a.id)) {
                    (real(m.width), real(m.height), real(m.baseline))
                } else {
                    (
                        f.width_pt.map(real).unwrap_or_else(|| {
                            a.text
                                .split('\n')
                                .map(|s| s.chars().count())
                                .max()
                                .unwrap_or(0) as f64
                                * real(s.size_pt)
                                * 0.6
                        }),
                        a.text.split('\n').count() as f64 * real(s.size_pt) * real(f.line_spacing),
                        real(s.size_pt) * 0.9,
                    )
                };
            if [width, height, baseline]
                .iter()
                .any(|v| !v.is_finite() || *v < 0.)
            {
                return Err(invalid("Invalid text metrics"));
            }
            let origin = P::from(a.position);
            let factor = match f.alignment {
                TextAlign::Center => 0.5,
                TextAlign::Right => 1.,
                _ => 0.,
            };
            let anchor = origin.add(width / self.scale * factor, baseline / self.scale);
            let far = origin.add(width / self.scale, height / self.scale);
            let id = self.id()?;
            let node = self.text(
                self.page,
                &a.text,
                f,
                [
                    ("id", id),
                    ("p", self.position(anchor)),
                    (
                        "BoundingBox",
                        format!("{} {}", self.position(origin), self.position(far)),
                    ),
                    (
                        "CaptionJustification",
                        match f.alignment {
                            TextAlign::Center => "Center",
                            TextAlign::Right => "Right",
                            TextAlign::Justified => "Full",
                            _ => "Left",
                        }
                        .into(),
                    ),
                    (
                        "CaptionLineHeight",
                        number(
                            (real(s.size_pt) * real(f.line_spacing) * 1000.).round_ties_even()
                                / 1000.,
                        ),
                    ),
                ],
            )?;
            if let Some(width) = f.width_pt {
                self.tree.set(
                    node,
                    "WordWrapWidth",
                    format!("{:.0}", real(width).round_ties_even()),
                )?;
            }
            self.objects.push((a.id, node));
        }
        Ok(())
    }
}
