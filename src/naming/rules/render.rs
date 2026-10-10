use super::*;

pub(super) fn build(c: &mut Context, parent: Vec<usize>, kind: ParentKind) -> Result<Ast, String> {
    let locant_map = parent
        .iter()
        .enumerate()
        .map(|(i, &a)| (a, i + 1))
        .collect::<BTreeMap<_, _>>();
    let mut covered = parent.iter().copied().collect::<BTreeSet<_>>();
    let mut ast = Ast {
        parent,
        kind,
        senior: c.senior(),
        suffix_locants: vec![],
        prefixes: vec![],
        doubles: vec![],
        triples: vec![],
        stereo: vec![],
        ester_alcohol: None,
    };
    let groups = c.groups.clone();
    for group in groups {
        let locant = *locant_map
            .get(&group.anchor)
            .ok_or("Functional groups in carbon branches are outside this naming profile")?;
        covered.extend(group.atoms.iter().copied());
        if Some(group.kind) == ast.senior {
            ast.suffix_locants.push(locant);
            if group.kind == GroupKind::Ester {
                if ast.ester_alcohol.is_some() {
                    return Err(
                        "Multiple ester groups require unsupported ester nomenclature".into(),
                    );
                }
                let alcohol = group.alcohol.ok_or("Missing ester alcohol component")?;
                let (text, atoms) = alkyl(c, alcohol, &covered, 0)?;
                covered.extend(atoms);
                ast.ester_alcohol = Some(text);
            }
        } else {
            let text =
                match group.kind {
                    GroupKind::Alcohol => "hydroxy",
                    GroupKind::Amine => "amino",
                    GroupKind::Ketone | GroupKind::Aldehyde => "oxo",
                    _ => return Err(
                        "This combination of subordinate suffix functional groups is unsupported"
                            .into(),
                    ),
                };
            ast.prefixes.push(Prefix {
                locant,
                text: text.into(),
                complex: false,
            });
        }
    }
    ast.suffix_locants.sort();
    if matches!(ast.kind, ParentKind::Retained(_)) && ast.suffix_locants.len() > 1 {
        return Err(
            "Multiple suffix groups on retained aromatic parents are outside this profile".into(),
        );
    }
    let is_ring = !matches!(ast.kind, ParentKind::Chain);
    for (i, &a) in ast.parent.iter().enumerate() {
        if let Some(&b) = ast.parent.get(i + 1) {
            match c.order(a, b)? {
                2 => ast.doubles.push(i + 1),
                3 => ast.triples.push(i + 1),
                1 | 4 => (),
                _ => return Err("Unsupported parent bond".into()),
            }
        } else if is_ring
            && c.order(a, *at(&ast.parent, 0)?)? != 1
            && !matches!(ast.kind, ParentKind::Retained(_))
        {
            return Err("Unsaturated nonaromatic rings are outside this profile".into());
        }
        let edges = c.edges(a)?.to_vec();
        for (b, order, _) in edges {
            if covered.contains(&b) {
                continue;
            }
            if order != 1 {
                return Err("Multiple bonds outside the numbered parent are unsupported".into());
            }
            let locant = i + 1;
            let (text, complex) = match c.number(b)? {
                6 => {
                    let (text, atoms) = alkyl(c, b, &covered, 0)?;
                    covered.extend(atoms);
                    let complex = text.contains('-');
                    (text, complex)
                }
                9 | 17 | 35 | 53 => {
                    if c.edges(b)?.len() != 1 || c.h(b)? != 0 {
                        return Err("Unsupported halogen valence".into());
                    }
                    covered.insert(b);
                    (
                        match c.number(b)? {
                            9 => "fluoro",
                            17 => "chloro",
                            35 => "bromo",
                            _ => "iodo",
                        }
                        .into(),
                        false,
                    )
                }
                8 => {
                    if c.h(b)? != 0 || c.edges(b)?.len() != 2 {
                        return Err("Unsupported ether oxygen".into());
                    }
                    let carbon = c
                        .edges(b)?
                        .iter()
                        .find(|e| e.0 != a)
                        .ok_or("Missing ether carbon")?
                        .0;
                    covered.insert(b);
                    let (text, atoms) = alkyl(c, carbon, &covered, 0)?;
                    covered.extend(atoms);
                    alkoxy(text)
                }
                _ => return Err(
                    "An unaccounted heteroatom or functional group is outside this naming profile"
                        .into(),
                ),
            };
            ast.prefixes.push(Prefix {
                locant,
                text,
                complex,
            });
        }
    }
    if covered.len() != c.state.graph.atoms.len() {
        return Err(
            "The naming AST did not account for every atom; no partial name was generated".into(),
        );
    }
    // Functional-group pattern checks and branch traversal account for all
    // nonparent bonds; bonds between two consumed but unrelated roles reject.
    for bond in &c.state.graph.bonds {
        if !covered.contains(&bond.a) || !covered.contains(&bond.b) {
            return Err("Unaccounted naming bond".into());
        }
    }
    ast.stereo = stereo(c, &locant_map)?;
    Ok(ast)
}

