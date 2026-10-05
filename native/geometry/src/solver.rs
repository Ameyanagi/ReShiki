//! Bounded adapter to the Rust COSMolKit molecular mechanics implementation.
use super::{
    ForceField, MAX_COORDINATES, Operation, Request, Response, cage::Cage, valid_coordinate,
};
use cosmolkit_core::{
    AddHsParams, AtomSpec, BondOrder, BondSpec, BondStereo, ChiralTag, Element, EmbedFailureCause,
    EmbedMultipleConfsResult, EmbedParameters, MmffMolProperties, MmffVariant, MolecularForceField,
    Molecule, MoleculeBuilder, embed_multiple_confs_result, mmff_get_molecule_force_field,
    uff_get_molecule_force_field, uff_has_all_molecule_params,
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
    Ok(mol)
}

fn with_hydrogens(mol: &Molecule, original_count: usize, add_coords: bool) -> Result<Molecule> {
    let mol = mol
        .with_hydrogens_with_params(AddHsParams {
            add_coords,
            ..Default::default()
        })
        .map_err(|error| error.to_string())?;
    require(
        mol.num_atoms() <= MAX_COORDINATES,
        "hydrogen-expanded atom count exceeds 4096",
    )?;
    validate_original_order(&mol, original_count)?;
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

// Test the principal extents, not the drawing's z coordinates: a flat drawing
// remains flat after rotation. A conservative thickness threshold also avoids
// treating roundoff or tiny display offsets as a physical starting geometry.
fn has_three_dimensional_extent(coordinates: &[[f64; 3]]) -> bool {
    if coordinates.len() < 4 || !coordinates.iter().all(valid_coordinate) {
        return false;
    }
    let count = coordinates.len() as f64;
    let center: [f64; 3] =
        std::array::from_fn(|axis| coordinates.iter().map(|p| p[axis]).sum::<f64>() / count);
    let mut covariance = [[0.; 3]; 3];
    for &point in coordinates {
        let delta = sub(point, center);
        for row in 0..3 {
            for column in 0..3 {
                covariance[row][column] += delta[row] * delta[column] / count;
            }
        }
    }
    // Jacobi diagonalization of this small symmetric matrix.
    for _ in 0..24 {
        let (p, q) = [(0, 1), (0, 2), (1, 2)]
            .into_iter()
            .max_by(|&(a, b), &(c, d)| covariance[a][b].abs().total_cmp(&covariance[c][d].abs()))
            .unwrap();
        let off_diagonal = covariance[p][q];
        let trace = covariance[0][0] + covariance[1][1] + covariance[2][2];
        if off_diagonal.abs() <= trace.abs() * 1e-14 {
            break;
        }
        let angle = 0.5 * (2. * off_diagonal).atan2(covariance[q][q] - covariance[p][p]);
        let (sin, cos) = angle.sin_cos();
        let pp = covariance[p][p];
        let qq = covariance[q][q];
        covariance[p][p] = cos * cos * pp - 2. * sin * cos * off_diagonal + sin * sin * qq;
        covariance[q][q] = sin * sin * pp + 2. * sin * cos * off_diagonal + cos * cos * qq;
        covariance[p][q] = 0.;
        covariance[q][p] = 0.;
        // A 3×3 covariance matrix has one axis outside the pivot pair.
        let other = 3 - p - q;
        let op = covariance[other][p];
        let oq = covariance[other][q];
        covariance[other][p] = cos * op - sin * oq;
        covariance[p][other] = covariance[other][p];
        covariance[other][q] = sin * op + cos * oq;
        covariance[q][other] = covariance[other][q];
    }
    let mut extents = [covariance[0][0], covariance[1][1], covariance[2][2]];
    extents.sort_by(f64::total_cmp);
    extents[0] > 1e-4 && extents[0] > extents[2] * 1e-4
}

fn existing_coordinates(
    original: &Molecule,
    mol: &Molecule,
    expected: &StereoExpectation,
    cage: Option<&Cage>,
    request: &Request,
) -> Result<Vec<[f64; 3]>> {
    validate_geometry(original, &request.coordinates)?;
    require(
        has_three_dimensional_extent(&request.coordinates),
        "starting geometry is flat, dimensionless, or has insufficient 3D extent",
    )?;
    let positioned = original
        .with_only_3d_conformer(request.coordinates.clone(), true)
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
        .ok_or("coordinate-aware hydrogen expansion lost the starting geometry")?
        .coordinates()
        .to_vec();
    validate_geometry(mol, &coordinates)?;
    expected.validate(mol, &coordinates)?;
    if let Some(cage) = cage {
        cage.validate(mol, &coordinates)?;
    }
    Ok(coordinates)
}

fn embedding_parameters(request: &Request) -> EmbedParameters {
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
    parameters
}

fn embedding_failures(parameters: &EmbedParameters) -> String {
    parameters
        .failures
        .iter()
        .enumerate()
        .filter(|(_, count)| **count != 0)
        .map(|(i, count)| format!("{i}:{count}"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn valid_conformer_ids(ids: &[i32]) -> impl Iterator<Item = i32> + '_ {
    // COSMolKit uses negative IDs as failure/timeout sentinels. They are not
    // conformers and must never be cast to an unsigned molecule index.
    ids.iter().copied().filter(|&id| id >= 0)
}

#[derive(Debug, PartialEq, Eq)]
enum EmbeddingRetry {
    WithoutBasicKnowledge,
    SingleConformer,
}

fn embedding_retry(
    parameters: &EmbedParameters,
    ids: &[i32],
    conformers: u32,
) -> Option<EmbeddingRetry> {
    if valid_conformer_ids(ids).next().is_some() {
        return None;
    }
    let planarity = u64::from(
        parameters
            .failures
            .get(EmbedFailureCause::EtkMinimization as usize)
            .copied()
            .unwrap_or(0),
    );
    let failures: u64 = parameters
        .failures
        .iter()
        .map(|&count| u64::from(count))
        .sum();
    if parameters.use_basic_knowledge && planarity * 2 > failures {
        Some(EmbeddingRetry::WithoutBasicKnowledge)
    } else if conformers > 1
        && parameters
            .failures
            .get(EmbedFailureCause::ExceededTimeout as usize)
            .is_some_and(|&count| count > 0)
    {
        Some(EmbeddingRetry::SingleConformer)
    } else {
        None
    }
}

fn embed(
    mol: &Molecule,
    request: &Request,
    cage: Option<&Cage>,
    diagnostics: &mut Vec<String>,
) -> Result<(EmbedMultipleConfsResult, EmbedParameters, &'static str)> {
    let mut parameters = embedding_parameters(request);
    let mut method = "Rust ETKDGv3";
    let mut calls = 0;
    let mut conformers = request.conformers;
    if cage.is_some() {
        // One seeded attempt diagnoses the known planarity incompatibility.
        // With clear_confs and one candidate, RMS pruning cannot reject it;
        // disabling pruning avoids needless unbounded symmetry matching.
        let mut trial = parameters.clone();
        trial.max_iterations = 1;
        trial.prune_rms_thresh = -1.;
        let result =
            embed_multiple_confs_result(mol, 1, &mut trial).map_err(|error| error.to_string())?;
        calls += 1;
        let planarity_rejected = result.conf_ids.is_empty()
            && trial.failures[EmbedFailureCause::EtkMinimization as usize] == 1;
        if planarity_rejected {
            parameters.use_basic_knowledge = false;
            method = "Rust cage ETDG";
            diagnostics.push(format!(
                "initialization=cage-ETDG; closed sp2 carbon cage; ETKDGv3 planarity trial rejected; trial_attempt_limit=1; trial_failure_counters={}; basic_knowledge=false; experimental_torsions=true",
                embedding_failures(&trial)
            ));
        }
    }
    loop {
        calls += 1;
        let embedded = embed_multiple_confs_result(mol, conformers, &mut parameters)
            .map_err(|error| error.to_string())?;
        // Each call retains its 100-attempt/10-second limits; the disposable
        // worker also bounds the entire operation. Count the cage pilot here.
        if calls >= 3 {
            return Ok((embedded, parameters, method));
        }
        match embedding_retry(&parameters, &embedded.conf_ids, conformers) {
            Some(EmbeddingRetry::WithoutBasicKnowledge) => {
                diagnostics.push(format!(
                    "initialization=ETDG-fallback; requested={}; actual_count={conformers}; seed={}; prior_embedding_failure_counters={}; basic_knowledge=false; experimental_torsions=true; embedding_call_limit=3",
                    request.conformers, request.seed, embedding_failures(&parameters)
                ));
                parameters.use_basic_knowledge = false;
                method = "Rust ETDG fallback";
            }
            Some(EmbeddingRetry::SingleConformer) => {
                diagnostics.push(format!(
                    "initialization=single-conformer; requested={}; prior_actual_count={conformers}; actual_count=1; seed={}; prior_embedding_failure_counters={}; embedding_call_limit=3",
                    request.conformers, request.seed, embedding_failures(&parameters)
                ));
                conformers = 1;
                // With no existing conformers, this has no pruning effect and
                // avoids needless symmetry matching for a sole retry candidate.
                parameters.prune_rms_thresh = -1.;
            }
            None => return Ok((embedded, parameters, method)),
        }
    }
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
    cage: Option<&Cage>,
    request: &Request,
    origin: Option<[f64; 3]>,
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
    if let Some(origin) = origin {
        require(
            request.fixed_atoms.is_empty(),
            "centered generation cannot fix atoms",
        )?;
        let centered: Vec<_> = coordinates
            .iter()
            .map(|&point| sub(point, origin))
            .collect();
        field
            .set_positions(&centered)
            .map_err(|error| error.to_string())?;
    }
    let status = field
        .minimize(request.max_iterations as usize, 1e-4, 1e-6)
        .map_err(|error| error.to_string())?;
    require(
        status == 0 || status == 1,
        "unexpected force-field optimizer status",
    )?;
    if let Some(origin) = origin {
        let restored: Vec<_> = field
            .positions()
            .iter()
            .map(|point| std::array::from_fn(|axis| point[axis] + origin[axis]))
            .collect();
        field
            .set_positions(&restored)
            .map_err(|error| error.to_string())?;
    }
    let final_energy = energy(&mut field)?;
    require(
        final_energy <= initial + 1e-6,
        "force-field minimization increased the energy",
    )?;
    let result = field.positions();
    validate_geometry(mol, &result)?;
    expected.validate(mol, &result)?;
    if let Some(cage) = cage {
        cage.validate(mol, &result)?;
    }
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
    let original = molecule(request)?;
    let mol = with_hydrogens(&original, request.atoms.len(), false)?;
    let expected = StereoExpectation::new(&mol)?;
    let mol = prepare_field_graph(mol, request.field)?;
    let cage = Cage::from_molecule(&mol);
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
        if !request.coordinates.is_empty() {
            match existing_coordinates(&original, &mol, &expected, cage.as_ref(), request).and_then(
                |coordinates| {
                    // The upstream optimizer scales its stopping test by
                    // absolute XYZ. Center only this new unpinned seed path,
                    // then restore the drawing's location before validation.
                    let origin = std::array::from_fn(|axis| {
                        coordinates[..request.atoms.len()]
                            .iter()
                            .map(|point| point[axis])
                            .sum::<f64>()
                            / request.atoms.len() as f64
                    });
                    minimized(
                        &mol,
                        &coordinates,
                        &expected,
                        cage.as_ref(),
                        request,
                        Some(origin),
                    )
                },
            ) {
                Ok(candidate) => {
                    response.coordinates = candidate.coordinates;
                    response.initial_energy = candidate.initial_energy;
                    response.energy = candidate.energy;
                    response.converged = candidate.converged;
                    response.diagnostics.push(format!(
                        "initialization=existing-3d; reused existing original-atom 3D coordinates; temporary hydrogens added with coordinate-aware AddHs; selection=validated existing geometry; iteration_limit={}; converged={}",
                        request.max_iterations, response.converged
                    ));
                    return Ok(response);
                }
                Err(error) => response.diagnostics.push(format!(
                    "existing original-atom coordinates ignored: {}",
                    error.chars().take(512).collect::<String>()
                )),
            }
        }
        let (embedded, parameters, method) =
            embed(&mol, request, cage.as_ref(), &mut response.diagnostics)?;
        let failures = embedding_failures(&parameters);
        let embedded_count = valid_conformer_ids(&embedded.conf_ids).count();
        let failed_embedding_ids = embedded.conf_ids.len() - embedded_count;
        require(
            embedded_count > 0,
            format!(
                "{method} could not generate a conformer; embedding failure counters={failures}"
            ),
        )?;
        let mut best = f64::INFINITY;
        let mut valid = 0;
        let mut rejected = 0;
        let mut last_failure = String::new();
        for id in valid_conformer_ids(&embedded.conf_ids) {
            let conformer = embedded
                .molecule
                .conformers_3d()
                .iter()
                .find(|conformer| conformer.id() == id as usize)
                .ok_or("embedding lost a conformer")?;
            match minimized(
                &embedded.molecule,
                conformer.coordinates(),
                &expected,
                cage.as_ref(),
                request,
                None,
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
        response.diagnostics.push(format!("{method}; requested={}; actual_count={}; embedded={embedded_count}; failed_embedding_ids={failed_embedding_ids}; valid={valid}; rejected={rejected}; seed={}; selection=lowest energy among valid generated conformers; iteration_limit={}; converged={}; embedding_failure_counters={failures}", request.conformers, embedded.requested_num_confs, request.seed, request.max_iterations, response.converged));
        if !last_failure.is_empty() {
            response
                .diagnostics
                .push(format!("last_rejection={last_failure}"));
        }
    } else if request.operation == Operation::Relax {
        if let Some(cage) = &cage {
            cage.validate(&mol, &request.coordinates)?;
        }
        let candidate = minimized(
            &mol,
            &request.coordinates,
            &expected,
            cage.as_ref(),
            request,
            None,
        )?;
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
        if let Some(cage) = &cage {
            cage.validate(&mol, &request.coordinates)?;
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_embedding_ids_are_skipped_without_losing_valid_conformers() {
        assert_eq!(
            valid_conformer_ids(&[0, -1, 4, -7]).collect::<Vec<_>>(),
            [0, 4]
        );
        assert_eq!(valid_conformer_ids(&[-1]).count(), 0);
        assert_eq!(valid_conformer_ids(&[]).count(), 0);
    }

    #[test]
    fn retries_require_failure_evidence_and_preserve_successful_ensembles() {
        let mut parameters = EmbedParameters::etkdg_v3();
        parameters.failures = vec![0; EmbedFailureCause::EndOfEnum as usize];
        parameters.failures[EmbedFailureCause::ExceededTimeout as usize] = 1;
        assert_eq!(
            embedding_retry(&parameters, &[-1], 8),
            Some(EmbeddingRetry::SingleConformer)
        );
        assert_eq!(embedding_retry(&parameters, &[-1], 1), None);
        assert_eq!(embedding_retry(&parameters, &[0, -1], 8), None);
        parameters.failures[EmbedFailureCause::InitialCoords as usize] = 13;
        parameters.failures[EmbedFailureCause::EtkMinimization as usize] = 87;
        assert_eq!(
            embedding_retry(&parameters, &[], 8),
            Some(EmbeddingRetry::WithoutBasicKnowledge)
        );
        assert_eq!(
            embedding_retry(&parameters, &[], 1),
            Some(EmbeddingRetry::WithoutBasicKnowledge)
        );
        assert_eq!(embedding_retry(&parameters, &[0], 8), None);
        parameters.use_basic_knowledge = false;
        assert_eq!(
            embedding_retry(&parameters, &[-1], 8),
            Some(EmbeddingRetry::SingleConformer)
        );
        assert_eq!(embedding_retry(&parameters, &[], 1), None);
        parameters.use_basic_knowledge = true;
        parameters.failures.fill(0);
        parameters.failures[EmbedFailureCause::InitialCoords as usize] = 50;
        parameters.failures[EmbedFailureCause::EtkMinimization as usize] = 50;
        assert_eq!(embedding_retry(&parameters, &[], 8), None);
        parameters.failures[EmbedFailureCause::EtkMinimization as usize] = 51;
        assert_eq!(
            embedding_retry(&parameters, &[], 8),
            Some(EmbeddingRetry::WithoutBasicKnowledge)
        );
        parameters.failures.clear();
        assert_eq!(embedding_retry(&parameters, &[], 8), None);
    }

    #[test]
    fn seed_dimensionality_is_intrinsic_and_requires_physical_thickness() {
        let spatial = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
        assert!(has_three_dimensional_extent(&spatial));
        let rotated = spatial.map(|[x, y, z]| {
            let u = (x + z) / 2_f64.sqrt();
            let v = (z - x) / 2_f64.sqrt();
            [
                u + 100.,
                (y + v) / 2_f64.sqrt() - 20.,
                (v - y) / 2_f64.sqrt() + 8.,
            ]
        });
        assert!(has_three_dimensional_extent(&rotated));
        let tilted_plane = [[0., 0., 0.], [1., 0., 1.], [0., 1., 1.], [1., 1., 2.]];
        assert!(!has_three_dimensional_extent(&tilted_plane));
        let almost_flat = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [1., 1., 1e-5]];
        assert!(!has_three_dimensional_extent(&almost_flat));
        assert!(!has_three_dimensional_extent(&[[0.; 3]; 4]));
        assert!(!has_three_dimensional_extent(&spatial[..3]));
        let dimensionless = spatial.map(|p| p.map(|v| v * 1e-3));
        assert!(!has_three_dimensional_extent(&dimensionless));
        let nonfinite = [[f64::NAN, 0., 0.]; 4];
        assert!(!has_three_dimensional_extent(&nonfinite));
    }
}
