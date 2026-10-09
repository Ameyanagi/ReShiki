use super::*;

pub(super) type Candidate = (Vec<usize>, ParentKind);

pub(super) fn candidates(c: &mut Context) -> Result<Vec<Candidate>, String> {
    // A connected graph's cyclomatic number detects fused/spiro/bridged and
    // heteroatom-linked extra rings before a carbon-parent search can hide them.
    let rings = c.state.graph.bonds.len() + 1 - c.state.graph.atoms.len();
    if rings > 1 {
        return Err(
            "Fused, spiro, bridged and multiple-ring reverse naming are outside this profile"
                .into(),
        );
    }
    if rings == 1 {
        return cycle(c);
    }
    if c.state.graph.atoms.iter().any(|a| a.aromatic) {
        return Err("Aromatic atoms require a supported exact ring parent".into());
    }
    if !c.state.graph.atoms.iter().any(|a| a.atomic_number == 6) {
        return Err("An organic carbon parent is required".into());
    }
    let senior = c.senior();
    let components = if let Some(group) = c.groups.iter().find(|g| Some(g.kind) == senior) {
        vec![c.carbon_component(group.anchor)?]
    } else {
        let mut components = vec![];
        let mut covered = BTreeSet::new();
        for atom in 0..c.state.graph.atoms.len() {
            if c.number(atom)? == 6 && !covered.contains(&atom) {
                let part = c.carbon_component(atom)?;
                covered.extend(part.iter().copied());
                components.push(part);
            }
        }
        components
    };
    let mut paths = vec![];
    for component in components {
        if c.groups
            .iter()
            .filter(|g| Some(g.kind) == senior)
            .any(|g| !component.contains(&g.anchor))
        {
            return Err(
                "Senior functional groups span unsupported heteroatom-linked carbon parents".into(),
            );
        }
        let endpoints = component
            .iter()
            .copied()
            .filter(|&a| {
                c.edges(a)
                    .is_ok_and(|e| e.iter().filter(|e| component.contains(&e.0)).count() <= 1)
            })
            .collect::<Vec<_>>();
        if endpoints.is_empty() {
            return Err("Unsupported cyclic carbon parent".into());
        }
        for &a in &endpoints {
            for &b in &endpoints {
                c.spend(component.len())?;
                if a == b && component.len() > 1 {
                    continue;
                }
                let mut previous = BTreeMap::new();
                let mut todo = vec![a];
                previous.insert(a, a);
                while let Some(x) = todo.pop() {
                    if x == b {
                        break;
                    }
                    for &(y, _, _) in c.edges(x)? {
                        if component.contains(&y) && !previous.contains_key(&y) {
                            previous.insert(y, x);
                            todo.push(y);
                        }
                    }
                }
                let mut path = vec![b];
                let mut x = b;
                while x != a {
                    x = *previous.get(&x).ok_or("Disconnected carbon parent")?;
                    path.push(x);
                }
                path.reverse();
                if c.groups
                    .iter()
                    .filter(|g| Some(g.kind) == senior)
                    .all(|g| path.contains(&g.anchor))
                {
                    paths.push((path, ParentKind::Chain));
                }
            }
        }
    }
    if paths.is_empty() {
        return Err(
            "No chain contains every senior functional group; this parent topology is unsupported"
                .into(),
        );
    }
    Ok(paths)
}

