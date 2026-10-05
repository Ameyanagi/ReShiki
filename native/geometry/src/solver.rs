//! Bounded adapter to the Rust COSMolKit molecular mechanics implementation.
use super::{ForceField, MAX_COORDINATES, Operation, Request, Response, valid_coordinate};
use cosmolkit_core::{
    AtomSpec, BondOrder, BondSpec, BondStereo, ChiralTag, Element, EmbedParameters,
    MmffMolProperties, MmffVariant, MolecularForceField, Molecule, MoleculeBuilder,
    embed_multiple_confs_result, mmff_get_molecule_force_field, uff_get_molecule_force_field,
    uff_has_all_molecule_params,
};

type Result<T> = std::result::Result<T, String>;
const ORIGINAL_INDEX: &str = "_reshikiGeometryOriginalIndex";

fn require(condition: bool, message: impl Into<String>) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}

fn molecule(request: &Request) -> Result<Molecule> {
    let mut builder = MoleculeBuilder::new();
    let mut ids = Vec::with_capacity(request.atoms.len());
    for (index, atom) in request.atoms.iter().enumerate() {
        require(
            matches!(
                atom.atomic_number,
                1 | 5 | 6 | 7 | 8 | 9 | 14 | 15 | 16 | 17 | 33 | 34 | 35 | 53
            ),
            format!("atom {index}: query, metal, or unsupported element"),
        )?;
        require(
            atom.radical == 0,
            format!("atom {index}: radicals are unsupported"),
        )?;
        require(
            (-8..=8).contains(&atom.charge),
            format!("atom {index}: formal charge is outside supported range"),
        )?;
        let element =
            Element::from_atomic_number(atom.atomic_number as u8).ok_or("Invalid element")?;
        let tag = match atom.chiral_tag {
            0 => ChiralTag::Unspecified,
            1 => ChiralTag::TetrahedralCw,
            2 => ChiralTag::TetrahedralCcw,
            _ => return Err("Unsupported atom stereo".into()),
        };
        let mut spec = AtomSpec::new(element)
            .with_formal_charge(atom.charge as i8)
            .with_explicit_hydrogens(atom.explicit_h as u8)
            .with_no_implicit(atom.no_implicit)
            .with_aromatic(atom.aromatic)
            .with_chiral_tag(tag)
            .with_prop(ORIGINAL_INDEX, index.to_string());
        if atom.isotope != 0 {
            spec = spec.with_isotope(atom.isotope as u16);
        }
        ids.push(builder.add_atom(spec));
    }
    let mut neighbors = vec![Vec::new(); ids.len()];
    for (index, bond) in request.bonds.iter().enumerate() {
        let order = match bond.order {
            1 => BondOrder::Single,
            2 => BondOrder::Double,
            3 => BondOrder::Triple,
            12 => BondOrder::Aromatic,
            _ => {
                return Err(format!(
                    "bond {index}: query or coordination bond is unsupported"
                ));
            }
        };
        let stereo = match bond.stereo {
            0 => BondStereo::None,
            1 => BondStereo::Any,
            2 => BondStereo::Z,
            3 => BondStereo::E,
            4 => BondStereo::Cis,
            5 => BondStereo::Trans,
            _ => return Err("Unsupported bond stereo".into()),
        };
        let mut spec = BondSpec::new(ids[bond.a], ids[bond.b], order)
            .with_aromatic(bond.aromatic || bond.order == 12)
            .with_stereo(stereo);
        if let Some([a, b]) = bond.stereo_atoms {
            spec = spec.with_stereo_atoms(ids[a], ids[b]);
        }
        builder.add_bond(spec).map_err(|error| error.to_string())?;
        neighbors[bond.a].push(bond.b);
        neighbors[bond.b].push(bond.a);
    }
    let mut reached = vec![false; ids.len()];
    let mut pending = vec![0];
    reached[0] = true;
    while let Some(index) = pending.pop() {
        for &neighbor in &neighbors[index] {
            if !reached[neighbor] {
                reached[neighbor] = true;
                pending.push(neighbor);
            }
        }
    }
    require(
        reached.iter().all(|value| *value),
        "geometry requests must contain one connected covalent component",
    )?;
    let mol = builder
        .build()
        .map_err(|error| error.to_string())?
        .sanitize()
        .map_err(|error| error.to_string())?;
    require(
        mol.atoms().iter().all(|atom| atom.radical_electrons() == 0),
        "sanitization produced an unsupported radical",
    )?;
    let mol = mol.with_hydrogens().map_err(|error| error.to_string())?;
    require(
        mol.num_atoms() <= MAX_COORDINATES,
        "hydrogen-expanded atom count exceeds 4096",
    )?;
    validate_original_order(&mol, request.atoms.len())?;
    Ok(mol)
}

