//! Collapsed drawing groups use CTfile superatoms without changing chemistry.
use super::{Error, Molecule, invalid};
use crate::document::Document;
use std::collections::{HashMap, HashSet};
use std::fmt::Write;

pub(super) fn restore_output(
    mut mol: String,
    doc: &Document,
    molecule: &Molecule,
) -> Result<String, Error> {
    if doc.abbreviations.is_empty() {
        return Ok(mol);
    }
    let indices: HashMap<_, _> = molecule
        .ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, i + 1))
        .collect();
    let groups: Vec<_> = doc
        .abbreviations
        .iter()
        .filter_map(|group| {
            group
                .members
                .iter()
                .map(|id| indices.get(id).copied())
                .collect::<Option<Vec<_>>>()
                .map(|members| (group, members))
        })
        .collect();
    if groups.is_empty() {
        return Ok(mol);
    }
    let v3000 = mol.lines().any(|line| line == "M  V30 BEGIN CTAB");
    let mut lines: Vec<_> = mol.lines().map(str::to_owned).collect();
    let mut previous = 0usize;
    if v3000 {
        let counts = lines
            .iter_mut()
            .find(|line| line.starts_with("M  V30 COUNTS "))
            .ok_or_else(|| invalid("Missing generated V3000 counts"))?;
        let fields: Vec<_> = counts.split_whitespace().collect();
        let [_, _, _, atoms, bonds, count, objects, chiral] = fields.as_slice() else {
            return Err(invalid("Invalid generated V3000 counts"));
        };
        previous = count
            .parse()
            .map_err(|_| invalid("Invalid generated V3000 Sgroup count"))?;
        *counts = format!(
            "M  V30 COUNTS {atoms} {bonds} {} {objects} {chiral}",
            previous + groups.len()
        );
    }
    let mut records = String::new();
    if v3000 && previous == 0 {
        records.push_str("M  V30 BEGIN SGROUP\n");
    }
    for (i, (group, members)) in groups.iter().enumerate() {
        let number = previous + i + 1;
        let member_set: HashSet<_> = members.iter().copied().collect();
        let mut bonds = Vec::new();
        for (i, bond) in molecule.state.graph.bonds.iter().enumerate() {
            let (a, b) = (bond.a + 1, bond.b + 1);
            if member_set.contains(&a) != member_set.contains(&b) {
                bonds.push(i + 1);
            }
        }
        let attachments: Vec<_> =
            std::iter::once((group.anchor, if group.exit.is_some() { "Al" } else { "1" }))
                .chain(group.exit.map(|id| (id, "Br")))
                .filter_map(|(id, label)| indices.get(&id).map(|&atom| (atom, label)))
                .collect();
        if v3000 {
            let mut record = format!("{number} SUP 0 ATOMS=({}", members.len());
            for atom in members {
                write!(record, " {atom}")?;
            }
            write!(record, ") XBONDS=({}", bonds.len())?;
            for bond in bonds {
                write!(record, " {bond}")?;
            }
            // Quoting every label covers spaces, parentheses and continuation dashes.
            write!(record, ") LABEL=\"{}\"", group.label.replace('"', "\"\""))?;
            for (atom, label) in attachments {
                write!(record, " SAP=(3 {atom} 0 {label})")?;
            }
            // Continuations concatenate verbatim, including inside quoted labels.
            let mut rest = record.as_str();
            while rest.len() > 70 {
                let end = rest.floor_char_boundary(70);
                let (line, tail) = rest
                    .split_at_checked(end)
                    .ok_or_else(|| invalid("Invalid superatom record"))?;
                writeln!(records, "M  V30 {line}-")?;
                rest = tail;
            }
            writeln!(records, "M  V30 {rest}")?;
        } else {
            writeln!(records, "M  STY  1 {number:3} SUP")?;
            for (kind, entries) in [("SAL", members.as_slice()), ("SBL", bonds.as_slice())] {
                for chunk in entries.chunks(15) {
                    write!(records, "M  {kind} {number:3} {:2}", chunk.len())?;
                    for entry in chunk {
                        write!(records, " {entry:3}")?;
                    }
                    records.push('\n');
                }
            }
            writeln!(records, "M  SMT {number:3} {}", group.label)?;
            for chunk in attachments.chunks(6) {
                write!(records, "M  SAP {number:3} {:2}", chunk.len())?;
                for (atom, label) in chunk {
                    write!(records, " {atom:3}   0 {label:>2}")?;
                }
                records.push('\n');
            }
        }
    }
    if v3000 && previous == 0 {
        records.push_str("M  V30 END SGROUP\n");
    }
    let end = if v3000 {
        if previous == 0 {
            "M  V30 END CTAB"
        } else {
            "M  V30 END SGROUP"
        }
    } else {
        "M  END"
    };
    let index = lines
        .iter()
        .position(|line| line == end)
        .ok_or_else(|| invalid("Missing generated MOL group boundary"))?;
    lines.splice(index..index, records.lines().map(str::to_owned));
    mol = lines.join("\n");
    mol.push('\n');
    Ok(mol)
}
