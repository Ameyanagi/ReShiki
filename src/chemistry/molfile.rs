//! MOL writing adapted from RDKit MolFileWriter.cpp (2026.03.6).
//! Copyright (C) 2003-2023 Greg Landrum and other RDKit contributors.
//! BSD-3-Clause; see licenses/rdkit/LICENSE and NOTICE.
use super::{
    ELEMENTS, ISOTOPES, RDKIT_VERSION,
    document::{self, Molecule},
    graph::{Atom, Graph, Valence},
    stereo::{perception::RingKind, wedging},
};
use std::{collections::HashSet, fmt::Write};
mod read;
pub(crate) use read::read_reaction;
pub use read::{FileAnnotations, Imported, ReadError, read};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid MOL output: {0}")]
    Invalid(String),
    #[error(transparent)]
    Preparation(#[from] document::Error),
    #[error("MOL stereochemistry: {0}")]
    Stereo(String),
    #[error("MOL cannot preserve hydrogen, partial or quadruple bonds; use native or CDXML")]
    UnsupportedBond,
    #[error("Could not format MOL output: {0}")]
    Formatting(#[from] std::fmt::Error),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Options {
    pub force_v3000: bool,
}

fn invalid(message: impl Into<String>) -> Error {
    Error::Invalid(message.into())
}
fn at<T>(items: &[T], index: usize) -> Result<&T, Error> {
    items
        .get(index)
        .ok_or_else(|| invalid("Missing MOL graph item"))
}

/// Write a prepared 2D molecule using the default publication interchange
/// conventions. V3000 is automatic for dative bonds, large graphs/coordinates.
/// Queries, enhanced stereo groups and non-tetrahedral tags are not drawing data.
pub fn write(molecule: &Molecule, options: Options) -> Result<String, Error> {
    write_part(molecule, options, false, &[])
}

/// Write semantic ALL/ANY endpoints as V3000 properties. Distributed charges
/// and unbonded attachment points require CDXML/native storage instead.
pub fn write_document(doc: &crate::document::Document) -> Result<String, Error> {
    let mut proxy = crate::attachments::interchange_graph(doc).map_err(invalid)?;
    let mut endpoints = Vec::new();
    for a in doc.atoms.iter().filter(|a| a.attachment.is_some()) {
        let kind = a
            .attachment
            .ok_or_else(|| invalid("Missing attachment kind"))?;
        if a.charge != 0 || a.radical_electrons != 0 {
            return Err(invalid(
                "Distributed attachment charges/radicals require CDXML or native ReShiki",
            ));
        }
        let bonds: Vec<_> = proxy
            .bonds
            .iter_mut()
            .filter(|b| b.a == a.id || b.b == a.id)
            .collect();
        let [bond] = bonds.as_slice() else {
            return Err(invalid(
                "V3000 requires exactly one bond per attachment point; use CDXML",
            ));
        };
        if !matches!(bond.order, 1 | 5)
            || bond.stereo.is_some()
            || !matches!(bond.display.as_str(), "plain" | "dashed" | "dotted")
        {
            return Err(invalid("Unsupported V3000 attachment bond; use CDXML"));
        }
        for bond in bonds {
            if kind == crate::attachments::Kind::MultiCenter {
                if bond.b == a.id {
                    std::mem::swap(&mut bond.a, &mut bond.b);
                }
                bond.order = 5; // RDKit haptic convention: dummy donor → metal.
            }
        }
        endpoints.push(crate::attachments::Attachment {
            id: a.id,
            kind,
            members: a.centroid.clone(),
        });
    }
    let molecule = document::prepare(&proxy)?;
    write_part(
        &molecule,
        Options {
            force_v3000: !endpoints.is_empty(),
        },
        false,
        &endpoints,
    )
}

/// Reaction CTABs retain aromatic bond types instead of assigning Kekulé bonds.
pub(crate) fn reaction_ctab(molecule: &Molecule) -> Result<String, Error> {
    write_part(molecule, Options { force_v3000: true }, true, &[])
}

fn write_part(
    molecule: &Molecule,
    options: Options,
    reaction: bool,
    attachments: &[crate::attachments::Attachment],
) -> Result<String, Error> {
    let source = &molecule.state;
    source.graph.validate().map_err(invalid)?;
    source.metadata.validate(&source.graph).map_err(invalid)?;
    let (n, e) = (source.graph.atoms.len(), source.graph.bonds.len());
    if molecule.rdkit_version != RDKIT_VERSION
        || molecule.ids.len() != n
        || molecule.ids.iter().collect::<HashSet<_>>().len() != n
        || molecule.positions.len() != n
        || molecule.positions.iter().any(|p| {
            [p.x, p.y, p.z]
                .iter()
                .any(|v| !v.is_finite() || v.abs() > 1e100)
        })
        || source.properties.atoms.len() != n
        || source.valences.len() != n
        || source.directions.len() != e
        || source.properties.bond_codes.len() != e
        || source.hybridizations.len() != n
        || source.conjugated.len() != e
        || source.rings.kind != RingKind::Symmetric
        || !source.metadata.groups.is_empty()
        || source.metadata.atoms.iter().any(|a| a.chiral_tag > 2)
        || source.metadata.bonds.iter().any(|b| b.stereo > 5)
    {
        return Err(invalid("Molecule dimensions or annotations changed"));
    }
    if source
        .graph
        .bonds
        .iter()
        .any(|b| matches!(b.order, 0 | 6 | 7))
    {
        return Err(Error::UnsupportedBond);
    }
    let kekule = (!reaction)
        .then(|| document::kekule(molecule))
        .transpose()?;
    let (graph, valences, directions) = if let Some(kekule) = &kekule {
        (
            &kekule.assignment.graph,
            &kekule.cache,
            &kekule.assignment.directions,
        )
    } else {
        (&source.graph, &source.valences, &source.directions)
    };
    let mut bonds = wedging::file_bonds(
        &wedging::WedgeState {
            graph: graph.clone(),
            metadata: source.metadata.clone(),
            directions: directions.clone(),
            rings: source.rings.clone(),
        },
        &wedging::WedgeProperties {
            valences: valences.clone(),
            attachment_points: vec![false; n],
        },
        &wedging::Conformer {
            positions: molecule.positions.clone(),
            is_3d: false,
        },
        &source
            .properties
            .atoms
            .iter()
            .map(|a| a.cip_rank)
            .collect::<Vec<_>>(),
    )
    .map_err(Error::Stereo)?;
    for (bond, original) in bonds.iter_mut().zip(&source.graph.bonds) {
        if !reaction && original.aromatic && bond.code == 3 {
            bond.code = 0;
        }
    }
    let mut degrees = vec![0usize; n];
    for bond in &graph.bonds {
        for atom in [bond.a, bond.b] {
            *degrees
                .get_mut(atom)
                .ok_or_else(|| invalid("Missing MOL degree"))? += 1;
        }
    }
    let v3000 = options.force_v3000
        || n > 999
        || e > 999
        || graph.bonds.iter().any(|b| b.order == 5)
        || molecule.positions.iter().any(|p| {
            [p.x, p.y, p.z]
                .iter()
                .any(|&v| v >= 100000. || v <= -10000.)
        });
    let mut output = if reaction {
        String::new()
    } else {
        String::from("\n     RDKit          2D\n\n")
    };
    if v3000 {
        if !reaction {
            output.push_str("  0  0  0  0  0  0  0  0  0  0999 V3000\n");
        }
        output.push_str("M  V30 BEGIN CTAB\n");
        writeln!(output, "M  V30 COUNTS {n} {e} 0 0 0\nM  V30 BEGIN ATOM")?;
    } else {
        writeln!(output, "{n:3}{e:3}  0  0  0  0  0  0  0  0999 V2000")?;
    }
    for (i, atom) in graph.atoms.iter().enumerate() {
        let p = at(&molecule.positions, i)?;
        let symbol = if atom.atomic_number == 0 {
            "R"
        } else {
            at(ELEMENTS, usize::from(atom.atomic_number))?.symbol
        };
        let map = at(&source.metadata.atoms, i)?.map_number;
        let valence = nondefault_valence(atom, at(valences, i)?, *at(&degrees, i)?)?;
        if v3000 {
            write!(
                output,
                "M  V30 {} {symbol} {:.6} {:.6} {:.6} {map}",
                i + 1,
                p.x,
                p.y,
                p.z
            )?;
            if atom.charge != 0 {
                write!(output, " CHG={}", atom.charge)?;
            }
            if atom.isotope != 0 {
                write!(output, " MASS={}", isotope_mass(atom))?;
            }
            if let Some(radical) = radical_code(atom, at(valences, i)?, *at(&degrees, i)?) {
                write!(output, " RAD={radical}")?;
            }
            if valence != 0 {
                write!(
                    output,
                    " VAL={}",
                    if valence == 15 { -1 } else { valence as i32 }
                )?;
            }
            output.push('\n');
        } else {
            writeln!(
                output,
                "{:10.4}{:10.4}{:10.4} {symbol:<3}{:2}{:3}{:3}{:3}{:3}{valence:3}  0{:3}{:3}{map:3}{:3}{:3}",
                p.x, p.y, p.z, 0, 0, 0, 0, 0, 0, 0, 0, 0
            )?;
        }
    }
    if v3000 {
        output.push_str("M  V30 END ATOM\n");
        if e != 0 {
            output.push_str("M  V30 BEGIN BOND\n");
        }
    }
    for (i, (bond, stereo)) in graph.bonds.iter().zip(bonds).enumerate() {
        let order = if bond.order == 5 { 9 } else { bond.order };
        let (a, b) = (stereo.a + 1, stereo.b + 1);
        if v3000 {
            if let Some(attachment) = attachments.iter().find(|n| {
                molecule.ids.get(bond.a) == Some(&n.id) || molecule.ids.get(bond.b) == Some(&n.id)
            }) {
                let mut line = format!(
                    "{} {order} {a} {b} ENDPTS=({}",
                    i + 1,
                    attachment.members.len()
                );
                for id in &attachment.members {
                    let index = molecule
                        .ids
                        .iter()
                        .position(|n| n == id)
                        .ok_or_else(|| invalid("Missing MOL attachment target"))?;
                    write!(line, " {}", index + 1)?;
                }
                write!(
                    line,
                    ") ATTACH={}",
                    if attachment.kind == crate::attachments::Kind::MultiCenter {
                        "ALL"
                    } else {
                        "ANY"
                    }
                )?;
                // V3000 continuation records concatenate without adding spaces.
                for (index, chunk) in line.as_bytes().chunks(70).enumerate() {
                    let text = std::str::from_utf8(chunk)
                        .map_err(|_| invalid("Invalid attachment record"))?;
                    writeln!(
                        output,
                        "M  V30 {text}{}",
                        if (index + 1) * 70 < line.len() {
                            "-"
                        } else {
                            ""
                        }
                    )?;
                }
                continue;
            }
            write!(output, "M  V30 {} {order} {a} {b}", i + 1)?;
            if stereo.code != 0 {
                let cfg = match stereo.code {
                    1 => 1,
                    3 | 4 => 2,
                    6 => 3,
                    _ => return Err(invalid("Unknown MOL bond stereo")),
                };
                write!(output, " CFG={cfg}")?;
            }
            output.push('\n');
        } else {
            writeln!(output, "{a:3}{b:3}{order:3} {:2}", stereo.code)?;
        }
    }
    if v3000 {
        if e != 0 {
            output.push_str("M  V30 END BOND\n");
        }
        output.push_str("M  V30 END CTAB\n");
    } else {
        properties(&mut output, graph, valences, &degrees)?;
    }
    if !reaction {
        output.push_str("M  END\n");
    }
    Ok(output)
}

fn nondefault_valence(atom: &Atom, cache: &Valence, degree: usize) -> Result<u32, Error> {
    let nondefault = if atom.radical_electrons != 0 {
        true
    } else if matches!(atom.atomic_number, 0 | 1 | 5..=9 | 15..=17 | 35 | 53) {
        if atom.no_implicit {
            let effective = usize::try_from(i16::from(atom.atomic_number) - i16::from(atom.charge))
                .map_err(|_| invalid("Charge gives an invalid effective element"))?;
            let default = at(ELEMENTS, effective)?
                .valences
                .first()
                .ok_or_else(|| invalid("Missing default valence"))?;
            i64::from(cache.explicit_valence) != i64::from(*default)
        } else {
            false
        }
    } else {
        true
    };
    Ok(if !nondefault {
        0
    } else if degree == 0 && atom.explicit_hydrogens == 0 && cache.implicit_hydrogens == 0 {
        15
    } else {
        cache
            .explicit_valence
            .checked_add(cache.implicit_hydrogens)
            .ok_or_else(|| invalid("MOL total valence overflow"))?
            % 15
    })
}

fn radical_code(atom: &Atom, cache: &Valence, degree: usize) -> Option<i32> {
    (atom.radical_electrons != 0
        && (degree != 0 || atom.explicit_hydrogens != 0 || cache.implicit_hydrogens != 0))
        .then_some(if atom.radical_electrons % 2 == 1 {
            2
        } else {
            3
        })
}

fn isotope_mass(atom: &Atom) -> u16 {
    ISOTOPES
        .binary_search_by_key(&(atom.atomic_number, atom.isotope), |&(n, i, _)| (n, i))
        .ok()
        .and_then(|i| ISOTOPES.get(i))
        .map(|&(_, _, mass)| mass.round() as u16)
        .filter(|&mass| mass != 0)
        .unwrap_or(atom.isotope)
}

fn flush(output: &mut String, name: &str, values: &mut Vec<(usize, i32)>) -> Result<(), Error> {
    if !values.is_empty() {
        write!(output, "M  {name}{:3}", values.len())?;
        for (index, value) in values.drain(..) {
            write!(output, " {index:3} {value:3}")?;
        }
        output.push('\n');
    }
    Ok(())
}

fn properties(
    output: &mut String,
    graph: &Graph,
    cache: &[Valence],
    degrees: &[usize],
) -> Result<(), Error> {
    let (mut charges, mut radicals, mut isotopes) = (Vec::new(), Vec::new(), Vec::new());
    for (i, atom) in graph.atoms.iter().enumerate() {
        if atom.charge != 0 {
            charges.push((i + 1, i32::from(atom.charge)));
            if charges.len() == 8 {
                flush(output, "CHG", &mut charges)?;
            }
        }
        if let Some(code) = radical_code(atom, at(cache, i)?, *at(degrees, i)?) {
            radicals.push((i + 1, code));
            if radicals.len() == 8 {
                flush(output, "RAD", &mut radicals)?;
            }
        }
        if atom.isotope != 0 {
            isotopes.push((i + 1, i32::from(atom.isotope)));
            if isotopes.len() == 8 {
                flush(output, "ISO", &mut isotopes)?;
            }
        }
    }
    flush(output, "CHG", &mut charges)?;
    flush(output, "RAD", &mut radicals)?;
    flush(output, "ISO", &mut isotopes)
}
