//! #870 writes each collapsed abbreviation of a MOL export as a SUP group over
//! its real atoms; the original worker wrote none. Read the groups back with
//! RDKit, require exactly the drawing's labels, members and attachment points,
//! then drop the records so the rest of the export is compared.
use anyhow::Context;
use reshiki::document::Document;
use serde::Deserialize;
use std::{
    collections::HashMap,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

#[derive(Debug, PartialEq, Deserialize)]
struct Group {
    label: String,
    atoms: Vec<u64>,
    attachments: Vec<(u64, String)>,
    expanded: bool,
}

/// `ids` are the exported atoms' drawing IDs in MOL order.
pub fn adapt(document: &Document, ids: &[u64], output: &mut String) -> anyhow::Result<()> {
    if document.abbreviations.is_empty() {
        return Ok(());
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let python = root.join(if cfg!(windows) {
        ".venv/Scripts/python.exe"
    } else {
        ".venv/bin/python"
    });
    let mut child = Command::new(python)
        .arg(root.join("reference/superatom_export_reference.py"))
        .env("PYTHONUTF8", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    child
        .stdin
        .take()
        .context("Missing superatom input")?
        .write_all(output.as_bytes())?;
    let read = child.wait_with_output()?;
    anyhow::ensure!(
        read.status.success(),
        "Independent SUP group reader failed: {}",
        String::from_utf8_lossy(&read.stderr)
    );
    let mut groups: Vec<Group> = serde_json::from_slice(&read.stdout)?;
    for group in &mut groups {
        group.atoms.sort_unstable();
        // RDKit keeps the padding of the two-character SAP id field.
        for (_, id) in &mut group.attachments {
            *id = id.trim().to_owned();
        }
    }
    let indices: HashMap<_, _> = ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i as u64 + 1))
        .collect();
    for abbreviation in &document.abbreviations {
        // The export skips a group whose members are not all exported atoms.
        let Some(mut atoms) = abbreviation
            .members
            .iter()
            .map(|id| indices.get(id).copied())
            .collect::<Option<Vec<_>>>()
        else {
            continue;
        };
        atoms.sort_unstable();
        let one_sided = abbreviation.exit.is_none();
        let expected = Group {
            label: abbreviation.label.clone(),
            atoms,
            attachments: std::iter::once((abbreviation.anchor, if one_sided { "1" } else { "Al" }))
                .chain(abbreviation.exit.map(|id| (id, "Br")))
                .map(|(id, point)| (indices[&id], point.to_owned()))
                .collect(),
            expanded: false,
        };
        let index = groups
            .iter()
            .position(|group| *group == expected)
            .with_context(|| format!("No SUP group is the collapsed label {:?}", expected))?;
        groups.remove(index);
    }
    anyhow::ensure!(
        groups.is_empty(),
        "SUP groups that are no collapsed label: {groups:?}"
    );
    *output = output
        .lines()
        .filter(|line| {
            !matches!(
                line.get(..6),
                Some("M  STY" | "M  SAL" | "M  SBL" | "M  SMT" | "M  SAP")
            )
        })
        .map(|line| format!("{line}\n"))
        .collect();
    Ok(())
}