fn cycle(c: &mut Context) -> Result<Vec<Candidate>, String> {
    // Strip leaves to obtain the one exact cycle, independent of an SSSR order.
    let mut alive = (0..c.state.graph.atoms.len()).collect::<BTreeSet<_>>();
    loop {
        let leaves = alive
            .iter()
            .copied()
            .filter(|&a| {
                c.edges(a)
                    .is_ok_and(|e| e.iter().filter(|e| alive.contains(&e.0)).count() < 2)
            })
            .collect::<Vec<_>>();
        if leaves.is_empty() {
            break;
        }
        for atom in leaves {
            alive.remove(&atom);
        }
    }
    if !(3..=12).contains(&alive.len()) {
        return Err("This profile supports simple ring parents of 3–12 atoms".into());
    }
    if c.groups.iter().any(|g| !alive.contains(&g.anchor)) {
        return Err(
            "Acyclic suffix groups attached to ring parents are outside this profile".into(),
        );
    }
    let first = *alive.first().ok_or("Missing ring parent")?;
    let mut path = vec![first];
    let mut previous = usize::MAX;
    let mut x = first;
    loop {
        let next = c
            .edges(x)?
            .iter()
            .find(|e| alive.contains(&e.0) && e.0 != previous)
            .ok_or("Invalid simple ring")?
            .0;
        if next == first {
            break;
        }
        if path.contains(&next) {
            return Err("Unsupported fused ring parent".into());
        }
        path.push(next);
        previous = x;
        x = next;
    }
    if path.len() != alive.len() {
        return Err("Unsupported cyclic parent topology".into());
    }
    let all_carbon = alive.iter().all(|&a| c.number(a).ok() == Some(6));
    let aromatic = alive
        .iter()
        .all(|&a| at(&c.state.graph.atoms, a).is_ok_and(|a| a.aromatic));
    if !aromatic
        && (!all_carbon
            || alive.iter().any(|&a| {
                c.edges(a)
                    .is_ok_and(|e| e.iter().any(|e| alive.contains(&e.0) && e.1 != 1))
            }))
    {
        return Err("Reverse cyclic naming currently supports saturated carbocycles or exact aromatic retained parents".into());
    }
    let mut result = vec![];
    for reverse in [false, true] {
        let oriented = if reverse {
            path.iter().rev().copied().collect::<Vec<_>>()
        } else {
            path.clone()
        };
        for offset in 0..oriented.len() {
            c.spend(oriented.len())?;
            let numbered = oriented
                .iter()
                .cycle()
                .skip(offset)
                .take(oriented.len())
                .copied()
                .collect::<Vec<_>>();
            let kind = if !aromatic {
                ParentKind::Cycle
            } else if all_carbon && numbered.len() == 6 {
                ParentKind::Retained(
                    match c.senior() {
                        Some(GroupKind::Alcohol) => "phenol",
                        Some(GroupKind::Amine) => "aniline",
                        None => "benzene",
                        _ => return Err("Unsupported benzene suffix class".into()),
                    }
                    .into(),
                )
            } else if let Some(name) = heterocycle(c, &numbered)? {
                ParentKind::Retained(name.into())
            } else {
                continue;
            };
            // This bounded ring profile excludes carbon branches larger than
            // the ring. This is a domain guard, not a PIN seniority claim.
            for &a in &numbered {
                for &(b, _, _) in c.edges(a)? {
                    if c.number(b)? == 6
                        && !alive.contains(&b)
                        && branch_size(c, b, &alive)? > alive.len()
                    {
                        return Err(
                            "A ring-attached carbon branch exceeds this profile's ring-size bound"
                                .into(),
                        );
                    }
                }
            }
            result.push((numbered, kind));
        }
    }
    if result.is_empty() {
        return Err(
            "Aromatic ring topology is outside the exact retained-parent vocabulary".into(),
        );
    }
    Ok(result)
}
fn branch_size(c: &Context, start: usize, excluded: &BTreeSet<usize>) -> Result<usize, String> {
    let mut seen = excluded.clone();
    let mut todo = vec![start];
    let mut count = 0;
    while let Some(a) = todo.pop() {
        if !seen.insert(a) {
            continue;
        }
        count += 1;
        todo.extend(c.edges(a)?.iter().filter_map(|e| {
            (c.number(e.0).ok() == Some(6) && !seen.contains(&e.0)).then_some(e.0)
        }));
    }
    Ok(count)
}
fn heterocycle<'a>(c: &Context, path: &[usize]) -> Result<Option<&'a str>, String> {
    let pattern = path
        .iter()
        .map(|&a| c.number(a))
        .collect::<Result<Vec<_>, _>>()?;
    let name = match pattern.as_slice() {
        [7, 6, 6, 6, 6, 6] => "pyridine",
        [7, 7, 6, 6, 6, 6] => "pyridazine",
        [7, 6, 7, 6, 6, 6] => "pyrimidine",
        [7, 6, 6, 7, 6, 6] => "pyrazine",
        [7, 6, 6, 6, 6] if c.h(*at(path, 0)?)? == 1 => "1H-pyrrole",
        [7, 7, 6, 6, 6] if c.h(*at(path, 0)?)? == 1 && c.h(*at(path, 1)?)? == 0 => "1H-pyrazole",
        [7, 6, 7, 6, 6] if c.h(*at(path, 0)?)? == 1 && c.h(*at(path, 2)?)? == 0 => "1H-imidazole",
        [8, 6, 6, 6, 6] => "furan",
        [16, 6, 6, 6, 6] => "thiophene",
        [16, 6, 7, 6, 6] => "1,3-thiazole",
        [8, 6, 7, 6, 6] => "1,3-oxazole",
        _ => return Ok(None),
    };
    // Aromatic heteroatoms must have the exact parent hydrogen state and no
    // exocyclic substitution. Atom counts alone cannot distinguish diazines.
    for (i, &a) in path.iter().enumerate() {
        if c.number(a)? != 6 {
            let expected_h = if i == 0 && name.starts_with("1H-") {
                1
            } else {
                0
            };
            if c.edges(a)?.len() != 2 || c.h(a)? != expected_h {
                return Ok(None);
            }
        }
    }
    Ok(Some(name))
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Score {
    length: std::cmp::Reverse<usize>,
    multiple_count: std::cmp::Reverse<usize>,
    double_count: std::cmp::Reverse<usize>,
    branch_count: std::cmp::Reverse<usize>,
    suffix: Vec<usize>,
    unsaturation: Vec<usize>,
    doubles: Vec<usize>,
    prefixes: Vec<usize>,
    alphabetic: Vec<(String, Vec<usize>)>,
}
pub(super) fn score(_c: &Context, a: &Ast) -> Score {
    let mut unsaturation = a
        .doubles
        .iter()
        .chain(&a.triples)
        .copied()
        .collect::<Vec<_>>();
    unsaturation.sort();
    let mut prefixes = a.prefixes.iter().map(|p| p.locant).collect::<Vec<_>>();
    prefixes.sort();
    let mut alphabetic: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for p in &a.prefixes {
        alphabetic
            .entry(alphabetic_key(&p.text))
            .or_default()
            .push(p.locant);
    }
    for values in alphabetic.values_mut() {
        values.sort();
    }
    Score {
        length: std::cmp::Reverse(a.parent.len()),
        multiple_count: std::cmp::Reverse(unsaturation.len()),
        double_count: std::cmp::Reverse(a.doubles.len()),
        branch_count: std::cmp::Reverse(a.prefixes.len()),
        suffix: a.suffix_locants.clone(),
        unsaturation,
        doubles: a.doubles.clone(),
        prefixes,
        alphabetic: alphabetic.into_iter().collect(),
    }
}
