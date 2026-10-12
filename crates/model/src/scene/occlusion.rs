//! Vector ink sources for chemical depth occlusion.

use super::{
    BondInputs, BondPass, DepthField, Document, Primitive, Resolve, RingStrokes, push_bond,
};

/// Real vector ink grouped with its chemical owners, layer, and current depth.
pub(crate) struct OcclusionSource {
    pub(crate) owners: Vec<u64>,
    pub(crate) layer: i16,
    pub(crate) field: DepthField,
    pub(crate) parts: Vec<Primitive>,
}

/// Real vector ink for visibility, with its chemical owner and current depth.
/// Clearing only display scopes prevents visibility from recursively querying
/// itself; coordinate, bond, label and projected-rail geometry remain exact.
pub(crate) fn occlusion_ink(doc: &Document) -> Vec<OcclusionSource> {
    let mut drawing = doc.clone();
    drawing.depth_appearance.clear();
    let doc = &drawing;
    let opacity = crate::rear_opacity::Paint::new(doc);
    let inputs = BondInputs::prepare(doc, &opacity, Resolve::AsGiven);
    let rings = RingStrokes::without_gaps(inputs.doc());
    let pass = BondPass::new(&inputs);
    let doc = pass.doc;
    let scene = pass.scene(&doc.drawing_style, &rings, &opacity);
    let mut ink = vec![];
    for (index, bond) in doc
        .bonds
        .iter()
        .enumerate()
        .filter(|(_, b)| doc.bond_visible(b.a, b.b))
    {
        let mut parts = vec![];
        let mut joined = Default::default();
        let mut sources = vec![];
        push_bond(
            scene,
            &mut parts,
            &mut joined,
            index,
            bond,
            Some(&mut sources),
        );
        for (field, parts) in sources {
            ink.push(OcclusionSource {
                owners: vec![bond.a, bond.b],
                layer: bond.z_order,
                field,
                parts,
            });
        }
    }
    for atom in doc.atoms.iter().filter(|a| doc.atom_visible(a.id)) {
        let mut parts = vec![];
        for part in pass.labels.runs.get(&atom.id).into_iter().flatten() {
            // Unknown/custom font outlines conservatively do not hide ink.
            // Default label outlines preserve counters instead of using a box.
            if let Primitive::Text {
                position,
                text,
                size,
                style,
                ..
            } = part
                && style.family == crate::style::DEFAULT.font_family
                && !style.bold
                && !style.italic
                && !style.underline
            {
                parts.push(Primitive::Path {
                    commands: crate::style::outline_text(
                        text,
                        *size,
                        position.offset(0., *size * crate::style::text_ascent(style)),
                    ),
                    style: crate::graphics::GraphicStyle {
                        fill: Some(crate::palette::Color::Ink),
                        width_pt: 0.,
                        ..Default::default()
                    },
                    filled: true,
                });
            }
        }
        parts.extend(
            crate::scientific::styled_mark_parts(atom, &doc.drawing_style)
                .into_iter()
                .map(|p| Primitive::Path {
                    commands: p.commands,
                    style: p.style,
                    filled: p.filled,
                }),
        );
        if !parts.is_empty() {
            ink.push(OcclusionSource {
                owners: vec![atom.id],
                layer: 0,
                field: DepthField::Constant(atom.depth),
                parts,
            });
        }
    }
    ink
}
