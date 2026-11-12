//! Opt-in preparation and disclosures for approximate coordination geometry.
//! UFF parameter acceptance always goes through COSMolKit's actual typer.
use super::*;
use crate::is_metal_atomic_number;
use cosmolkit::{Atom, CoordinateBlock, Hybridization, SanitizeOperations as Ops, SanitizeParams};
use std::collections::BTreeSet;

const TYPE_FALLBACK: &str = "_reshikiGeometryUffTypeFallback";

fn metal(atom: &Atom) -> bool {
    is_metal_atomic_number(u32::from(atom.atomic_number()))
}

fn operations() -> Ops {
    // Do not infer radicals, adjust H, rewrite drawn metal bonds, or remove
    // explicitly supplied stereochemistry. The omitted PROPERTIES stage makes
    // sanitize install its source-backed nonstrict property cache.
    Ops::SYMM_RINGS
        | Ops::KEKULIZE
        | Ops::SET_AROMATICITY
        | Ops::SET_CONJUGATION
        | Ops::SET_HYBRIDIZATION
        | Ops::CLEANUP_ATROPISOMERS
}

fn rebuilt(mol: &Molecule, topology: cosmolkit::TopologyBlock, ops: Ops) -> Result<Molecule> {
    Molecule::from_parts(
        topology,
        CoordinateBlock::default(),
        mol.properties().clone(),
    )
    .map_err(|error| error.to_string())?
    .sanitize_with_params(&SanitizeParams { operations: ops })
    .map_err(|error| error.to_string())
}

pub(super) fn prepare(mol: &Molecule) -> Result<Molecule> {
    let mut topology = mol.topology().clone();
    let valence = cosmolkit_core::assign_valence(
        &topology,
        &cosmolkit_core::ValenceParams {
            strict: false,
            ..Default::default()
        },
    )
    .map_err(|error| error.to_string())?;
    for (index, atom) in topology.atoms.iter_mut().enumerate() {
        // The nonstrict cache retains the source charge, aromaticity, and
        // radical rules. A neutral atom additionally cannot acquire H beyond
        // the nonnegative default-valence remainder. Metals never acquire H.
        let implicit = if atom.no_implicit() || metal(atom) || atom.atomic_number() == 1 {
            0
        } else {
            let source = valence.implicit_hydrogens[index].max(0);
            if atom.formal_charge() == 0 {
                let remainder = (cosmolkit_core::rdkit_default_valence(atom.atomic_number())
                    .map_err(|error| error.to_string())?
                    - valence.explicit_valence[index]
                    - i32::from(atom.radical_electrons()))
                .max(0);
                source.min(remainder)
            } else {
                source
            }
        };
        let hydrogens = i32::from(atom.explicit_hydrogens()) + implicit;
        atom.set_explicit_hydrogens(hydrogens as u8);
        atom.set_no_implicit(true);
    }
    let prepared = rebuilt(mol, topology, operations())?;
    let metadata = prepared
        .atom_metadata(false)
        .map_err(|error| error.to_string())?;
    let mut topology = prepared.topology().clone();
    let mut changed = false;
    for atom in &mut topology.atoms {
        let index = atom.id().index();
        let total_valence = metadata[index].total_valence;
        let forced = match atom.atomic_number() {
            12 | 13 | 50 | 81..=84 => Some(Hybridization::Sp3),
            80 => Some(Hybridization::Sp),
            _ => None,
        };
        if let Some(forced) = forced
            && atom.hybridization() != forced
        {
            // UFF forces these metal parameter prefixes regardless of the
            // source electron-count hybridization. Keep its angular model and
            // disclosed parameter geometry consistent in the relaxed path.
            atom.set_hybridization(forced);
            atom.set_prop(TYPE_FALLBACK, true)
                .map_err(|error| error.to_string())?;
            changed = true;
        }
        if atom_has_parameters(atom, total_valence, conjugated(&prepared, index))? {
            continue;
        }
        // Some metal electron counts imply a hybridization absent from UFF's
        // finite table (notably Pt(II) -> Pt6+2). Prefer the geometry associated
        // with its drawn coordination number, then the nearest available type.
        let degree = prepared.topology().adjacency.neighbors_of(index).len();
        let max_order = prepared
            .topology()
            .adjacency
            .neighbors_of(index)
            .iter()
            .map(|neighbor| prepared.bonds()[neighbor.bond.index()].order())
            .map(|order| match order {
                BondOrder::Double => 2,
                BondOrder::Triple | BondOrder::Quadruple => 3,
                _ => 1,
            })
            .max()
            .unwrap_or(0);
        let preferred = match degree {
            _ if !metal(atom) && max_order >= 3 => Hybridization::Sp,
            _ if !metal(atom) && max_order == 2 => Hybridization::Sp2,
            0 | 1 => Hybridization::S,
            2 => Hybridization::Sp,
            3 => Hybridization::Sp2,
            4 if matches!(atom.atomic_number(), 28 | 46 | 54 | 78) => Hybridization::Sp2d,
            4 => Hybridization::Sp3,
            5 => Hybridization::Sp3d,
            _ => Hybridization::Sp3d2,
        };
        let mut selected = None;
        for hybridization in [
            preferred,
            Hybridization::Sp3d2,
            Hybridization::Sp3,
            Hybridization::Sp2d,
            Hybridization::Sp3d,
            Hybridization::Sp2,
            Hybridization::Sp,
            Hybridization::S,
        ] {
            atom.set_hybridization(hybridization);
            if atom_has_parameters(atom, total_valence, conjugated(&prepared, index))? {
                selected = Some(hybridization);
                break;
            }
        }
        if let Some(hybridization) = selected {
            atom.set_hybridization(hybridization);
            atom.set_prop(TYPE_FALLBACK, true)
                .map_err(|error| error.to_string())?;
            changed = true;
        } else {
            atom.set_hybridization(prepared.atoms()[index].hybridization());
        }
    }
    if changed {
        // Preserve the selected UFF geometry while reinstalling rings and the
        // nonstrict cache for the otherwise unchanged calculation topology.
        let ops = Ops::SYMM_RINGS | Ops::KEKULIZE | Ops::SET_AROMATICITY | Ops::SET_CONJUGATION;
        rebuilt(&prepared, topology, ops)
    } else {
        Ok(prepared)
    }
}