/// P-63.2.2: the retained methoxy/ethoxy/propoxy/butoxy prefixes are fully
/// substitutable. Other alkyl parents concatenate `oxy`, including -yloxy.
fn alkoxy(alkyl: String) -> (String, bool) {
    for (ending, contracted) in [
        ("methyl", "methoxy"),
        ("ethyl", "ethoxy"),
        ("propyl", "propoxy"),
        ("butyl", "butoxy"),
    ] {
        if let Some(prefix) = alkyl.strip_suffix(ending) {
            return (format!("{prefix}{contracted}"), !prefix.is_empty());
        }
    }
    (format!("{alkyl}oxy"), true)
}

/// Rooted substitutive alkyl AST. It enumerates every endpoint pair through the attachment and
/// recursively represents branches, never substitutes carbon count for shape.
fn alkyl(
    c: &mut Context,
    start: usize,
    excluded: &BTreeSet<usize>,
    depth: usize,
) -> Result<(String, BTreeSet<usize>), String> {
    if depth > 3 {
        return Err("Alkyl substituent nesting exceeds the supported three levels".into());
    }
    let mut atoms = BTreeSet::new();
    let mut todo = vec![start];
    while let Some(a) = todo.pop() {
        c.spend(1)?;
        if excluded.contains(&a) || !atoms.insert(a) {
            continue;
        }
        if c.number(a)? != 6 || at(&c.state.graph.atoms, a)?.aromatic {
            return Err("Only saturated acyclic carbon branches are supported".into());
        }
        for &(b, order, _) in c.edges(a)? {
            if excluded.contains(&b) {
                continue;
            }
            if order != 1 {
                return Err(
                    "Unsaturated or heteroatom-substituted alkyl branches are unsupported".into(),
                );
            }
            todo.push(b);
        }
    }
    if atoms.is_empty() {
        return Err("Empty alkyl branch".into());
    }
    // A branch must attach at exactly one excluded atom. This also rejects a
    // hidden ring or a second linkage into another consumed role.
    let boundary = atoms
        .iter()
        .map(|&a| {
            c.edges(a)
                .map(|e| e.iter().filter(|e| excluded.contains(&e.0)).count())
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .sum::<usize>();
    if boundary != 1 {
        return Err("An alkyl branch has multiple parent attachments".into());
    }
    let endpoints = atoms
        .iter()
        .copied()
        .filter(|&a| {
            c.edges(a)
                .is_ok_and(|e| e.iter().filter(|e| atoms.contains(&e.0)).count() <= 1)
        })
        .collect::<Vec<_>>();
    let mut leaves = vec![];
    for &a in &endpoints {
        for &b in &endpoints {
            c.spend(atoms.len())?;
            if a == b && atoms.len() > 1 {
                continue;
            }
            let mut previous = BTreeMap::from([(a, a)]);
            let mut todo = vec![a];
            while let Some(x) = todo.pop() {
                if x == b {
                    break;
                }
                for &(y, _, _) in c.edges(x)? {
                    if atoms.contains(&y) && !previous.contains_key(&y) {
                        previous.insert(y, x);
                        todo.push(y);
                    }
                }
            }
            let mut path = vec![b];
            let mut x = b;
            while x != a {
                x = *previous.get(&x).ok_or("Disconnected alkyl branch")?;
                path.push(x);
            }
            path.reverse();
            if path.contains(&start) {
                leaves.push(path);
            }
        }
    }
    let mut candidates = vec![];
    for path in leaves {
        let mut used = excluded.clone();
        used.extend(path.iter().copied());
        let mut prefixes = vec![];
        for (i, &a) in path.iter().enumerate() {
            for &(b, _, _) in &c.edges(a)?.to_vec() {
                if used.contains(&b) {
                    continue;
                }
                let (text, branch) = alkyl(c, b, &used, depth + 1)?;
                used.extend(branch);
                let complex = text.contains('-');
                prefixes.push(Prefix {
                    locant: i + 1,
                    text,
                    complex,
                });
            }
        }
        let attachment = path
            .iter()
            .position(|&a| a == start)
            .ok_or("Missing alkyl attachment")?
            + 1;
        let root = if attachment == 1 {
            format!("{}yl", stem(path.len())?)
        } else {
            format!("{}an-{attachment}-yl", stem(path.len())?)
        };
        let mut positions = prefixes.iter().map(|p| p.locant).collect::<Vec<_>>();
        positions.sort();
        candidates.push((
            std::cmp::Reverse(path.len()),
            attachment,
            std::cmp::Reverse(prefixes.len()),
            positions,
            format!("{}{root}", prefixes_text(&prefixes)?),
        ));
    }
    candidates.sort();
    let name = candidates
        .into_iter()
        .next()
        .map(|a| a.4)
        .ok_or("No complete alkyl parent")?;
    Ok((name, atoms))
}

fn prefixes_text(prefixes: &[Prefix]) -> Result<String, String> {
    let mut grouped: BTreeMap<String, (Vec<usize>, bool)> = BTreeMap::new();
    for p in prefixes {
        let entry = grouped.entry(p.text.clone()).or_default();
        entry.0.push(p.locant);
        entry.1 |= p.complex;
    }
    let mut pieces = vec![];
    let mut groups = grouped.into_iter().collect::<Vec<_>>();
    groups.sort_by_key(|(text, _)| (alphabetic_key(text), text.clone()));
    for (text, (mut places, complex)) in groups {
        places.sort();
        let body = if complex {
            match places.len() {
                1 => format!("({text})"),
                2 => format!("bis({text})"),
                3 => format!("tris({text})"),
                _ => return Err("Repeated complex substituents above tris are unsupported".into()),
            }
        } else {
            format!("{}{text}", multiplier(places.len())?)
        };
        pieces.push(format!("{}-{body}", locants(&places)));
    }
    Ok(pieces.join("-"))
}

fn hydrocarbon(a: &Ast, terminal_e: bool) -> Result<String, String> {
    let prefix = if matches!(a.kind, ParentKind::Cycle) {
        "cyclo"
    } else {
        ""
    };
    let mut text = format!("{prefix}{}", stem(a.parent.len())?);
    if a.doubles.is_empty() && a.triples.is_empty() {
        text.push_str(if terminal_e { "ane" } else { "an" });
        return Ok(text);
    }
    let repeated = a.doubles.len() > 1 || a.triples.len() > 1;
    if repeated {
        text.push('a');
    }
    if !a.doubles.is_empty() {
        text.push_str(&format!(
            "-{}-{}en",
            locants(&a.doubles),
            multiplier(a.doubles.len())?
        ));
    }
    if !a.triples.is_empty() {
        text.push_str(&format!(
            "-{}-{}yn",
            locants(&a.triples),
            multiplier(a.triples.len())?
        ));
    }
    if terminal_e {
        text.push('e');
    }
    Ok(text)
}

pub(super) fn name(a: &Ast) -> Result<String, String> {
    let senior = a.senior;
    let parent = if let ParentKind::Retained(text) = &a.kind {
        if senior.is_some() && a.suffix_locants.as_slice() != [1] {
            return Err("Retained suffix parent must start at locant 1".into());
        }
        text.clone()
    } else {
        let n = a.suffix_locants.len();
        match senior {
            None => hydrocarbon(a, true)?,
            Some(
                GroupKind::Acid
                | GroupKind::Ester
                | GroupKind::Amide
                | GroupKind::Nitrile
                | GroupKind::Aldehyde,
            ) => {
                if n != 1
                    || a.suffix_locants.as_slice() != [1]
                    || !matches!(a.kind, ParentKind::Chain)
                {
                    return Err(
                        "Terminal suffix classes currently require one group at chain locant 1"
                            .into(),
                    );
                }
                let base = hydrocarbon(a, false)?;
                let suffix = match senior {
                    Some(GroupKind::Acid) => "oic acid",
                    Some(GroupKind::Ester) => "oate",
                    Some(GroupKind::Amide) => "amide",
                    Some(GroupKind::Nitrile) => "enitrile",
                    _ => "al",
                };
                format!("{base}{suffix}")
            }
            Some(GroupKind::Ketone | GroupKind::Alcohol | GroupKind::Amine) => {
                let suffix = match senior {
                    Some(GroupKind::Ketone) => "one",
                    Some(GroupKind::Alcohol) => "ol",
                    _ => "amine",
                };
                let base = hydrocarbon(a, n > 1)?;
                let places = if a.parent.len() == 1 {
                    String::new()
                } else {
                    format!("-{}-", locants(&a.suffix_locants))
                };
                format!("{base}{places}{}{suffix}", multiplier(n)?)
            }
        }
    };
    let prefix = prefixes_text(&a.prefixes)?;
    let separator =
        if !prefix.is_empty() && parent.chars().next().is_some_and(|c| c.is_ascii_digit()) {
            "-"
        } else {
            ""
        };
    let stereo = if a.stereo.is_empty() {
        String::new()
    } else {
        format!(
            "({})-",
            a.stereo
                .iter()
                .map(|(l, d)| format!("{l}{d}"))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let name = format!("{stereo}{prefix}{separator}{parent}");
    Ok(if let Some(alcohol) = &a.ester_alcohol {
        format!("{alcohol} {name}")
    } else {
        name
    })
}
