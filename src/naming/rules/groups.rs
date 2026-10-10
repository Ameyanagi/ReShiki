use super::*;

/// Graph patterns are exclusive at each carbonyl; seniority is explicit in
/// GroupKind, not inferred from heteroatom counts or a successful decoder.
pub(super) fn recognize(c: &Context) -> Result<Vec<Group>, String> {
    let mut groups = vec![];
    let mut consumed = BTreeSet::new();
    for (carbon, atom) in c.state.graph.atoms.iter().enumerate() {
        if atom.atomic_number != 6 || atom.aromatic {
            continue;
        }
        let edges = c.edges(carbon)?;
        let oxygen = edges
            .iter()
            .find(|&&(a, o, _)| o == 2 && c.number(a).ok() == Some(8));
        let nitrile = edges
            .iter()
            .find(|&&(a, o, _)| o == 3 && c.number(a).ok() == Some(7));
        if let Some(&(nitrogen, _, _)) = nitrile {
            if c.edges(nitrogen)?.len() != 1 || c.h(nitrogen)? != 0 {
                return Err("Substituted or charged nitrile nitrogen is unsupported".into());
            }
            consumed.insert(nitrogen);
            groups.push(Group {
                kind: GroupKind::Nitrile,
                anchor: carbon,
                atoms: vec![nitrogen],
                alcohol: None,
            });
        } else if let Some(&(oxygen, _, _)) = oxygen {
            if c.edges(oxygen)?.len() != 1 || c.h(oxygen)? != 0 {
                return Err("Unsupported carbonyl oxygen".into());
            }
            let singles = edges.iter().filter(|e| e.1 == 1).collect::<Vec<_>>();
            let single_o = singles.iter().find(|e| c.number(e.0).ok() == Some(8));
            let single_n = singles.iter().find(|e| c.number(e.0).ok() == Some(7));
            let (kind, mut atoms, alcohol) = if let Some(e) = single_o {
                let o = e.0;
                if c.edges(o)?.len() == 1 && c.h(o)? == 1 {
                    (GroupKind::Acid, vec![o], None)
                } else if c.edges(o)?.len() == 2 && c.h(o)? == 0 {
                    let other = c
                        .edges(o)?
                        .iter()
                        .find(|e| e.0 != carbon)
                        .ok_or("Missing ester alcohol atom")?;
                    if other.1 != 1 || c.number(other.0)? != 6 {
                        return Err("Only simple carbon-linked esters are supported".into());
                    }
                    (GroupKind::Ester, vec![o], Some(other.0))
                } else {
                    return Err("Unsupported acid/ester oxygen substitution".into());
                }
            } else if let Some(e) = single_n {
                if c.edges(e.0)?.len() != 1 || c.h(e.0)? != 2 {
                    return Err("This profile supports primary amides only; N-substituted amides are unsupported".into());
                }
                (GroupKind::Amide, vec![e.0], None)
            } else if c.h(carbon)? == 1 || (c.h(carbon)? == 2 && edges.len() == 1) {
                (GroupKind::Aldehyde, vec![], None)
            } else if c.h(carbon)? == 0
                && singles.len() == 2
                && singles.iter().all(|e| c.number(e.0).ok() == Some(6))
            {
                (GroupKind::Ketone, vec![], None)
            } else {
                return Err("Unsupported carbonyl functional group".into());
            };
            atoms.push(oxygen);
            consumed.extend(atoms.iter().copied());
            groups.push(Group {
                kind,
                anchor: carbon,
                atoms,
                alcohol,
            });
        }
    }
    for (atom, data) in c.state.graph.atoms.iter().enumerate() {
        if consumed.contains(&atom) || data.aromatic {
            continue;
        }
        match data.atomic_number {
            8 if c.edges(atom)?.len()==1 && c.h(atom)?==1 => {
                let &(anchor,order,_)=at(c.edges(atom)?,0)?;
                if order!=1 || c.number(anchor)?!=6 { return Err("Only carbon-linked alcohols are supported".into()); }
                groups.push(Group { kind:GroupKind::Alcohol, anchor, atoms:vec![atom], alcohol:None });
            }
            7 if c.edges(atom)?.len()==1 && c.h(atom)?==2 => {
                let &(anchor,order,_)=at(c.edges(atom)?,0)?;
                if order!=1 || c.number(anchor)?!=6 { return Err("Only primary carbon-linked amines are supported".into()); }
                groups.push(Group { kind:GroupKind::Amine, anchor, atoms:vec![atom], alcohol:None });
            }
            8 if c.edges(atom)?.len()==2 && c.h(atom)?==0 && c.edges(atom)?.iter().all(|e| e.1==1 && c.number(e.0).ok()==Some(6)) => (), // alkoxy prefix, resolved after parent selection
            6 | 9 | 17 | 35 | 53 => (),
            _ => return Err("Unsupported nitrogen, oxygen or sulfur functional group; no simplified name was generated".into()),
        }
    }
    Ok(groups)
}