fn conjugated(mol: &Molecule, index: usize) -> bool {
    mol.topology()
        .adjacency
        .neighbors_of(index)
        .iter()
        .any(|neighbor| mol.bonds()[neighbor.bond.index()].is_conjugated())
}

fn atom_has_parameters(atom: &Atom, total_valence: i32, conjugated: bool) -> Result<bool> {
    // A singleton preserves exactly the inputs read by the UFF typer: element,
    // total valence, formal charge, hybridization, aromatic/conjugated state.
    // This is used only for a metal fallback or a missing-parameter diagnostic;
    // the full molecule is always independently admitted by the real query.
    let mut builder = MoleculeBuilder::new();
    builder.add_atom(
        AtomSpec::new(atom.element())
            .with_formal_charge(atom.formal_charge())
            .with_radical_electrons(atom.radical_electrons())
            .with_explicit_hydrogens(total_valence.clamp(0, 255) as u8)
            .with_no_implicit(true)
            .with_hybridization(atom.hybridization())
            .with_aromatic(
                atom.is_aromatic()
                    || (conjugated && matches!(atom.atomic_number(), 6 | 7 | 8 | 16)),
            ),
    );
    builder
        .build()
        .map_err(|error| error.to_string())?
        .sanitize_with_params(&SanitizeParams {
            operations: Ops::NONE,
        })
        .map_err(|error| error.to_string())?
        .uff_has_all_molecule_params()
        .map_err(|error| error.to_string())
}

