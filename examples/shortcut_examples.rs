//! Rebuild the single editable shortcut reference, using the editor's graph edits.
//! cargo run --example shortcut_examples -- assets/examples/shortcut-examples.rsk
use anyhow::{Context, ensure};
use reshiki::{
    bonds::{BondPreset, DoublePosition},
    document::{Annotation, Document, Point},
    editing::{self, Transform},
    hotkeys,
    pages::Layout,
    style::DEFAULT as STYLE,
    typography::TextFormat,
};
use std::{fs, path::PathBuf};

struct Sample {
    caption: String,
    drawing: Document,
}
struct Section {
    title: &'static str,
    hint: &'static str,
    samples: Vec<Sample>,
}
fn sample(caption: impl Into<String>, drawing: Document) -> Sample {
    Sample {
        caption: caption.into(),
        drawing,
    }
}
fn chain(count: usize) -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let mut ids = Vec::new();
    let mut p = Point::default();
    for i in 0..count {
        ids.push(doc.add_atom("C", p));
        let angle = if i % 2 == 0 { -30_f32 } else { 30_f32 }.to_radians();
        p = p.offset(42. * angle.cos(), 42. * angle.sin());
    }
    for pair in ids.windows(2) {
        if let [a, b] = pair {
            doc.add_bond(*a, *b, 1, "plain");
        }
    }
    (doc, ids)
}
fn atom_sample(key: &str, label: &str, count: usize) -> anyhow::Result<Sample> {
    let (doc, ids) = chain(count);
    let target = *ids.last().context("Atom target")?;
    let (result, _) = hotkeys::atom_edit(&doc, target, key, 42.)
        .context("Unmapped atom shortcut")?
        .map_err(anyhow::Error::msg)?;
    Ok(sample(format!("{key}  ·  {label}"), result))
}
fn caption(doc: &mut Document, text: &str, p: Point, size: f32, bold: bool, width: f32) {
    let mut format = TextFormat::default();
    format.style.size_pt = size;
    format.style.bold = bold;
    format.style.color = if bold { [41, 62, 56] } else { [78, 90, 87] };
    format.width_pt = Some(width);
    doc.annotations.push(Annotation {
        id: doc.next_id(),
        position: p,
        text: text.into(),
        format,
    });
}
fn sections() -> anyhow::Result<Vec<Section>> {
    let mut groups = Vec::new();
    for (key, label) in [
        ("m", "Me"),
        ("e", "Et"),
        ("y", "Boc"),
        ("A", "Ac"),
        ("E", "CO2Me"),
        ("F", "CF3"),
        ("H", "Cbz"),
        ("N", "NO2"),
        ("O", "OMe"),
        ("P", "Ph"),
        ("Q", "Fmoc"),
        ("Z", "N3"),
    ] {
        groups.push(atom_sample(key, label, 3)?);
    }
    let mut atoms = Vec::new();
    for (key, label) in [
        ("c", "Carbon"),
        ("n", "Nitrogen (also w)"),
        ("o", "Oxygen (also q)"),
        ("s", "Sulfur"),
        ("p", "Phosphorus"),
        ("f", "Fluorine"),
        ("h", "Hydrogen"),
        ("b", "Bromine"),
        ("C", "Chlorine (also l)"),
        ("B", "Boron"),
        ("i", "Iodine"),
        ("L", "Lithium"),
        ("S", "Silicon"),
        ("d", "Deuterium"),
        ("r", "Variable R"),
        ("x", "Variable X"),
        ("+", "Increase charge"),
        ("-", "Decrease charge"),
    ] {
        atoms.push(atom_sample(key, label, 2)?);
    }
    let mut growth = Vec::new();
    for (key, label) in [
        ("0", "Branch"),
        ("1", "Extend chain"),
        ("2", "Terminal carbonyl"),
        ("4", "Wedge growth"),
        ("5", "Hashed wedge growth"),
        ("8", "Add =CH2"),
        ("9", "Dimethyl"),
        ("z", "Alkyne"),
        ("K", "tert-Butyl"),
        ("k", "Sulfonyl"),
    ] {
        growth.push(atom_sample(key, label, 2)?);
    }
    let (doc, ids) = chain(3);
    let id = *ids.get(1).context("Middle atom")?;
    let (ketone, _) = hotkeys::atom_edit(&doc, id, "2", 42.)
        .context("Carbonyl shortcut")?
        .map_err(anyhow::Error::msg)?;
    growth.push(sample("2 on an internal atom · Ketone", ketone));
    let mut rings = Vec::new();
    for (key, label) in [
        ("3", "Phenyl (also a)"),
        ("6", "Six-member ring"),
        ("7", "Five-member ring"),
        ("v", "Three-member ring"),
        ("u", "Four-member ring"),
    ] {
        let (doc, ids) = chain(2);
        let (result, _) = hotkeys::ring_edit(&doc, ids.last().copied(), None, key, 42.)
            .context("Ring shortcut")?
            .map_err(anyhow::Error::msg)?;
        rings.push(sample(format!("{key}  ·  {label}"), result));
    }
    let mut aromatic = Document::default();
    editing::ring(&mut aromatic, Point::default(), 6, true, 0.);
    rings.push(sample("a on selected ring · Circle", aromatic));
    let mut bonds = Vec::new();
    for (key, label) in [
        ("1", "Single"),
        ("2", "Double"),
        ("3", "Triple"),
        ("d", "Dashed"),
        ("D", "Partial double"),
        ("b", "Bold"),
        ("B", "Bold double"),
        ("w", "Wedge"),
        ("h", "Hashed wedge (also W)"),
        ("H", "Hashed"),
        ("y", "Wavy"),
    ] {
        let (doc, ids) = chain(4);
        let [_, a, b, _] = ids.as_slice() else {
            anyhow::bail!("Four atoms required");
        };
        let result = hotkeys::bond_edit(
            &doc,
            *a,
            *b,
            hotkeys::bond_preset(key).context("Bond shortcut")?,
        )
        .map_err(anyhow::Error::msg)?;
        bonds.push(sample(format!("{key}  ·  {label}"), result));
    }
    for (key, position) in [
        ("l", DoublePosition::Left),
        ("c", DoublePosition::Center),
        ("r", DoublePosition::Right),
    ] {
        let (mut doc, ids) = chain(4);
        let [_, a, b, _] = ids.as_slice() else {
            anyhow::bail!("Four atoms required");
        };
        doc = hotkeys::bond_edit(&doc, *a, *b, BondPreset::Double).map_err(anyhow::Error::msg)?;
        doc.bonds
            .iter_mut()
            .find(|b| b.a == *a)
            .context("Double bond")?
            .double_position = position;
        bonds.push(sample(format!("{key}  ·  Double line {position}"), doc));
    }
    let mut fusion = Vec::new();
    for (key, label) in [
        ("v", "Three-member ring"),
        ("4", "Four-member ring"),
        ("5", "Five-member ring"),
        ("6", "Six-member ring"),
        ("7", "Seven-member ring"),
        ("8", "Eight-member ring"),
        ("a", "Benzene"),
        ("z", "Diene"),
        ("9", "Chair"),
        ("0", "Alternate chair"),
    ] {
        let doc = reshiki::rings::Preset::Regular.document(42., false);
        let b = doc.bonds.first().context("Ring edge")?;
        let (result, _) = hotkeys::ring_edit(&doc, None, Some((b.a, b.b)), key, 42.)
            .context("Fusion shortcut")?
            .map_err(anyhow::Error::msg)?;
        fusion.push(sample(format!("{key}  ·  {label}"), result));
    }
    let mut attachments = vec![atom_sample("M", "MgBr", 3)?];
    for (key, element, times, charge, label) in [
        ("j", "Fe", 1, 0, "j · Cp at an atom"),
        ("j", "Fe", 2, 2, "j twice · Two Cp ligands"),
        ("J", "Ru", 1, 0, "J · Arene at an atom"),
    ] {
        let mut doc = Document::default();
        let id = doc.add_atom(element, Point::default());
        doc.atom_mut(id).context("Metal")?.charge = charge;
        for _ in 0..times {
            doc = hotkeys::atom_edit(&doc, id, key, 42.)
                .context("Ligand shortcut")?
                .map_err(anyhow::Error::msg)?
                .0;
        }
        attachments.push(sample(label, doc));
    }
    for label in ["C2H5", "Cp*", "Boc", "NH3"] {
        let mut doc = Document::default();
        let id = doc.add_atom("C", Point::default());
        doc = reshiki::atom_text::apply(&doc, id, label, reshiki::atom_text::Mode::Auto)
            .map_err(anyhow::Error::msg)?;
        attachments.push(sample(format!("Enter · Type {label}"), doc));
    }
    let mut tools = Vec::new();
    for (key, preset) in [
        ("1 / x / b", BondPreset::Single),
        ("2", BondPreset::Double),
        ("3", BondPreset::Triple),
        ("4", BondPreset::Quadruple),
    ] {
        let (mut doc, _) = chain(2);
        preset.apply(doc.bonds.first_mut().context("Bond")?);
        tools.push(sample(format!("{key}  ·  {preset}"), doc));
    }
    tools.push(sample("X · Straight chain tool", chain(5).0));
    let mut ring = Document::default();
    editing::ring(&mut ring, Point::default(), 6, false, 0.);
    tools.push(sample("r · Ring tool (last chosen size)", ring));
    tools.push(sample(
        "j · Benzene tool",
        reshiki::rings::Preset::Benzene.document(42., false),
    ));
    let mut circle = Document::default();
    editing::ring(&mut circle, Point::default(), 6, true, 0.);
    tools.push(sample("Cmd/Ctrl-click · Delocalized ring", circle));
    tools.push(sample(
        "J · Cyclopentadiene tool",
        reshiki::rings::Preset::Cyclopentadiene.document(42., false),
    ));
    let mut arrow = Document::default();
    arrow.arrows.push(reshiki::document::Arrow {
        id: 1,
        start: Point::default(),
        end: Point::new(140., 0.),
        kind: "forward".into(),
        control: None,
        style: None,
    });
    tools.push(sample("a / e · Reaction arrow tool", arrow));
    let (base, ids) = chain(2);
    let from = *ids.last().context("Atom drag source")?;
    let end = editing::bond_extension(
        &base,
        base.atom(from).context("Atom")?.position,
        Some(from),
        1,
    );
    let (added, _) =
        editing::add_bonded_atom(&base, from, end, None, "O").map_err(anyhow::Error::msg)?;
    tools.push(sample("Atoms: O · Drag to add with a bond", added));
    let mut transforms = Vec::new();
    let (doc, ids) = chain(2);
    let (source, _) = hotkeys::ring_edit(&doc, ids.last().copied(), None, "3", 42.)
        .context("Transform source")?
        .map_err(anyhow::Error::msg)?;
    for (label, transform) in [
        ("Alt+Down · Rotate 15 degrees", Transform::Rotate(15.)),
        ("Alt+Right · Rotate 1 degree", Transform::Rotate(1.)),
        ("Shift+Alt+Down · Tilt X 12 degrees", Transform::TiltX(12.)),
        ("Shift+Alt+Left · Tilt Y 12 degrees", Transform::TiltY(12.)),
        (
            "Cmd/Ctrl+Shift+V · Flip horizontally",
            Transform::FlipHorizontal,
        ),
        (
            "Cmd/Ctrl+Shift+H · Flip vertically",
            Transform::FlipVertical,
        ),
    ] {
        let mut doc = source.clone();
        let ids = doc.all_ids();
        editing::transform(&mut doc, &ids, transform);
        transforms.push(sample(label, doc));
    }
    Ok(vec![
        Section {
            title: "Common groups",
            hint: "Hover an atom, then press the key. Uppercase means Shift+letter. Groups retain their atoms.",
            samples: groups,
        },
        Section {
            title: "Elements, variables and charge",
            hint: "Hover an atom or select one atom. R and X are variable labels, not fully specified molecules.",
            samples: atoms,
        },
        Section {
            title: "Grow a structure",
            hint: "Hover the endpoint unless stated otherwise. The next edit continues from the new endpoint.",
            samples: growth,
        },
        Section {
            title: "Attach rings",
            hint: "Hover an atom. Select a complete aromatic ring and press a to switch circle / alternating bonds.",
            samples: rings,
        },
        Section {
            title: "Change a bond",
            hint: "Hover the middle bond. Repeat 2 to cycle double-line placement. f brings a crossing bond forward.",
            samples: bonds,
        },
        Section {
            title: "Fuse rings",
            hint: "Hover an existing bond. Ring shortcuts share its two endpoints and preserve the connected structure.",
            samples: fusion,
        },
        Section {
            title: "Groups, ligands and typed labels",
            hint: "j / J add tilted pi ligands with retained 3D coordinates. Enter edits labels; Cmd/Ctrl+Enter finishes typing.",
            samples: attachments,
        },
        Section {
            title: "Choose a drawing tool",
            hint: "Move the pointer to empty canvas and clear the selection first. Then press the key and draw.",
            samples: tools,
        },
        Section {
            title: "Transform a selection",
            hint: "Select the entire structure first. Option is Alt on Mac. Opposite arrow keys reverse rotation or tilt.",
            samples: transforms,
        },
    ])
}