fn validate_original_order(mol: &Molecule, original_count: usize) -> Result<()> {
    require(
        mol.num_atoms() >= original_count,
        "calculation lost original atoms",
    )?;
    for (index, atom) in mol.atoms().iter().take(original_count).enumerate() {
        require(
            atom.prop(ORIGINAL_INDEX) == Some(index.to_string().as_str()),
            "hydrogen expansion changed original atom ordering",
        )?;
    }
    Ok(())
}

fn hydrogen_parents(mol: &Molecule, original_count: usize) -> Result<Vec<usize>> {
    let mut result = Vec::with_capacity(mol.num_atoms() - original_count);
    for index in original_count..mol.num_atoms() {
        require(
            mol.atoms()[index].atomic_number() == 1,
            "hydrogen expansion appended a non-hydrogen atom",
        )?;
        let mut parents = mol.bonds().iter().filter_map(|bond| {
            if bond.begin().index() == index {
                Some(bond.end().index())
            } else if bond.end().index() == index {
                Some(bond.begin().index())
            } else {
                None
            }
        });
        let parent = parents.next().ok_or("calculation hydrogen has no parent")?;
        require(
            parent < original_count && parents.next().is_none(),
            "calculation hydrogen must have exactly one original parent",
        )?;
        result.push(parent);
    }
    Ok(result)
}

fn prepare_field_graph(mol: Molecule, field: ForceField) -> Result<Molecule> {
    if field == ForceField::UFF {
        require(
            uff_has_all_molecule_params(&mol).map_err(|error| error.to_string())?,
            "UFF parameters are unavailable for this molecule",
        )?;
        Ok(mol)
    } else {
        let properties =
            MmffMolProperties::new(&mol, variant(field).as_rdkit_str(), 0).map_err(|error| {
                format!("MMFF parameters are unavailable for this molecule: {error}")
            })?;
        require(
            properties.is_valid(),
            "MMFF parameters are unavailable for this molecule",
        )?;
        // MMFF prepares its own aromaticity graph; ETKDG and the field must use
        // that same calculation graph. It never leaves this worker.
        Ok(properties.molecule)
    }
}

fn variant(field: ForceField) -> MmffVariant {
    if field == ForceField::MMFF94s {
        MmffVariant::Mmff94s
    } else {
        MmffVariant::Mmff94
    }
}

fn force_field(mol: &Molecule, field: ForceField) -> Result<MolecularForceField> {
    let result = if field == ForceField::UFF {
        uff_get_molecule_force_field(mol, f64::INFINITY, 0, true)
            .map_err(|error| error.to_string())?
    } else {
        mmff_get_molecule_force_field(mol, variant(field), f64::INFINITY, 0, true)
            .map_err(|error| error.to_string())?
    };
    result.ok_or_else(|| {
        format!(
            "{} parameters are unavailable for this molecule",
            if field == ForceField::UFF {
                "UFF"
            } else {
                "MMFF"
            }
        )
    })
}

fn energy(field: &mut MolecularForceField) -> Result<f64> {
    let value = field.energy();
    require(value.is_finite(), "force field produced a nonfinite energy")?;
    Ok(value)
}

struct StereoExpectation {
    atoms: Vec<(usize, ChiralTag)>,
    bonds: Vec<(usize, usize, usize, usize, bool)>,
}

impl StereoExpectation {
    fn new(mol: &Molecule) -> Result<Self> {
        let mut atoms = Vec::new();
        for atom in mol.atoms() {
            if atom.chiral_tag() == ChiralTag::Unspecified {
                continue;
            }
            let index = atom.id().index();
            let degree = mol
                .bonds()
                .iter()
                .filter(|bond| bond.begin().index() == index || bond.end().index() == index)
                .count();
            require(
                (3..=4).contains(&degree),
                format!("atom {index}: invalid tetrahedral stereo degree"),
            )?;
            atoms.push((index, atom.chiral_tag()));
        }
        let mut bonds = Vec::new();
        for bond in mol.bonds() {
            if !matches!(
                bond.stereo(),
                BondStereo::Z | BondStereo::E | BondStereo::Cis | BondStereo::Trans
            ) {
                continue;
            }
            let refs = bond
                .stereo_atoms()
                .ok_or("specified double bond has no stereo references")?;
            bonds.push((
                bond.begin().index(),
                bond.end().index(),
                refs[0].index(),
                refs[1].index(),
                matches!(bond.stereo(), BondStereo::Z | BondStereo::Cis),
            ));
        }
        Ok(Self { atoms, bonds })
    }