pub(super) fn check_field(mol: &Molecule, field: ForceField) -> Result<()> {
    if field != ForceField::UFF {
        require(
            !mol.atoms().iter().any(metal),
            "MMFF parameters are unavailable for metal-containing relaxed geometry; use UFF",
        )?;
        return check_field_parameters(mol, field).map_err(|error| format!("{error}; use UFF"));
    }
    if mol
        .uff_has_all_molecule_params()
        .map_err(|error| error.to_string())?
    {
        return Ok(());
    }
    let metadata = mol
        .atom_metadata(false)
        .map_err(|error| error.to_string())?;
    let mut missing = BTreeSet::new();
    for (index, atom) in mol.atoms().iter().enumerate() {
        if !atom_has_parameters(atom, metadata[index].total_valence, conjugated(mol, index))? {
            missing.insert(symbol(atom));
        }
    }
    Err(format!(
        "UFF parameters are unavailable for {}",
        if missing.is_empty() {
            "this molecule".into()
        } else {
            missing.into_iter().collect::<Vec<_>>().join(", ")
        }
    ))
}

fn symbol(atom: &Atom) -> &'static str {
    cosmolkit_core::rdkit_element_symbol(atom.atomic_number()).unwrap_or("?")
}

// COSMolKit has no public label API. Project labels only for disclosed metals
// and fallback atoms; the actual UFF typer controls parameter acceptance.
fn type_label(atom: &Atom, total_valence: i32, conjugated: bool) -> Result<String> {
    let number = atom.atomic_number();
    let mut label = symbol(atom).to_owned();
    if label.len() == 1 {
        label.push('_');
    }
    let default =
        cosmolkit_core::rdkit_default_valence(number).map_err(|error| error.to_string())?;
    let outer = cosmolkit_core::periodic_table_outer_electrons(number)
        .map_err(|error| error.to_string())?;
    if default == -1 || (outer != 1 && outer != 7) {
        let suffix = match number {
            12 | 13 | 14 | 15 | 50 | 51 | 52 | 81 | 82 | 83 | 84 => "3",
            80 => "1",
            _ => match atom.hybridization() {
                Hybridization::Sp => "1",
                Hybridization::Sp2
                    if (atom.is_aromatic() || conjugated) && matches!(number, 6 | 7 | 8 | 16) =>
                {
                    "R"
                }
                Hybridization::Sp2 => "2",
                Hybridization::Sp3 => "3",
                Hybridization::Sp2d => "4",
                Hybridization::Sp3d => "5",
                Hybridization::Sp3d2 => "6",
                _ => "",
            },
        };
        label.push_str(suffix);
    }
    label.push_str(match number {
        29 | 47 => "+1",
        4 | 12 | 20 | 25 | 26 | 28 | 30 | 34 | 46 | 48 | 52 | 78 | 80 | 84 => "+2",
        21 | 24 | 27 | 31 | 33 | 49 | 51 | 57..=71 | 79 | 81..=83 | 89 | 96..=103 => "+3",
        2 | 18 | 22 | 36 | 54 | 90..=95 => "+4",
        23 | 41 | 43 | 73 => "+5",
        42 => "+6",
        15 if total_valence == 3 => "+3",
        15 => "+5",
        16 if atom.hybridization() == Hybridization::Sp2 => "",
        16 if total_valence == 4 => "+4",
        16 if total_valence == 2 => "+2",
        16 => "+6",
        75 if atom.hybridization() == Hybridization::Sp3d2 => "+5",
        75 if atom.hybridization() == Hybridization::Sp3 => "+7",
        _ => "",
    });
    Ok(label)
}

fn geometry(atom: &Atom) -> (&'static str, Option<usize>) {
    // Alkali atom labels omit hybridization and do not prescribe a unique
    // chemical coordination number, even when the table has angular terms.
    if matches!(atom.atomic_number(), 3 | 11 | 19 | 37 | 55 | 87) {
        return ("alkali ion coordination", None);
    }
    // These table rows do not use the tetrahedral angle suggested by their
    // forced `3` type prefix. Report the parameter angle without inventing a
    // unique coordination number or three-dimensional arrangement.
    if atom.atomic_number() == 81 {
        return ("120 degree angles", None);
    }
    if matches!(atom.atomic_number(), 83 | 84) {
        return ("90 degree angles", None);
    }
    match atom.hybridization() {
        Hybridization::Sp => ("linear", Some(2)),
        Hybridization::Sp2 => ("trigonal planar", Some(3)),
        Hybridization::Sp3 => ("tetrahedral", Some(4)),
        Hybridization::Sp2d => ("square planar", Some(4)),
        Hybridization::Sp3d => ("trigonal bipyramidal", Some(5)),
        Hybridization::Sp3d2 => ("octahedral", Some(6)),
        _ => ("unspecified coordination geometry", None),
    }
}

