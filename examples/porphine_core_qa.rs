//! Actual application-renderer fixtures for placement and reusable construction.
use anyhow::{Context, ensure};
use reshiki::{
    atom_labels::HydrogenPosition,
    document::{Annotation, Document, Point},
    editing::{
        self,
        reference::{self, Edge, Stretch},
    },
    engine::{ChemistryEngine, LocalEngine, Request},
    templates::{Anchor, Connection, LIBRARY},
};
use std::{collections::HashMap, fs, path::Path};

fn write(directory: &Path, name: &str, doc: &Document) -> anyhow::Result<()> {
    doc.validate().map_err(anyhow::Error::msg)?;
    fs::write(
        directory.join(format!("{name}.rsk")),
        doc.file_json().map_err(anyhow::Error::msg)?,
    )?;
    for format in ["svg", "png", "pdf"] {
        fs::write(
            directory.join(format!("{name}.{format}")),
            reshiki::export::drawing(doc, format).map_err(anyhow::Error::msg)?,
        )?;
    }
    Ok(())
}

fn construction(core: &Document) -> anyhow::Result<Vec<(&'static str, Document)>> {
    let seed_ids = vec![1, 2, 3, 4, 5, 21];
    let mut seed = editing::selection(core, &seed_ids);
    for atom in &mut seed.atoms {
        atom.element = "C".into();
        atom.label_h = 0;
    }
    for bond in &mut seed.bonds {
        bond.order = 1;
    }
    let plan = Stretch::new(&seed, 4, 5).map_err(anyhow::Error::msg)?;
    seed = plan.apply(&seed, 63.).map_err(anyhow::Error::msg)?;
    (seed, _) = reference::rotate(&seed, &seed_ids, Point::default(), 17.3, false)
        .map_err(anyhow::Error::msg)?;
    let raw = seed.clone();
    let plan = Stretch::new(&seed, 4, 5).map_err(anyhow::Error::msg)?;
    seed = plan.apply(&seed, 42.).map_err(anyhow::Error::msg)?;
    let angle = reference::alignment_degrees(&seed, Edge::Bond(2, 3), 0., false)
        .map_err(anyhow::Error::msg)?;
    (seed, _) = reference::rotate(&seed, &seed_ids, Point::default(), angle, false)
        .map_err(anyhow::Error::msg)?;
    let aligned = seed.clone();
    let mut map: HashMap<u64, u64> = seed_ids.iter().copied().map(|id| (id, id)).collect();
    for q in 1_u64..=3 {
        let (copied, ids) =
            reference::rotate(&seed, &seed_ids, Point::default(), q as f32 * 90., true)
                .map_err(anyhow::Error::msg)?;
        for (original, id) in seed_ids.iter().copied().zip(ids) {
            let locant = if original == 21 {
                21 + q
            } else {
                original + 5 * q
            };
            map.insert(locant, id);
        }
        seed = copied;
    }
    let copies = seed.clone();
    let mapped = |locant: u64| {
        map.get(&locant)
            .copied()
            .context("Construction locant mapping")
    };
    for (a, b) in [(5, 6), (10, 11), (15, 16), (20, 1)] {
        seed.add_bond(mapped(a)?, mapped(b)?, 1, "plain");
    }
    let closed = seed.clone();
    for source in &core.bonds {
        let (a, b) = (mapped(source.a)?, mapped(source.b)?);
        let bond = seed
            .bonds
            .iter_mut()
            .find(|bond| (bond.a == a && bond.b == b) || (bond.a == b && bond.b == a))
            .context("Construction bond")?;
        bond.order = source.order;
    }
    for source in core.atoms.iter().filter(|a| a.element == "N") {
        let atom = seed
            .atom_mut(mapped(source.id)?)
            .context("Construction nitrogen")?;
        atom.element = "N".into();
        atom.label_h = source.label_h;
        atom.display.hydrogen_position = if source.id == 21 {
            HydrogenPosition::Below
        } else if source.id == 23 {
            HydrogenPosition::Above
        } else {
            HydrogenPosition::Auto
        };
    }
    seed.validate().map_err(anyhow::Error::msg)?;
    Ok(vec![
        ("Seed: 17.3°; arm 21.6 pt", raw),
        ("Exact alignment; arm 14.4 pt", aligned),
        ("Three copies about one pinned center", copies),
        ("Close four links: carbon scaffold", closed),
        ("Assign N/bonds: validated free base", seed),
    ])
}

