//! Issue #377 evidence: the glucose template with OBn at C4 and MeO at C1,
//! typed through the model's label entry and exported with ReShiki's PNG path.
use anyhow::Context;
use reshiki::{
    atom_text::{Mode, apply},
    haworth::{Anomer, Sugar, sugar_document},
};
use std::{fs, path::PathBuf};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let directory = PathBuf::from(args.next().context("Usage: ev377 OUT PREFIX")?);
    let prefix = args.next().context("PREFIX")?.into_string().ok().context("PREFIX")?;
    fs::create_dir_all(&directory)?;
    let doc = sugar_document(
        Sugar::Glucose,
        Anomer::Alpha,
        reshiki::style::DEFAULT.bond_length_world,
    )
    .map_err(anyhow::Error::msg)?;
    // C1's OH oxygen is atom 7; C4's HO oxygen is atom 10.
    for (name, id, typed) in [("template", 7, None), ("c4-OBn", 10, Some("OBn")), ("c1-MeO", 7, Some("MeO"))] {
        let atom = doc.atom(id).context("Oxygen")?;
        anyhow::ensure!(atom.element == "O", "atom {id} is {}", atom.element);
        let mut result = match typed {
            Some(text) => apply(&doc, id, text, Mode::Auto).map_err(anyhow::Error::msg)?,
            None => doc.clone(),
        };
        // Apply clears computed hydrogens until the app's chemistry check runs;
        // the other hydroxyls are unchanged, so restore their template counts.
        for a in result.atoms.iter_mut().filter(|a| a.id != id) {
            if let Some(original) = doc.atom(a.id) {
                a.label_h = original.label_h;
            }
        }
        let drawn = result
            .abbreviation(id)
            .map(|g| format!("group {} drawn {} ({:?})", g.label, g.text(&result), g.alignment))
            .unwrap_or_else(|| {
                let a = result.atom(id).expect("atom");
                format!("atom {} variable {:?}", a.element, a.display.variable)
            });
        println!("{prefix} {name}: {drawn}");
        fs::write(
            directory.join(format!("{prefix}-{name}.png")),
            reshiki::export::drawing(&result, "png").map_err(anyhow::Error::msg)?,
        )?;
    }
    Ok(())
}