pub(super) fn approximations(mol: &Molecule) -> Result<Vec<String>> {
    // Sort by element, category, then text, independent of atom ordering.
    let mut notes = BTreeSet::new();
    let metadata = mol
        .atom_metadata(false)
        .map_err(|error| error.to_string())?;
    for (index, atom) in mol.atoms().iter().enumerate() {
        let element = symbol(atom);
        let mut note = |category, text| notes.insert((element, category, text));
        let neighbors = mol.topology().adjacency.neighbors_of(index).len();
        if atom.hybridization() == Hybridization::Sp2d {
            note(
                3,
                format!("{element}: UFF square planar angles can bend trans ligands"),
            );
        }
        if neighbors > 0
            && matches!(
                atom.atomic_number(),
                3 | 4 | 11 | 12 | 19 | 20 | 37 | 38 | 55 | 56
            )
        {
            note(
                4,
                format!("{element}: drawn contacts are modeled as UFF bonds and come out short"),
            );
        }
        if !metal(atom) && atom.prop(TYPE_FALLBACK) != Some(&PropertyValue::Bool(true)) {
            continue;
        }
        let label = type_label(atom, metadata[index].total_valence, conjugated(mol, index))?;
        let (geometry, ideal) = geometry(atom);
        note(0, format!("UFF type {label} ({geometry}) for {element}"));
        if let Some(ideal) = ideal
            && ideal != neighbors
        {
            note(
                1,
                format!("{element}: {neighbors} neighbors drawn; UFF type {label} assumes {ideal}"),
            );
        }
        if let Some((_, suffix)) = label.split_once('+')
            && let Ok(charge) = suffix.parse::<i8>()
            && charge != atom.formal_charge()
        {
            let drawn = if atom.formal_charge() == 0 {
                "0".into()
            } else {
                format!("{:+}", atom.formal_charge())
            };
            note(
                2,
                format!("{element}: UFF type {label} assumes charge {charge:+}; drawn {drawn}"),
            );
        }
    }
    Ok(notes.into_iter().map(|(_, _, text)| text).collect())
}

pub(super) fn uses_drawing_seed(request: &Request) -> bool {
    // The verified Co(III) case fails ETKDG but converges from the drawing;
    // retain that seed and its relative metal-ligand placement when available.
    !request.coordinates.is_empty() && request.coordinates.iter().all(valid_coordinate)
        && (request.atoms.iter().any(|atom| is_metal_atomic_number(atom.atomic_number))
            || !has_three_dimensional_extent(&request.coordinates))
        // ETKDG owns enforcement of explicitly drawn atom/bond stereo when the
        // drawing lacks a matching spatial arrangement.
        && (has_three_dimensional_extent(&request.coordinates)
            || (!request.atoms.iter().any(|atom| atom.chiral_tag != 0)
                && !request.bonds.iter().any(|bond| bond.stereo > 1)))
}

pub(super) fn starting_coordinates(
    original: &Molecule,
    mol: &Molecule,
    request: &Request,
) -> Result<Vec<[f64; 3]>> {
    let mut points = request.coordinates.clone();
    if !has_three_dimensional_extent(&points) {
        // Break a planar saddle reproducibly without discarding the drawing's
        // relative ligand placement (cis/trans metal arrangements included).
        perturb_drawing(&mut points, request.seed);
    }
    validate_geometry(original, &points)?;
    let positioned = original
        .with_only_3d_conformer(rows(&points))
        .map_err(|error| error.to_string())?;
    let expanded = with_hydrogens(&positioned, request.atoms.len(), true)?;
    require(
        expanded.num_atoms() == mol.num_atoms()
            && hydrogen_parents(&expanded, request.atoms.len())?
                == hydrogen_parents(mol, request.atoms.len())?,
        "coordinate-aware hydrogen expansion changed the calculation atom mapping",
    )?;
    let coordinates = expanded
        .conformers_3d()
        .first()
        .ok_or("relaxed hydrogen expansion lost starting coordinates")?
        .coordinates()
        .to_vec();
    validate_geometry(mol, &coordinates)?;
    Ok(coordinates)
}

