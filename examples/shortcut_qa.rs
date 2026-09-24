//! Generate a review gallery from the same graph edits and renderer as the editor.
//! All output goes to the explicitly supplied directory.
use anyhow::Context;
use reshiki::{
    document::{Annotation, Document, Point},
    editing::{self, Transform},
    engine::{LocalEngine, Request},
    hotkeys,
    templates::{Anchor, Connection},
};
use std::{fs, path::PathBuf};

fn chain(count: usize) -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let mut point = Point::default();
    let mut ids = Vec::new();
    for i in 0..count {
        ids.push(doc.add_atom("C", point));
        let angle = if i % 2 == 0 { -30_f32 } else { 30_f32 }.to_radians();
        point = point.offset(42. * angle.cos(), 42. * angle.sin());
    }
    for pair in ids.windows(2) {
        if let [a, b] = pair {
            doc.add_bond(*a, *b, 1, "plain");
        }
    }
    (doc, ids)
}

fn main() -> anyhow::Result<()> {
    let category_filter = std::env::args().nth(2);
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("Usage: shortcut_qa OUTPUT_DIRECTORY [CATEGORY]")?,
    );
    fs::create_dir_all(&directory)?;
    let mut cases = Vec::<(String, String, Document)>::new();
    for key in [
        "b", "B", "c", "C", "f", "h", "i", "L", "n", "o", "p", "s", "S", "d", "r", "x", "+", "-",
    ] {
        let (doc, ids) = chain(2);
        let id = *ids.last().context("Missing atom")?;
        let (result, _) = hotkeys::atom_edit(&doc, id, key, 42.)
            .context("Unmapped atom")?
            .map_err(anyhow::Error::msg)?;
        cases.push(("Atoms".into(), format!("Atom key {key}"), result));
    }
    for key in ["A", "e", "E", "F", "H", "m", "N", "O", "P", "Q", "y"] {
        let (doc, ids) = chain(3);
        let id = *ids.last().context("Missing atom")?;
        for rotation in [0., 90., 180., 270.] {
            let mut original = doc.clone();
            editing::transform_about(&mut original, &ids, Point::default(), 1., rotation);
            let (result, _) = hotkeys::atom_edit(&original, id, key, 42.)
                .context("Unmapped group")?
                .map_err(anyhow::Error::msg)?;
            cases.push((
                "Groups".into(),
                format!("Group {key}, {rotation} degrees"),
                result,
            ));
        }
    }
    for key in ["0", "1", "2", "4", "5", "8", "9", "z", "K", "k"] {
        for count in [2, 3] {
            if key == "z" && count == 3 {
                continue;
            } // A saturated secondary carbon cannot accept a triple bond.
            let (doc, ids) = chain(count);
            let id = if count == 2 {
                *ids.last().context("Missing atom")?
            } else {
                *ids.get(1).context("Missing center")?
            };
            let (result, _) = hotkeys::atom_edit(&doc, id, key, 42.)
                .context("Unmapped growth")?
                .map_err(anyhow::Error::msg)?;
            cases.push((
                "Growth".into(),
                format!(
                    "Growth {key}, {}",
                    if count == 2 { "terminal" } else { "internal" }
                ),
                result,
            ));
        }
    }
    let (mut doc, ids) = chain(4);
    if let [a, b, ..] = ids.as_slice() {
        doc.add_bond(*a, *b, 2, "plain");
    }
    let id = *ids.last().context("Missing terminal")?;
    fs::write(
        directory.join("angle-input.rsk"),
        serde_json::to_vec_pretty(&doc)?,
    )?;
    for key in ["3", "a", "6", "7", "v", "u"] {
        for rotation in [0., 17.3, 90.] {
            let mut original = doc.clone();
            editing::transform_about(&mut original, &ids, Point::default(), 1., rotation);
            let (result, _) = hotkeys::ring_edit(&original, Some(id), None, key, 42.)
                .context("Unmapped ring")?
                .map_err(anyhow::Error::msg)?;
            if key == "3" && rotation == 0. {
                fs::write(
                    directory.join("angle-fixed.rsk"),
                    serde_json::to_vec_pretty(&result)?,
                )?;
            }
            cases.push((
                "Ring attachment".into(),
                format!("Ring {key}, {rotation} degrees"),
                result,
            ));
        }
    }
    let part = reshiki::rings::Preset::Regular.document(42., false);
    let bond = part.bonds.first().context("Missing ring edge")?;
    for key in ["v", "4", "5", "6", "7", "8", "a", "z", "9", "0"] {
        let (result, _) = hotkeys::ring_edit(&part, None, Some((bond.a, bond.b)), key, 42.)
            .context("Unmapped fusion")?
            .map_err(anyhow::Error::msg)?;
        cases.push(("Ring fusion".into(), format!("Fuse {key}"), result));
    }
    for key in ["1", "2", "3", "d", "D", "b", "B", "w", "h", "W", "H", "y"] {
        let (mut doc, ids) = chain(4);
        let a = *ids.get(1).context("Missing atom")?;
        let b = *ids.get(2).context("Missing atom")?;
        doc = hotkeys::bond_edit(
            &doc,
            a,
            b,
            hotkeys::bond_preset(key).context("Unmapped bond")?,
        )
        .map_err(anyhow::Error::msg)?;
        cases.push(("Bonds".into(), format!("Bond {key}"), doc));
    }
    for position in [
        reshiki::bonds::DoublePosition::Left,
        reshiki::bonds::DoublePosition::Center,
        reshiki::bonds::DoublePosition::Right,
    ] {
        let (mut doc, ids) = chain(3);
        if let [a, b, ..] = ids.as_slice() {
            doc.add_bond(*a, *b, 2, "plain");
        }
        if let Some(b) = doc.bonds.first_mut() {
            b.double_position = position;
        }
        cases.push(("Bonds".into(), format!("Double {position}"), doc));
    }
    for position in [
        reshiki::bonds::DoublePosition::Auto,
        reshiki::bonds::DoublePosition::Left,
        reshiki::bonds::DoublePosition::Right,
        reshiki::bonds::DoublePosition::Center,
    ] {
        for reverse in [false, true] {
            let (mut doc, ids) = chain(4);
            let a = *ids.get(1).context("Missing atom")?;
            let b = *ids.get(2).context("Missing atom")?;
            reshiki::bonds::BondPreset::BoldDouble.place(&mut doc, a, b);
            let bond = doc
                .bonds
                .iter_mut()
                .find(|e| e.a == a && e.b == b)
                .context("Missing bold double")?;
            bond.double_position = position;
            if reverse {
                bond.reverse();
            }
            cases.push((
                "Bonds".into(),
                format!(
                    "Bold double {position}{}",
                    if reverse { ", reversed" } else { "" }
                ),
                doc,
            ));
        }
    }
    for variant in ["ring", "labeled", "colored", "three-way"] {
        let mut doc = if variant == "ring" {
            let mut ring = Document::default();
            editing::ring(&mut ring, Point::default(), 6, false, 0.);
            ring
        } else {
            chain(4).0
        };
        reshiki::bonds::BondPreset::BoldDouble.apply(doc.bonds.get_mut(1).context("Missing edge")?);
        if variant == "labeled" {
            doc.atoms.get_mut(1).context("Missing atom")?.element = "N".into();
        }
        if variant == "colored" {
            for b in &mut doc.bonds {
                b.color = [180, 68, 32];
            }
        }
        if variant == "three-way" {
            let atom = doc.atoms.get(1).context("Missing atom")?;
            let (id, position) = (atom.id, atom.position);
            let other = doc.add_atom("C", position.offset(0., -42.));
            doc.add_bond(id, other, 1, "plain");
        }
        cases.push(("Bonds".into(), format!("Bold double, {variant}"), doc));
    }
    for (label, transform) in [
        ("Rotate 15", Transform::Rotate(15.)),
        ("Tilt X 12", Transform::TiltX(12.)),
        ("Tilt Y -12", Transform::TiltY(-12.)),
        ("Flip horizontal", Transform::FlipHorizontal),
        ("Flip vertical", Transform::FlipVertical),
    ] {
        let (mut result, _) = hotkeys::ring_edit(&doc, Some(id), None, "3", 42.)
            .context("Missing ring")?
            .map_err(anyhow::Error::msg)?;
        let ids = result.all_ids();
        editing::transform(&mut result, &ids, transform);
        cases.push(("Transforms".into(), label.into(), result));
    }
    let (mut joined, ends) = chain(2);
    let more = editing::append(&mut joined, &chain(2).0, Point::new(160., 40.));
    let source = *ends.last().context("Missing source")?;
    let target = *more.first().context("Missing target")?;
    let prepared =
        reshiki::joining::Prepared::new(&joined, &[source]).map_err(anyhow::Error::msg)?;
    let target_point = joined.atom(target).context("Missing target")?.position;
    let (joined, _) = prepared
        .place(
            target_point,
            None,
            1.,
            Anchor::Atom(source),
            Connection::ShareAtom,
        )
        .map_err(anyhow::Error::msg)?;
    cases.push(("Join".into(), "Join two carbon endpoints".into(), joined));
    let mut text_doc = Document::default();
    let mut format = reshiki::typography::TextFormat::default();
    format.style.formula = true;
    text_doc.annotations.push(Annotation {
        id: 1,
        position: Point::default(),
        text: "C2H5 + NH3 → C2H5NH2".into(),
        format,
    });
    cases.push(("Text".into(), "Formula text".into(), text_doc));

    let runtime = tokio::runtime::Runtime::new()?;
    let engine = LocalEngine::default();
    let mut html = String::from(
        "<!doctype html><meta charset='utf-8'><title>Drawing shortcut review</title><style>body{font:16px system-ui;background:#f4f5f4;color:#202824;margin:32px}h1{font-size:28px}.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(300px,1fr));gap:16px}article{background:white;padding:20px;border:1px solid #d5ddd9;border-radius:12px}img{width:100%;height:210px;object-fit:contain}p{font-size:13px;color:#53605a}a{color:#286655}</style><h1>Drawing shortcut review</h1><p>Generated using the editor's graph operations and figure renderer. Chemical analysis results are listed separately from visual inspection. Files and this review stay outside Git.</p><div class='grid'>",
    );
    let mut sheets = std::collections::BTreeMap::<String, Vec<(String, Document)>>::new();
    for (index, (category, title, mut doc)) in cases
        .into_iter()
        .filter(|(category, _, _)| {
            category_filter
                .as_ref()
                .is_none_or(|filter| filter.eq_ignore_ascii_case(category))
        })
        .enumerate()
    {
        doc.validate().map_err(anyhow::Error::msg)?;
        let mut request = Request::molecule("analyze", doc.clone());
        request.selected_ids = None;
        let analysis = if doc.atoms.is_empty() {
            "Drawing objects".into()
        } else {
            match runtime.block_on(engine.request(request)) {
                Ok(response) => {
                    if let Some(checked) = response.document {
                        reshiki::atom_labels::refresh_computed(&mut doc, &checked);
                    }
                    response
                        .analysis
                        .map(|a| format!("{} · {}", a.formula, a.smiles))
                        .unwrap_or_else(|| "No molecular identity".into())
                }
                Err(error) => format!("Analysis: {error}"),
            }
        };
        let name = format!("case-{index:03}");
        fs::write(
            directory.join(format!("{name}.rsk")),
            serde_json::to_vec_pretty(&doc)?,
        )?;
        for format in ["svg", "png", "pdf"] {
            fs::write(
                directory.join(format!("{name}.{format}")),
                reshiki::export::drawing(&doc, format).map_err(anyhow::Error::msg)?,
            )?;
        }
        let escape = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        };
        html.push_str(&format!("<article><h3>{}</h3><img src='{name}.svg'><p>{}</p><a href='{name}.rsk'>Editable drawing</a> · <a href='{name}.pdf'>PDF</a></article>",escape(&title),escape(&analysis)));
        sheets.entry(category).or_default().push((title, doc));
    }
    html.push_str("</div>");
    fs::write(directory.join("index.html"), html)?;
    for (category, cases) in sheets {
        for (page, chunk) in cases.chunks(12).enumerate() {
            let mut sheet = Document::default();
            for (index, (title, doc)) in chunk.iter().enumerate() {
                let (lo, hi) = doc.bounds();
                let center = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
                let mut positioned = doc.clone();
                let scale = (180. / (hi.x - lo.x).max(180.)).min(130. / (hi.y - lo.y).max(130.));
                let ids = positioned.all_ids();
                editing::transform_about(&mut positioned, &ids, center, scale, 0.);
                let dest = Point::new(
                    (index % 3) as f32 * 250. + 125.,
                    (index / 3) as f32 * 220. + 90.,
                );
                editing::append(
                    &mut sheet,
                    &positioned,
                    Point::new(dest.x - center.x, dest.y - center.y),
                );
                sheet.annotations.push(Annotation {
                    id: sheet.next_id(),
                    position: Point::new(dest.x - 100., dest.y + 85.),
                    text: title
                        .replace("Group ", "")
                        .replace("Growth ", "")
                        .replace("Bold double", "Bold ×2")
                        .replace(" of bond", "")
                        .replace("Automatic", "Auto")
                        .replace(", reversed", " ↔")
                        .replace(" degrees", "°"),
                    format: reshiki::typography::TextFormat {
                        style: reshiki::typography::TextStyle {
                            size_pt: 6.,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                });
            }
            let name = format!("sheet-{}-{page}", category.replace(' ', "-").to_lowercase());
            for format in ["svg", "png"] {
                fs::write(
                    directory.join(format!("{name}.{format}")),
                    reshiki::export::drawing(&sheet, format).map_err(anyhow::Error::msg)?,
                )?;
            }
            fs::write(
                directory.join(format!("{name}.rsk")),
                serde_json::to_vec_pretty(&sheet)?,
            )?;
        }
    }
    Ok(())
}