fn gallery(cases: &[(&str, Document)]) -> Document {
    let mut doc = Document::default();
    for (i, (label, part)) in cases.iter().enumerate() {
        let (lo, hi) = part.bounds();
        let center = Point::new(180. + (i % 2) as f32 * 460., 180. + (i / 2) as f32 * 410.);
        editing::append(
            &mut doc,
            part,
            Point::new(
                center.x - (lo.x + hi.x) * 0.5,
                center.y - (lo.y + hi.y) * 0.5,
            ),
        );
        doc.annotations.push(Annotation {
            id: doc.next_id(),
            position: center.offset(-165., 160.),
            text: (*label).into(),
            format: reshiki::typography::TextFormat {
                style: reshiki::typography::TextStyle {
                    size_pt: 6.,
                    ..Default::default()
                },
                width_pt: Some(140.),
                ..Default::default()
            },
        });
    }
    doc
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let output = std::env::args_os()
        .nth(1)
        .context("Usage: porphine_core_qa OUTPUT_DIRECTORY")?;
    let directory = Path::new(&output);
    fs::create_dir_all(directory)?;
    let item = LIBRARY
        .iter()
        .find(|t| t.name == "Porphine (21H,23H)")
        .context("Porphine template")?;
    let core = &item.document;
    let mut cases = construction(core)?;
    let engine = LocalEngine::default();
    for name in ["template", "constructed"] {
        let doc = if name == "template" {
            core
        } else {
            &cases.last().context("Final construction")?.1
        };
        let analysis = engine
            .execute(Request::molecule("analyze", doc.clone()))
            .await
            .map_err(anyhow::Error::msg)?
            .analysis
            .context("Chemical identity")?;
        ensure!(
            analysis.formula == "C20H14N4" && analysis.inchikey == "RKCAIXNGYQCCAL-CEVVSZFKSA-N",
            "{name}: chemical identity mismatch"
        );
        fs::write(
            directory.join(format!("{name}-identity.json")),
            serde_json::to_string_pretty(&analysis)?,
        )?;
    }
    let benzene = LIBRARY
        .iter()
        .find(|t| t.name == "Benzene")
        .context("Benzene template")?;
    let p = core.atom(5).context("Meso position 5")?.position;
    let (mut joined, _) = benzene
        .place(
            core,
            p,
            Some(p.offset(42., -42.)),
            5.,
            Anchor::Atom(1),
            Connection::Connect,
        )
        .map_err(anyhow::Error::msg)?;
    let bridge = joined
        .bonds
        .iter()
        .find(|b| (b.a <= 24) != (b.b <= 24))
        .context("Phenyl bridge")?;
    let (fixed, moving) = if bridge.a <= 24 {
        (bridge.a, bridge.b)
    } else {
        (bridge.b, bridge.a)
    };
    let plan = Stretch::new(&joined, fixed, moving).map_err(anyhow::Error::msg)?;
    let mut stretched = plan
        .apply(&joined, plan.length * 1.5)
        .map_err(anyhow::Error::msg)?;
    let before = engine
        .execute(Request::molecule("analyze", joined.clone()))
        .await
        .map_err(anyhow::Error::msg)?;
    let after = engine
        .execute(Request::molecule("analyze", stretched.clone()))
        .await
        .map_err(anyhow::Error::msg)?;
    // Bond creation invalidates computed labels. Match the application's checked
    // label refresh while retaining this fixture's coordinates and bond phase.
    reshiki::atom_labels::refresh_computed(
        &mut joined,
        before.document.as_ref().context("Checked phenyl labels")?,
    );
    reshiki::atom_labels::refresh_computed(
        &mut stretched,
        after
            .document
            .as_ref()
            .context("Checked stretched labels")?,
    );
    for doc in [&joined, &stretched] {
        for nitrogen in core.atoms.iter().filter(|atom| atom.element == "N") {
            ensure!(
                doc.atom(nitrogen.id).context("Core nitrogen")?.label_h == nitrogen.label_h,
                "Attachment must retain the two opposite displayed N–H sites"
            );
        }
    }
    let before = before.analysis.context("Phenyl identity")?;
    let after = after.analysis.context("Stretched phenyl identity")?;
    ensure!(
        before.formula == "C26H18N4" && after.inchikey == before.inchikey,
        "Phenyl bridge stretch must keep chemical identity"
    );
    fs::write(
        directory.join("phenyl-identity.json"),
        serde_json::to_string_pretty(&after)?,
    )?;
    cases.push(("Attach one phenyl at a meso carbon", joined));
    cases.push(("Stretch only its bridge to 21.6 pt", stretched));
    write(directory, "porphine-template", core)?;
    for (i, (_, doc)) in cases.iter().enumerate() {
        write(directory, &format!("stage-{i}"), doc)?;
    }
    write(directory, "porphine-workflow", &gallery(&cases))?;
    fs::write(
        directory.join("actions.txt"),
        "Actual application-renderer outputs; manual desktop step counts are not measured.\nFast path: Templates > Macrocycles > Porphine (21H,23H), or search porphyrin. Place once.\nConstruction fixture stage0: carbon five-ring and meso arm, reference2→3 at17.3°, arm21.6pt. Stretch4→5 to14.4pt, align2→3 horizontally, Pinned point X0/Y0. Copy original seed at90/180/270°. Close meso→next-ring links5→6,10→11,15→16,20→1 using the saved locant mapping. Then assign the four inward N sites and frozen bond phase; only stage4 is asserted porphine.\nStages5/6: connect Benzene source atom1 to core meso atom5, outward direction−45°. The core and benzene remain rigid when the bridge is stretched. No metal or derivative library is added.\n",
    )?;
    println!(
        "Wrote validated template, seven construction/attachment stages and actual SVG/PNG/PDF outputs to {}",
        directory.display()
    );
    Ok(())
}