    fn validate(&self, mol: &Molecule, coordinates: &[[f64; 3]]) -> Result<()> {
        validate_coordinates(mol, coordinates)?;
        if !self.atoms.is_empty() {
            let derived = mol
                .with_only_3d_conformer(coordinates.to_vec(), true)
                .map_err(|error| error.to_string())?
                .with_chiral_tags_from_structure(0, true)
                .map_err(|error| error.to_string())?;
            for &(index, expected) in &self.atoms {
                require(
                    derived.atoms()[index].chiral_tag() == expected,
                    format!("atom {index}: specified tetrahedral stereo inverted or degenerated"),
                )?;
            }
        }
        for &(a, b, ref_a, ref_b, same_side) in &self.bonds {
            let axis = sub(coordinates[b], coordinates[a]);
            let axis2 = dot(axis, axis);
            require(axis2 > 1e-8, "specified double-bond endpoints coincide")?;
            let u = sub(coordinates[ref_a], coordinates[a]);
            let v = sub(coordinates[ref_b], coordinates[b]);
            let u = sub(u, scale(axis, dot(u, axis) / axis2));
            let v = sub(v, scale(axis, dot(v, axis) / axis2));
            let norm2 = dot(u, u) * dot(v, v);
            require(
                norm2 > 1e-12,
                "specified double-bond stereo references are collinear",
            )?;
            let cosine = dot(u, v) / norm2.sqrt();
            require(
                cosine.abs() >= 0.8 && (cosine > 0.) == same_side,
                "specified double-bond stereo inverted or degenerated",
            )?;
        }
        Ok(())
    }
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}
fn scale(a: [f64; 3], value: f64) -> [f64; 3] {
    a.map(|x| x * value)
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}

fn validate_coordinates(mol: &Molecule, coordinates: &[[f64; 3]]) -> Result<()> {
    require(
        coordinates.len() == mol.num_atoms() && coordinates.iter().all(valid_coordinate),
        "invalid all-atom calculation coordinates",
    )
}

fn validate_geometry(mol: &Molecule, coordinates: &[[f64; 3]]) -> Result<()> {
    validate_coordinates(mol, coordinates)?;
    for bond in mol.bonds() {
        let delta = sub(
            coordinates[bond.begin().index()],
            coordinates[bond.end().index()],
        );
        require(
            (0.04..=16.).contains(&dot(delta, delta)),
            "optimized covalent bond is collapsed or exceeds 4 Angstrom",
        )?;
    }
    for (index, &point) in coordinates.iter().enumerate() {
        for &other in &coordinates[index + 1..] {
            let delta = sub(point, other);
            require(
                dot(delta, delta) >= 0.04,
                "optimized atoms overlap within 0.2 Angstrom",
            )?;
        }
    }
    Ok(())
}

struct Candidate {
    coordinates: Vec<[f64; 3]>,
    initial_energy: f64,
    energy: f64,
    converged: bool,
}

fn minimized(
    mol: &Molecule,
    coordinates: &[[f64; 3]],
    expected: &StereoExpectation,
    request: &Request,
) -> Result<Candidate> {
    expected.validate(mol, coordinates)?;
    let positioned = mol
        .with_only_3d_conformer(coordinates.to_vec(), true)
        .map_err(|error| error.to_string())?;
    let mut field = force_field(&positioned, request.field)?;
    field
        .set_fixed_points(&request.fixed_atoms)
        .map_err(|error| error.to_string())?;
    let initial = energy(&mut field)?;
    let status = field
        .minimize(request.max_iterations as usize, 1e-4, 1e-6)
        .map_err(|error| error.to_string())?;
    require(
        status == 0 || status == 1,
        "unexpected force-field optimizer status",
    )?;
    let final_energy = energy(&mut field)?;
    require(
        final_energy <= initial + 1e-6,
        "force-field minimization increased the energy",
    )?;
    let result = field.positions();
    validate_geometry(mol, &result)?;
    expected.validate(mol, &result)?;
    for &index in &request.fixed_atoms {
        require(
            result[index] == coordinates[index],
            "force field changed a fixed atom coordinate",
        )?;
    }
    Ok(Candidate {
        coordinates: result,
        initial_energy: initial,
        energy: final_energy,
        converged: status == 0,
    })
}