pub fn build() -> anyhow::Result<Document> {
    build_document(false)
}

fn build_document(print_review: bool) -> anyhow::Result<Document> {
    let sections = sections()?;
    let layout = Layout {
        width_pt: 540.,
        height_pt: 420.,
        columns: if print_review { 1 } else { 3 },
        rows: if print_review {
            u8::try_from(sections.len())?
        } else {
            3
        },
        margins: reshiki::pages::Margins {
            top: 12.,
            right: 12.,
            bottom: 12.,
            left: 12.,
        },
        ..Layout::default()
    };
    let mut doc = Document {
        page_layout: print_review.then_some(layout.clone()),
        ..Document::default()
    };
    for (index, section) in sections.iter().enumerate() {
        let (lo, _) = layout.bounds(index).context("Section position")?;
        let p = |x: f32, y: f32| lo.offset(STYLE.world(x), STYLE.world(y));
        caption(
            &mut doc,
            &format!("{:02}  {}", index + 1, section.title),
            p(24., 18.),
            19.,
            true,
            492.,
        );
        caption(&mut doc, section.hint, p(24., 48.), 9., false, 492.);
        let columns = match section.title {
            "Fuse rings" => 4,
            "Groups, ligands and typed labels" => 2,
            _ => 3,
        };
        let width = 492. / columns as f32;
        let rows = section.samples.len().div_ceil(columns);
        let height = 294. / rows as f32;
        for (i, item) in section.samples.iter().enumerate() {
            item.drawing.validate().map_err(anyhow::Error::msg)?;
            let left = 24. + (i % columns) as f32 * width;
            let top = 78. + (i / columns) as f32 * height;
            let mut drawing = item.drawing.clone();
            // Fill derived hydrogen labels just as a normal chemistry refresh does.
            // Variable/attachment diagrams may intentionally lack a molecular identity.
            if let Ok(molecule) = reshiki::chemistry::document::prepare(&drawing) {
                let prepared = reshiki::chemistry::document::for_drawing(&molecule, &drawing)?;
                let labels = prepared.labels()?;
                let checked = prepared.finish(labels)?;
                reshiki::atom_labels::refresh_computed(&mut drawing, &checked);
            }
            let (low, high) = reshiki::scene::selection_bounds(&drawing, &drawing.all_ids())
                .context("Sample bounds")?;
            if high.y - low.y > STYLE.world(height - 24.)
                && high.x - low.x <= STYLE.world(height - 24.)
            {
                let ids = drawing.all_ids();
                editing::transform(&mut drawing, &ids, Transform::Rotate(90.));
            }
            let (low, high) = reshiki::scene::selection_bounds(&drawing, &drawing.all_ids())
                .context("Sample bounds")?;
            ensure!(
                high.x - low.x <= STYLE.world(width - 14.),
                "{} is too wide",
                item.caption
            );
            ensure!(
                high.y - low.y <= STYLE.world(height - 24.),
                "{} is too tall",
                item.caption
            );
            let center = p(left + (width - 14.) / 2., top + (height - 24.) / 2.);
            let added = editing::append(
                &mut doc,
                &drawing,
                center.offset(-(low.x + high.x) / 2., -(low.y + high.y) / 2.),
            );
            ensure!(!added.is_empty(), "Could not insert {}", item.caption);
            caption(
                &mut doc,
                &item.caption,
                p(left, top + height - 22.),
                8.5,
                true,
                width - 14.,
            );
        }
        caption(
            &mut doc,
            "Double-click a structure, then Cmd/Ctrl+C. Paste into your drawing with Cmd/Ctrl+V.",
            p(24., 377.),
            8.,
            false,
            492.,
        );
        caption(
            &mut doc,
            &format!(
                "Section {} / {}  ·  Pan or zoom to browse. Groups and templates may be revised in future releases.",
                index + 1,
                sections.len()
            ),
            p(24., 394.),
            7.5,
            false,
            492.,
        );
    }
    doc.validate().map_err(anyhow::Error::msg)?;
    Ok(doc)
}

fn main() -> anyhow::Result<()> {
    let path = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: shortcut_examples OUTPUT.rsk [REVIEW_DIRECTORY]")?,
    );
    let doc = build()?;
    fs::write(path, serde_json::to_vec_pretty(&doc)?)?;
    if let Some(out) = std::env::args_os().nth(2) {
        let out = PathBuf::from(out);
        fs::create_dir_all(&out)?;
        fs::write(
            out.join("shortcut-examples.pdf"),
            reshiki::export::pages_pdf(&build_document(true)?).map_err(anyhow::Error::msg)?,
        )?;
    }
    Ok(())
}