pub(super) fn relax_hydrogens(
    mol: &Molecule,
    coordinates: Vec<[f64; 3]>,
    request: &Request,
) -> Result<Vec<[f64; 3]>> {
    // AddHs torsions can clash across crowded ligands and blow the complex
    // apart in full minimization. First move only the appended hydrogens.
    let original_count = request.atoms.len();
    // Fixed atoms do not remove absolute XYZ from the optimizer's stopping
    // test. Use the same original-atom center as minimize_seed.
    let origin = std::array::from_fn(|axis| {
        coordinates[..original_count]
            .iter()
            .map(|point| point[axis])
            .sum::<f64>()
            / original_count as f64
    });
    let centered: Vec<_> = coordinates
        .iter()
        .map(|&point| sub(point, origin))
        .collect();
    let positioned = mol
        .with_only_3d_conformer(rows(&centered))
        .map_err(|error| error.to_string())?;
    let mut field = request_force_field(&positioned, request)?;
    let fixed: Vec<_> = (0..original_count).map(AtomId::new).collect();
    field
        .set_fixed_atoms_(&fixed)
        .map_err(|error| error.to_string())?;
    field
        .minimize_with_params_(&ForceFieldMinimizeParams::new(
            request.max_iterations,
            1e-4,
            1e-6,
        ))
        .map_err(|error| error.to_string())?;
    let mut result = field.positions();
    // Preserve drawn atoms exactly instead of round-tripping their origin.
    result[..original_count].copy_from_slice(&coordinates[..original_count]);
    for point in &mut result[original_count..] {
        *point = std::array::from_fn(|axis| point[axis] + origin[axis]);
    }
    Ok(result)
}

fn perturb_drawing(points: &mut [[f64; 3]], seed: i32) {
    for (index, point) in points.iter_mut().enumerate() {
        let phase = ((index as u64 + 1) * 2_654_435_761_u64 + seed as u64) % 65_521;
        point[2] += 0.22 * (phase as f64 * 0.017).sin();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typed_atom(number: u8, hybridization: Hybridization, bonded: bool) -> Molecule {
        let mut builder = MoleculeBuilder::new();
        let center = builder.add_atom(
            AtomSpec::new(Element::from_atomic_number(number).unwrap())
                .with_no_implicit(true)
                .with_hybridization(hybridization),
        );
        if bonded {
            let ligand = builder.add_atom(
                AtomSpec::new(Element::from_atomic_number(8).unwrap()).with_no_implicit(true),
            );
            builder
                .add_bond(BondSpec::new(center, ligand, BondOrder::Single))
                .unwrap();
        }
        builder
            .build()
            .unwrap()
            .sanitize_with_params(&SanitizeParams {
                operations: Ops::NONE,
            })
            .unwrap()
    }

    #[test]
    fn contact_note_covers_all_bonded_s_block_elements() {
        // Test the disclosure independently of parameter availability: the
        // pinned UFF typer does not admit every element (notably Sr).
        for (number, element) in [
            (3, "Li"),
            (11, "Na"),
            (19, "K"),
            (37, "Rb"),
            (55, "Cs"),
            (4, "Be"),
            (12, "Mg"),
            (20, "Ca"),
            (38, "Sr"),
            (56, "Ba"),
        ] {
            for bonded in [false, true] {
                let mol = typed_atom(number, Hybridization::Sp3, bonded);
                let notes = approximations(&mol).unwrap();
                let expected = format!(
                    "{element}: drawn contacts are modeled as UFF bonds and come out short"
                );
                assert_eq!(notes.contains(&expected), bonded, "{element}: {notes:?}");
            }
        }
    }

    #[test]
    fn square_planar_note_does_not_require_a_metal_or_type_fallback() {
        let mol = typed_atom(54, Hybridization::Sp2d, false);
        assert_eq!(
            approximations(&mol).unwrap(),
            ["Xe: UFF square planar angles can bend trans ligands"]
        );
    }
}