pub(super) fn solve(request: &Request) -> Result<Response> {
    let mol = molecule(request)?;
    let expected = StereoExpectation::new(&mol)?;
    let mol = prepare_field_graph(mol, request.field)?;
    validate_original_order(&mol, request.atoms.len())?;
    let parents = hydrogen_parents(&mol, request.atoms.len())?;
    let mut response = Response {
        coordinates: Vec::new(),
        hydrogen_parents: parents,
        original_count: request.atoms.len(),
        initial_energy: 0.,
        energy: 0.,
        gradient: None,
        converged: false,
        field: request.field,
        diagnostics: Vec::new(),
    };
    if request.operation == Operation::Generate {
        let mut parameters = EmbedParameters::etkdg_v3();
        parameters.random_seed = request.seed;
        parameters.num_threads = 1;
        parameters.max_iterations = 100;
        parameters.timeout = 10;
        parameters.enforce_chirality = true;
        parameters.prune_rms_thresh = 0.1;
        parameters.clear_confs = true;
        parameters.enable_sequential_random_seeds = true;
        parameters.track_failures = true;
        let embedded = embed_multiple_confs_result(&mol, request.conformers, &mut parameters)
            .map_err(|error| error.to_string())?;
        let failures = parameters
            .failures
            .iter()
            .enumerate()
            .filter(|(_, count)| **count != 0)
            .map(|(i, count)| format!("{i}:{count}"))
            .collect::<Vec<_>>()
            .join(" ");
        require(
            !embedded.conf_ids.is_empty(),
            format!(
                "ETKDGv3 could not generate a conformer; embedding failure counters={failures}"
            ),
        )?;
        let mut best = f64::INFINITY;
        let mut valid = 0;
        let mut rejected = 0;
        let mut last_failure = String::new();
        for id in &embedded.conf_ids {
            let conformer = embedded
                .molecule
                .conformers_3d()
                .iter()
                .find(|conformer| conformer.id() == *id as usize)
                .ok_or("embedding lost a conformer")?;
            match minimized(
                &embedded.molecule,
                conformer.coordinates(),
                &expected,
                request,
            ) {
                Ok(candidate) => {
                    valid += 1;
                    if candidate.energy < best {
                        best = candidate.energy;
                        response.coordinates = candidate.coordinates;
                        response.initial_energy = candidate.initial_energy;
                        response.energy = candidate.energy;
                        response.converged = candidate.converged;
                    }
                }
                Err(error) => {
                    rejected += 1;
                    last_failure = error.chars().take(512).collect();
                }
            }
        }
        require(
            valid > 0,
            format!("all generated conformers failed validation: {last_failure}"),
        )?;
        response.diagnostics.push(format!("Rust ETKDGv3; requested={}; embedded={}; valid={valid}; rejected={rejected}; seed={}; selection=lowest energy among valid generated conformers; iteration_limit={}; converged={}; embedding_failure_counters={failures}", request.conformers, embedded.conf_ids.len(), request.seed, request.max_iterations, response.converged));
        if !last_failure.is_empty() {
            response
                .diagnostics
                .push(format!("last_rejection={last_failure}"));
        }
    } else if request.operation == Operation::Relax {
        let candidate = minimized(&mol, &request.coordinates, &expected, request)?;
        response.coordinates = candidate.coordinates;
        response.initial_energy = candidate.initial_energy;
        response.energy = candidate.energy;
        response.converged = candidate.converged;
        response.diagnostics.push(format!(
            "bounded Rust relaxation; fixed_atoms={}; iteration_limit={}; converged={}",
            request.fixed_atoms.len(),
            request.max_iterations,
            response.converged
        ));
    } else {
        validate_geometry(&mol, &request.coordinates)?;
        expected.validate(&mol, &request.coordinates)?;
        let positioned = mol
            .with_only_3d_conformer(request.coordinates.clone(), true)
            .map_err(|error| error.to_string())?;
        let mut field = force_field(&positioned, request.field)?;
        response.energy = energy(&mut field)?;
        response.initial_energy = response.energy;
        response.coordinates = field.positions();
        let gradient = field.gradient();
        require(
            gradient.len() == mol.num_atoms()
                && gradient.iter().flatten().all(|value| value.is_finite()),
            "force field produced an invalid analytic gradient",
        )?;
        response.gradient = Some(gradient);
        response
            .diagnostics
            .push("Rust energy and analytic gradient; no minimization".into());
    }
    Ok(response)
}
