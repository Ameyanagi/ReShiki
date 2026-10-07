use super::*;
use reshiki::{
    document::{Arrow, Point},
    reactions::{Participant, Reaction},
};

#[test]
fn public_fixture_checks_fit_one_megabyte_stack() -> anyhow::Result<()> {
    std::env::var_os("RESHIKI_INCHI_HELPER")
        .context("Set RESHIKI_INCHI_HELPER to the built native helper")?;
    let directory = tempfile::tempdir()?;
    let output = directory.path().join("handoff");
    let thread_output = output.clone();
    std::thread::Builder::new()
        .name("one-megabyte-handoff".into())
        .stack_size(1024 * 1024)
        .spawn(move || {
            let fixtures = generate_fixtures(&thread_output);
            println!(
                "Handoff driver future: {} bytes; execution stack: 1048576 bytes",
                std::mem::size_of_val(&fixtures)
            );
            tokio::runtime::Runtime::new()?.block_on(fixtures)
        })?
        .join()
        .map_err(|_| anyhow::anyhow!("Handoff fixture thread panicked"))??;
    let manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(output.join("manifest.json"))?)?;
    ensure!(manifest["fixtures"].as_array().context("fixtures")?.len() == 5);
    Ok(())
}

fn reaction() -> Document {
    let mut doc = Document::default();
    let mut sides = Vec::new();
    for (offset, order) in [(0., 1), (240., 2)] {
        let a = doc.add_atom("C", Point::new(offset, 0.));
        let b = doc.add_atom("C", Point::new(offset + 42., 0.));
        let c = doc.add_atom("O", Point::new(offset + 84., 0.));
        doc.add_bond(a, b, 1, "plain");
        doc.add_bond(b, c, order, "plain");
        sides.push(Participant {
            atoms: vec![a, b, c],
            coefficient: 1,
        });
    }
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(130., 0.),
        Point::new(190., 0.),
        Default::default(),
        Default::default(),
    ));
    let mut reaction = Reaction::new(arrow);
    reaction.reactants.push(sides.remove(0));
    reaction.products.push(sides.remove(0));
    doc.reactions.push(reaction);
    doc
}

#[test]
fn reaction_oracle_detects_role_and_connectivity_loss() -> anyhow::Result<()> {
    let doc = reaction();
    let expected = reaction_oracle(&doc)?;
    assert_eq!(expected.reaction_smiles, "CCO>>CC=O");
    let mut swapped = doc.clone();
    let reaction = &mut swapped.reactions[0];
    std::mem::swap(&mut reaction.reactants, &mut reaction.products);
    assert_ne!(reaction_oracle(&swapped)?, expected);
    let mut changed = doc.clone();
    changed.bonds.last_mut().context("product bond")?.order = 1;
    assert_ne!(reaction_oracle(&changed)?, expected);
    let mut unassigned = doc;
    unassigned.reactions.clear();
    assert!(reaction_oracle(&unassigned).is_err());
    Ok(())
}
