//! #870 draws a MOL file's SUP groups as collapsed labels; the original worker
//! drew them expanded. Check each label against the group RDKit read, require
//! every group that can be one label to collapse, then take the labels and the
//! expanded-group notice into the expectation.
use anyhow::Context;
use reshiki::engine::Response;
use serde::Deserialize;
use std::collections::HashSet;

#[derive(Deserialize)]
pub struct Group {
    label: String,
    atoms: Vec<u64>,
    attachments: Vec<(u64, String)>,
    expanded: bool,
}

pub fn adapt(groups: &[Group], actual: &Response, expected: &mut Response) -> anyhow::Result<()> {
    let (Some(drawn), Some(document)) = (&actual.document, expected.document.as_mut()) else {
        return Ok(());
    };
    let ids: HashSet<_> = document.atoms.iter().map(|atom| atom.id).collect();
    // Bonds and attachment-to-target links, as collapsing reads them.
    let bonds: Vec<_> = reshiki::attachments::edges(document).collect();
    // Members the drawing keeps; a group without any has nothing to collapse.
    let mut open: Vec<_> = groups
        .iter()
        .filter(|group| !group.expanded)
        .map(|group| {
            let mut members: Vec<_> = group
                .atoms
                .iter()
                .copied()
                .filter(|id| ids.contains(id))
                .collect();
            members.sort_unstable();
            members.dedup();
            (group, members)
        })
        .filter(|(_, members)| !members.is_empty())
        .collect();
    // A group can be one label when its label fits, its atoms are connected,
    // at most two of them bond outside it, and any SAPs name exactly those.
    let collapsible = |group: &Group, members: &[u64]| {
        let inside = |id: &u64| members.contains(id);
        let boundary: HashSet<_> = bonds
            .iter()
            .filter(|(a, b)| inside(a) != inside(b))
            .map(|(a, b)| if inside(a) { *a } else { *b })
            .collect();
        let points: HashSet<_> = group.attachments.iter().map(|(atom, _)| *atom).collect();
        let mut reached = vec![members[0]];
        let mut index = 0;
        while let Some(&atom) = reached.get(index) {
            for (a, b) in &bonds {
                let next = if *a == atom {
                    b
                } else if *b == atom {
                    a
                } else {
                    continue;
                };
                if inside(next) && !reached.contains(next) {
                    reached.push(*next);
                }
            }
            index += 1;
        }
        !group.label.trim().is_empty()
            && group.label.chars().count() <= 32
            && !group.label.chars().any(char::is_control)
            && reached.len() == members.len()
            && boundary.len() <= 2
            && points.len() <= 2
            && (points.is_empty() || points.iter().all(inside) && boundary.is_subset(&points))
    };
    for abbreviation in &drawn.abbreviations {
        let mut members = abbreviation.members.clone();
        members.sort_unstable();
        let index = open
            .iter()
            .position(|(group, ids)| group.label.trim() == abbreviation.label && *ids == members)
            .with_context(|| format!("No SUP group matches label {:?}", abbreviation.label))?;
        let (group, _) = open.remove(index);
        // Explicit SAPs set the anchor and exit: Al (N side) before Br.
        let mut points: Vec<&(u64, String)> = Vec::new();
        for point in &group.attachments {
            if !points.iter().any(|other| other.0 == point.0) {
                points.push(point);
            }
        }
        points.sort_by_key(|(_, id)| id == "Br");
        if let [(anchor, _), rest @ ..] = points.as_slice() {
            anyhow::ensure!(
                abbreviation.anchor == *anchor
                    && abbreviation.exit == rest.first().map(|(atom, _)| *atom),
                "{:?} ignores its SAP attachment points",
                abbreviation.label
            );
        }
    }
    // A group overlapping an earlier collapsed one stays expanded.
    for (group, members) in &open {
        let overlaps = drawn
            .abbreviations
            .iter()
            .any(|other| other.members.iter().any(|id| members.contains(id)));
        anyhow::ensure!(
            overlaps || !collapsible(group, members),
            "SUP group {:?} stayed expanded although it can be one label",
            group.label
        );
    }
    let notice = actual
        .warnings
        .iter()
        .position(|warning| warning.starts_with("Kept ") && warning.contains(" superatom group"));
    match notice {
        Some(index) => {
            let warning = &actual.warnings[index];
            anyhow::ensure!(
                warning.starts_with(&format!("Kept {} superatom group", open.len())),
                "{warning:?} does not count the {} uncollapsed SUP groups",
                open.len()
            );
            expected
                .warnings
                .insert(index.min(expected.warnings.len()), warning.clone());
        }
        None => anyhow::ensure!(
            open.is_empty(),
            "{} SUP groups stayed expanded without a notice",
            open.len()
        ),
    }
    if !drawn.abbreviations.is_empty() {
        // Collapsed and residue labels need a newer document version.
        document.version = drawn.version;
        document.abbreviations = drawn.abbreviations.clone();
    }
    Ok(())
}
