use super::*;
use crate::{
    document::{CALCULATIONS, History, Point, VERSION},
    editing,
    models3d::{Cell, ForceField, Hydrogen, Model3d, Placed},
    transaction,
};
use reshiki_qcdata::{
    AtomicCharges, BasisSet, Beta, BondOrder, ChargeScheme, OrbitalSet, Orbitals, Provenance,
    Shell, Source, Status, Task, Thermo, Vibrations, XtbMethod, XtbSettings,
};
use serde_json::{Value, json};

fn water() -> (Document, u64) {
    let mut doc = Document::default();
    let oxygen = doc.add_atom("O", Point::new(20., 30.));
    let atoms = vec![Placed {
        id: oxygen,
        xyz: [0.123_456_79, -0.25, 0.125],
    }];
    let hydrogens = vec![
        Hydrogen {
            parent: oxygen,
            xyz: [0.9, 0.55, -0.1],
        },
        Hydrogen {
            parent: oxygen,
            xyz: [-0.6, 0.65, 0.2],
        },
    ];
    doc.models
        .push(Model3d::new(&doc, atoms, hydrogens, ForceField::Mmff94s));
    crate::models3d::require_version(&mut doc);
    (doc, oxygen)
}

fn water_pair() -> (Document, [u64; 2]) {
    let (mut doc, first) = water();
    let second = editing::append(&mut doc, &water().0, Point::new(150., 0.))[0];
    (doc, [first, second])
}

fn ethanol() -> (Document, [u64; 3]) {
    let mut doc = Document::default();
    let ids = ["C", "C", "O"].map(|element| {
        let index = doc.atoms.len();
        doc.add_atom(element, Point::new(index as f32 * 36., 0.))
    });
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[1], ids[2], 1, "plain");
    let atoms = ids
        .iter()
        .enumerate()
        .map(|(index, &id)| Placed {
            id,
            xyz: [index as f32 * 1.3, (index % 2) as f32 * 0.4, 0.1],
        })
        .collect();
    let hydrogens = vec![
        Hydrogen {
            parent: ids[0],
            xyz: [-0.5, 0.8, 0.3],
        },
        Hydrogen {
            parent: ids[2],
            xyz: [3.1, 0.6, -0.2],
        },
    ];
    doc.models
        .push(Model3d::new(&doc, atoms, hydrogens, ForceField::Mmff94s));
    crate::models3d::require_version(&mut doc);
    (doc, ids)
}

/// A fixed carbon chain, with the atom count also being the scientific frame count.
fn chain(n: usize) -> (Document, Vec<u64>) {
    let mut doc = Document::default();
    let ids: Vec<_> = (0..n)
        .map(|i| doc.add_atom("C", Point::new(i as f32 * 28., (i % 2) as f32 * 14.)))
        .collect();
    for pair in ids.windows(2) {
        doc.add_bond(pair[0], pair[1], 1, "plain");
    }
    let atoms = ids
        .iter()
        .enumerate()
        .map(|(i, &id)| Placed {
            id,
            xyz: [i as f32 * 1.4, (i % 2) as f32 * 0.6, (i % 3) as f32 * 0.1],
        })
        .collect();
    doc.models
        .push(Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s));
    crate::models3d::require_version(&mut doc);
    (doc, ids)
}

fn reconciled_edit(doc: &mut Document, history: &mut History, edit: impl FnOnce(&mut Document)) {
    let before = doc.clone();
    edit(doc);
    assert!(
        transaction::reconcile(doc, before)
            .unwrap()
            .commit(doc, history, false)
            .recorded
    );
}

/// Reproducible f32 values with varying mantissas, rather than compressible zero arrays.
fn random_values(count: usize, seed: u64) -> Vec<f64> {
    let mut state = seed;
    (0..count)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1);
            f64::from(((state >> 40) as f32 / 8_388_608.) - 1.)
        })
        .collect()
}

/// Localized modes, orbitals and force constants along the synthetic chain.
fn banded_values(rows: usize, columns: usize, seed: u64) -> Vec<f64> {
    let width = columns.div_ceil(16);
    let samples = random_values(rows * width, seed);
    let mut values = vec![0.; rows * columns];
    for (i, (row, samples)) in values
        .chunks_exact_mut(columns)
        .zip(samples.chunks_exact(width))
        .enumerate()
    {
        let start = (i * columns / rows)
            .saturating_sub(width / 2)
            .min(columns - width);
        row[start..start + width].copy_from_slice(samples);
    }
    values
}

fn result(frame: &Frame, task: Task, full: bool) -> CalcResult {
    let n = frame.numbers.len();
    let axes = n * 3;
    let modes = axes.saturating_sub(6);
    let mut shells = Vec::new();
    for (atom, &number) in frame.numbers.iter().enumerate() {
        for l in 0..=u8::from(number != 1) {
            shells.push(Shell {
                atom: atom as u32,
                l,
                pure: false,
                exponents_per_bohr2: vec![1.],
                coefficients: vec![1.],
            });
        }
    }
    let n_ao: usize = shells.iter().map(Shell::n_functions).sum();
    let mut displacements = if full {
        banded_values(modes, axes, 0x51_7e)
    } else {
        vec![]
    };
    for mode in displacements.chunks_exact_mut(axes) {
        let norm = mode.iter().map(|x| x * x).sum::<f64>().sqrt();
        for component in mode {
            // Use the exact q16 decoded values, so native reopen can compare equality.
            *component = (*component / norm * 32_767.).round() / 32_767.;
        }
    }
    let result = CalcResult {
        frame: frame.hash(),
        input_frame: None,
        task,
        endpoints: None,
        settings: Settings::Xtb(XtbSettings {
            method: XtbMethod::Gfn2Xtb,
            conventions: Default::default(),
            solvent: None,
            electronic_temperature_k: None,
            accuracy: None,
            max_iterations: None,
            scf_retries: vec![],
            gfnff: None,
            threads: None,
            convergence: Default::default(),
            fixed_atoms: vec![],
        }),
        provenance: Provenance {
            program: "axtb".into(),
            program_version: Some("test".into()),
            method: Some("GFN2-xTB".into()),
            basis: None,
            source: Source::Calculated {
                engine_revision: "test".into(),
            },
        },
        status: Status::Converged,
        energy_hartree: Some(-(n as f64) * 2.5),
        energy_terms: [("electronic".into(), -(n as f64) * 2.6)].into(),
        gradient_hartree_per_bohr: if full {
            random_values(axes, 0x67_ad)
        } else {
            vec![]
        },
        hessian_hartree_per_bohr2: if full {
            banded_values(axes, axes, 0x4e_55)
        } else {
            vec![]
        },
        charges: vec![AtomicCharges {
            scheme: ChargeScheme::Mulliken,
            values: random_values(n, 0xc4_a6),
        }],
        dipole_e_bohr: Some([0.1, -0.2, 0.3]),
        bond_orders: (1..n)
            .map(|i| BondOrder {
                a: (i - 1) as u32,
                b: i as u32,
                order: 1.,
            })
            .collect(),
        orbitals: (frame.multiplicity == 1).then(|| Orbitals {
            basis: full.then_some(BasisSet { shells }),
            core_electrons: frame
                .numbers
                .iter()
                .map(|&number| if number > 2 { 2 } else { 0 })
                .collect(),
            alpha: OrbitalSet {
                energies_hartree: (0..n_ao).map(|i| -2. + i as f64 * 0.01).collect(),
                occupations: (0..n_ao)
                    .map(|i| if i < n_ao / 2 { 2. } else { 0. })
                    .collect(),
                symmetries: vec![],
                rows: if full {
                    (0..n_ao as u32).collect()
                } else {
                    vec![]
                },
                coefficients: if full {
                    banded_values(n_ao, n_ao, 0xc0_ef)
                } else {
                    vec![]
                },
            },
            beta: Beta::Restricted,
        }),
        vibrations: Some(Vibrations {
            frequencies_cm1: (0..modes).map(|i| 40. + i as f64 * 6.).collect(),
            ir_intensities_km_mol: random_values(modes, 0x1a_aa)
                .into_iter()
                .map(|x| (x + 1.) * 50.)
                .collect(),
            modes: displacements,
            reduced_masses_amu: if full { vec![1.; modes] } else { vec![] },
            masses_amu: if full { vec![12.; n] } else { vec![] },
        }),
        thermo: Some(Thermo {
            temperature_k: 298.15,
            pressure_pa: 101_325.,
            zero_point_energy_hartree: 0.05,
            enthalpy_correction_hartree: 0.06,
            entropy_hartree_per_k: 0.000_1,
            gibbs_correction_hartree: 0.06 - 298.15 * 0.000_1,
            heat_capacity_hartree_per_k: Some(0.000_02),
            frequency_scale: 1.,
            rotor_cutoff_cm1: 100.,
            imaginary_cutoff_cm1: 50.,
            symmetry_number: 1,
            point_group: Some("C1".into()),
        }),
        trajectory: None,
        keep_full: false,
        omitted_bytes: None,
        run: None,
    };
    result.validate(frame).unwrap();
    result
}

fn imported(mut result: CalcResult) -> CalcResult {
    result.settings = Settings::Imported {
        keywords: vec!["opt".into(), "freq".into()],
    };
    result.provenance.source = Source::Imported {
        format: "molden".into(),
        file_name: "test.molden".into(),
        file_sha256: None,
        job: 0,
    };
    result
}

fn add_result(doc: &mut Document, start: &Launch, result: CalcResult) -> Attached {
    let mut attached = None;
    reconciled_edit(doc, &mut History::default(), |doc| {
        attached = Some(attach(doc, start, vec![], result).unwrap());
    });
    attached.unwrap()
}

#[test]
fn result_key_uses_identity_and_ignores_result_values() {
    let (doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Energy, true);
    let key = result_key(&full);
    assert_eq!(key.len(), 16);
    assert!(
        key.bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    );
    assert_eq!(result_key(&full.light(&[])), key);
    let mut changed = full.clone();
    changed.orbitals = None;
    changed.energy_hartree = Some(-500.);
    changed.keep_full = true;
    changed.omitted_bytes = Some(20_000);
    assert_eq!(result_key(&changed), key);

    let mut identities = Vec::new();
    let mut changed = full.clone();
    changed.frame = FrameHash([0; 32]);
    identities.push(changed);
    let mut changed = full.clone();
    changed.input_frame = Some(full.frame);
    identities.push(changed);
    let mut changed = full.clone();
    changed.task = Task::Gradient;
    identities.push(changed);
    let mut changed = full.clone();
    changed.settings = Settings::Imported { keywords: vec![] };
    identities.push(changed);
    let mut changed = full;
    changed.provenance.program_version = Some("another version".into());
    identities.push(changed);
    for changed in identities {
        assert_ne!(result_key(&changed), key);
    }
}

#[test]
fn figure_result_and_pinning_follow_the_model_across_paste() {
    use crate::models3d::surface::{Figure, OrbitalRef, draw::figure_ready};

    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Energy, true);
    let basis = full.orbitals.as_ref().unwrap().basis.as_ref().unwrap();
    let n_ao = basis.shells.iter().map(Shell::n_functions).sum();
    doc.models[0].surface = Some(Figure {
        orbital: OrbitalRef {
            result: result_key(&full),
            spin: Spin::Alpha,
            index: 3,
        },
        iso: 0.05,
        look: Default::default(),
        snapshot: None,
    });
    add_result(&mut doc, &start, full.light(&[]));
    add_result(&mut doc, &start, full.light(&[]));
    let part = editing::selection(&doc, &[oxygen]);
    let pasted = editing::append(&mut doc, &part, Point::new(150., 0.))[0];
    doc.atom_mut(pasted).unwrap().isotope = 18;
    let (original_index, original) = figure_result(&doc, &doc.models[0]).unwrap();
    let (copy_index, copy) = figure_result(&doc, &doc.models[1]).unwrap();
    assert_eq!(original_index, 1);
    assert_eq!(copy_index, 3);
    assert_ne!(original.attachment, copy.attachment);
    assert_eq!(result_key(&original.result), result_key(&copy.result));
    assert!(figure_ready(&doc, &doc.models[0]));
    assert!(!figure_ready(&doc, &doc.models[1]));

    let row = vec![0.25; n_ao];
    pin_orbital(&mut doc, original_index, Spin::Alpha, 3, basis, &row).unwrap();
    let orbitals = |index: usize| doc.science.results[index].result.orbitals.as_ref().unwrap();
    assert_eq!(orbitals(original_index).alpha.rows, [3]);
    assert_eq!(orbitals(original_index).alpha.coefficients, row);
    assert!(orbitals(copy_index).alpha.rows.is_empty());
    doc.models[0].surface.as_mut().unwrap().orbital.result = "unknown".into();
    assert!(figure_result(&doc, &doc.models[0]).is_none());
}

#[test]
fn a_repeated_calculation_leaves_a_placed_figure_on_its_pinned_result() {
    use crate::models3d::surface::{Figure, OrbitalRef};

    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Energy, true);
    let basis = full.orbitals.as_ref().unwrap().basis.clone().unwrap();
    let n_ao = basis.shells.iter().map(Shell::n_functions).sum();
    doc.models[0].surface = Some(Figure {
        orbital: OrbitalRef {
            result: result_key(&full),
            spin: Spin::Alpha,
            index: 3,
        },
        iso: 0.05,
        look: Default::default(),
        snapshot: None,
    });
    add_result(&mut doc, &start, full.light(&[]));
    pin_orbital(&mut doc, 0, Spin::Alpha, 3, &basis, &vec![0.5; n_ao]).unwrap();
    // Recalculate: same frame and settings, so the same key, no rows.
    add_result(&mut doc, &start, full.light(&[]));
    assert_eq!(doc.science.results.len(), 2);
    let (index, _) = figure_result(&doc, &doc.models[0]).unwrap();
    assert_eq!(index, 0, "the figure keeps the result holding its row");
    // Without a pinned row anywhere, the newest result is used.
    doc.models[0].surface.as_mut().unwrap().orbital.index = 4;
    assert_eq!(figure_result(&doc, &doc.models[0]).unwrap().0, 1);
}

#[test]
fn pin_orbital_inserts_sorted_rows_and_keeps_the_result_key() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Energy, true);
    let basis = full.orbitals.as_ref().unwrap().basis.as_ref().unwrap();
    let n_ao: usize = basis.shells.iter().map(Shell::n_functions).sum();
    add_result(&mut doc, &start, full.light(&[]));
    let original = doc.science.results[0].result.clone();
    let key = result_key(&original);
    for index in [4, 1, 3] {
        let row = vec![f64::from(index) * -0.125; n_ao];
        pin_orbital(&mut doc, 0, Spin::Alpha, index, basis, &row).unwrap();
        assert_eq!(result_key(&doc.science.results[0].result), key);
    }
    let pinned = doc.science.results[0].result.clone();
    let orbitals = pinned.orbitals.as_ref().unwrap();
    assert_eq!(orbitals.basis.as_ref(), Some(basis));
    assert_eq!(orbitals.alpha.rows, [1, 3, 4]);
    for (&index, row) in orbitals
        .alpha
        .rows
        .iter()
        .zip(orbitals.alpha.coefficients.chunks_exact(n_ao))
    {
        assert_eq!(row, vec![f64::from(index) * -0.125; n_ao]);
    }
    assert!(original.orbitals.as_ref().unwrap().alpha.rows.is_empty());
    // A second calculation of an existing row cannot change its signs.
    pin_orbital(&mut doc, 0, Spin::Alpha, 3, basis, &[f64::NAN]).unwrap();
    assert!(Arc::ptr_eq(&pinned, &doc.science.results[0].result));
    doc.science.validate().unwrap();
}

#[test]
fn pin_orbital_rejects_invalid_rows_and_mismatched_bases_without_changes() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Energy, true);
    let basis = full.orbitals.as_ref().unwrap().basis.as_ref().unwrap();
    let n_ao: usize = basis.shells.iter().map(Shell::n_functions).sum();
    add_result(&mut doc, &start, full.light(&[]));
    let original = doc.science.results[0].result.clone();
    for (index, row) in [
        (0, vec![0.; n_ao - 1]),
        (n_ao as u32, vec![0.; n_ao]),
        (0, vec![f64::INFINITY; n_ao]),
    ] {
        assert!(matches!(
            pin_orbital(&mut doc, 0, Spin::Alpha, index, basis, &row),
            Err(ScienceError::Invalid(_))
        ));
        assert!(Arc::ptr_eq(&original, &doc.science.results[0].result));
    }
    let mut invalid_basis = basis.clone();
    invalid_basis.shells[0].atom = start.frame.numbers.len() as u32;
    assert!(matches!(
        pin_orbital(&mut doc, 0, Spin::Alpha, 0, &invalid_basis, &vec![0.; n_ao]),
        Err(ScienceError::Invalid(_))
    ));
    assert!(Arc::ptr_eq(&original, &doc.science.results[0].result));

    pin_orbital(&mut doc, 0, Spin::Alpha, 0, basis, &vec![0.25; n_ao]).unwrap();
    let pinned = doc.science.results[0].result.clone();
    let mut other_basis = basis.clone();
    other_basis.shells[0].exponents_per_bohr2[0] *= 2.;
    assert!(matches!(
        pin_orbital(&mut doc, 0, Spin::Alpha, 1, &other_basis, &vec![0.; n_ao]),
        Err(ScienceError::Invalid(_))
    ));
    assert!(Arc::ptr_eq(&pinned, &doc.science.results[0].result));
    assert!(matches!(
        pin_orbital(&mut doc, 1, Spin::Alpha, 0, basis, &vec![0.; n_ao]),
        Err(ScienceError::Unknown)
    ));
}

#[test]
fn pin_orbital_uses_separate_beta_rows_only_for_unrestricted_orbitals() {
    let (doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Energy, true);
    let basis = full.orbitals.as_ref().unwrap().basis.as_ref().unwrap();
    let n_ao: usize = basis.shells.iter().map(Shell::n_functions).sum();
    for unrestricted in [false, true] {
        let mut doc = doc.clone();
        let mut light = full.light(&[]);
        let orbitals = light.orbitals.as_mut().unwrap();
        orbitals.alpha.occupations.fill(1.);
        orbitals.beta = if unrestricted {
            Beta::Unrestricted(orbitals.alpha.clone())
        } else {
            Beta::SameOrbitals {
                occupations: vec![],
            }
        };
        add_result(&mut doc, &start, light);
        pin_orbital(&mut doc, 0, Spin::Beta, 2, basis, &vec![-0.5; n_ao]).unwrap();
        let orbitals = doc.science.results[0].result.orbitals.as_ref().unwrap();
        match &orbitals.beta {
            Beta::Unrestricted(set) => {
                assert_eq!(set.rows, [2]);
                assert!(orbitals.alpha.rows.is_empty());
            }
            _ => assert_eq!(orbitals.alpha.rows, [2]),
        }
        doc.science.validate().unwrap();
    }
}

#[test]
fn saved_with_pins_keeps_an_xtb_row_and_its_basis() {
    use crate::models3d::surface::{Figure, OrbitalRef};

    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Hessian, true);
    let original = Arc::new(full.clone());
    let key = result_key(&full);
    add_result(&mut doc, &start, full);
    let pins = HashMap::from([(key.clone(), vec![(Spin::Alpha, 4)])]);
    doc.models[0].surface = Some(Figure {
        orbital: OrbitalRef {
            result: key.clone(),
            spin: Spin::Alpha,
            index: 4,
        },
        iso: 0.05,
        look: Default::default(),
        snapshot: None,
    });
    let unpinned = doc.science.saved();
    let orbitals = unpinned.results[0].result.orbitals.as_ref().unwrap();
    assert!(orbitals.basis.is_none());
    assert!(orbitals.alpha.rows.is_empty());

    let saved = doc.science.saved_with_pins(&pins);
    assert_eq!(doc.current().science, saved);
    let result = &saved.results[0].result;
    let orbitals = result.orbitals.as_ref().unwrap();
    let full = original.orbitals.as_ref().unwrap();
    let n_ao: usize = full
        .basis
        .as_ref()
        .unwrap()
        .shells
        .iter()
        .map(Shell::n_functions)
        .sum();
    assert_eq!(orbitals.basis, full.basis);
    assert_eq!(orbitals.alpha.rows, [4]);
    assert_eq!(
        orbitals.alpha.coefficients,
        full.alpha.coefficients[4 * n_ao..5 * n_ao]
    );
    assert!(result.hessian_hartree_per_bohr2.is_empty());
    assert_eq!(result_key(result), key);
    result.validate(&start.frame).unwrap();
    let json = serde_json::to_vec_pretty(&saved).unwrap();
    let reopened: Science = serde_json::from_slice(&json).unwrap();
    assert_eq!(reopened, saved);
    assert_eq!(doc.science.file_bytes_with_pins(&pins), json.len());
    assert_eq!(
        doc.science.saved_sizes_with_pins(&pins),
        vec![serde_json::to_vec_pretty(&saved.results[0]).unwrap().len()]
    );
    assert_eq!(doc.science.results[0].result.as_ref(), original.as_ref());
    doc.science.results[0].keep_full = true;
    assert_eq!(make_lighter(&mut doc), 1);
    assert_eq!(doc.science, saved);
}

#[test]
fn launch_maps_model_atoms_then_hydrogens() {
    let (doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    assert_eq!(start.frame.numbers, [8, 1, 1]);
    assert_eq!(start.atoms, [Some(oxygen), None, None]);
    let model = &doc.models[0];
    assert_eq!(
        start.frame.positions_angstrom,
        model
            .atoms
            .iter()
            .map(|a| a.xyz.map(f64::from))
            .chain(model.hydrogens.iter().map(|h| h.xyz.map(f64::from)))
            .collect::<Vec<_>>()
    );
    assert_eq!(drawn_state(&doc, oxygen), (0, 1));

    let (mut doc, ids) = ethanol();
    let start = launch(&doc, ids[0], 0, 1).unwrap();
    assert_eq!(start.frame.numbers, [6, 6, 8, 1, 1]);
    assert_eq!(
        start.atoms,
        [Some(ids[0]), Some(ids[1]), Some(ids[2]), None, None]
    );
    reconciled_edit(&mut doc, &mut History::default(), |doc| {
        let atom = doc.atom_mut(ids[2]).unwrap();
        atom.charge = 1;
        atom.radical_electrons = 1;
    });
    assert_eq!(drawn_state(&doc, ids[0]), (1, 2));
    reconciled_edit(&mut doc, &mut History::default(), |doc| {
        let atom = doc.atom_mut(ids[2]).unwrap();
        atom.radical_electrons = 2;
        atom.singlet = true;
    });
    assert_eq!(drawn_state(&doc, ids[0]), (1, 1));
}

#[test]
fn drawn_state_saturates_charges() {
    let mut doc = Document::default();
    let a = doc.add_atom("C", Point::default());
    let b = doc.add_atom("C", Point::new(36., 0.));
    doc.add_bond(a, b, 1, "plain");
    doc.atom_mut(a).unwrap().charge = i32::MAX;
    doc.atom_mut(b).unwrap().charge = 1;
    assert_eq!(drawn_state(&doc, a), (i32::MAX, 1));
}

#[test]
fn launch_refuses_without_model_stale_model_or_crystal() {
    let (doc, oxygen) = water();
    let mut no_model = doc.clone();
    reconciled_edit(&mut no_model, &mut History::default(), |doc| {
        doc.models.clear()
    });
    assert!(matches!(
        launch(&no_model, oxygen, 0, 1),
        Err(ScienceError::NoModel)
    ));
    let mut stale = doc.clone();
    reconciled_edit(&mut stale, &mut History::default(), |doc| {
        doc.atom_mut(oxygen).unwrap().element = "S".into();
    });
    assert!(matches!(
        launch(&stale, oxygen, 0, 1),
        Err(ScienceError::StaleModel)
    ));
    let mut crystal = doc;
    reconciled_edit(&mut crystal, &mut History::default(), |doc| {
        doc.models[0] = doc.models[0].clone().with_cell(
            doc,
            Some(Cell {
                vectors: [[10., 0., 0.], [0., 10., 0.], [0., 0., 10.]],
                crossing: vec![],
            }),
            vec![],
        );
    });
    assert!(matches!(
        launch(&crystal, oxygen, 0, 1),
        Err(ScienceError::Periodic)
    ));
}

#[test]
fn attach_stores_frames_results_and_attachments_once() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let mut final_frame = (*start.frame).clone();
    final_frame.positions_angstrom[0][0] += 0.012_345_678_9;
    let mut optimized = result(&final_frame, Task::Optimize, false);
    optimized.input_frame = Some(start.frame.hash());
    let mut ids = Vec::new();
    let mut history = History::default();
    for energy in [-7.5, -7.6] {
        let mut optimized = optimized.clone();
        optimized.energy_hartree = Some(energy);
        reconciled_edit(&mut doc, &mut history, |doc| {
            let attached = attach(doc, &start, vec![final_frame.clone()], optimized).unwrap();
            assert_eq!(attached.freshness, Freshness::Current);
            ids.push(attached.attachment);
        });
    }
    assert_eq!(ids[0], ids[1]);
    assert_eq!(doc.science.frames.len(), 2);
    assert_eq!(doc.science.results.len(), 2);
    assert_eq!(doc.science.attachments.len(), 1);
    assert!(Arc::ptr_eq(
        doc.science.frame(&start.frame.hash()).unwrap(),
        &start.frame
    ));
    assert_eq!(
        doc.science.frame(&final_frame.hash()).unwrap().as_ref(),
        &final_frame
    );
    for (stored, energy) in doc.science.results.iter().zip([-7.5, -7.6]) {
        assert_eq!(stored.attachment, ids[0]);
        assert_eq!(stored.result.energy_hartree, Some(energy));
    }
    assert_eq!(
        doc.science.attachment(ids[0]).unwrap().frame,
        start.frame.hash()
    );
    assert_eq!(results_for(&doc, oxygen).len(), 2);
    assert_eq!(history.frames(), 2);
}

#[test]
fn identical_geometries_keep_their_own_results() {
    let (mut doc, [a, b]) = water_pair();
    let first = launch(&doc, a, 0, 1).unwrap();
    let second = launch(&doc, b, 0, 1).unwrap();
    assert_eq!(first.frame.hash(), second.frame.hash());

    let one = add_result(&mut doc, &first, result(&first.frame, Task::Energy, false));
    let mut other = result(&second.frame, Task::Energy, false);
    other.energy_hartree = Some(-8.);
    let two = add_result(&mut doc, &second, other);
    assert_ne!(one.attachment, two.attachment);
    assert_eq!(doc.science.frames.len(), 1);

    for (atom, index, attachment, energy) in
        [(a, 0, one.attachment, -7.5), (b, 1, two.attachment, -8.)]
    {
        let views = results_for(&doc, atom);
        let [view] = views.as_slice() else {
            panic!("expected one result for atom {atom}, found {}", views.len());
        };
        assert_eq!(view.index, index);
        assert_eq!(view.attachment.id, attachment);
        assert_eq!(view.stored.result.energy_hartree, Some(energy));
    }

    let surviving = doc.science.results[1].clone();
    reconciled_edit(&mut doc, &mut History::default(), |doc| doc.delete(&[a]));
    assert!(results_for(&doc, a).is_empty());
    assert_eq!(doc.science.results, [surviving]);
    let views = results_for(&doc, b);
    assert_eq!(views.len(), 1);
    assert_eq!(views[0].index, 0);
    assert_eq!(views[0].attachment.id, two.attachment);
    assert_eq!(views[0].freshness, Freshness::Current);
}

#[test]
fn attachment_ids_reuse_the_smallest_free_id() {
    let (mut doc, [a, b]) = water_pair();
    let start = launch(&doc, a, 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    let mut saved: Value = serde_json::from_slice(&doc.file_json().unwrap()).unwrap();
    saved["science"]["attachments"][0]["id"] = json!(u64::MAX);
    saved["science"]["results"][0]["attachment"] = json!(u64::MAX);
    let mut reopened = Document::from_native_file(&serde_json::to_vec(&saved).unwrap()).unwrap();
    assert_eq!(results_for(&reopened, a)[0].attachment.id, u64::MAX);

    let start = launch(&reopened, b, 0, 1).unwrap();
    let added = add_result(
        &mut reopened,
        &start,
        result(&start.frame, Task::Energy, false),
    );
    assert_eq!(added.attachment, 1);
    let reopened = Document::from_native_file(&reopened.file_json().unwrap()).unwrap();
    assert_eq!(results_for(&reopened, a)[0].attachment.id, u64::MAX);
    assert_eq!(results_for(&reopened, b)[0].attachment.id, 1);
}

#[test]
fn save_and_reopen_keeps_results() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Hessian, true);
    add_result(&mut doc, &start, full.clone());
    let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.version, CALCULATIONS);
    assert_eq!(reopened.science.frames, doc.science.frames);
    assert_eq!(reopened.science.attachments, doc.science.attachments);
    let light = &reopened.science.results[0].result;
    assert_eq!(light.as_ref(), &full.light(&[]));
    assert!(light.vibrations.as_ref().unwrap().modes.is_empty());
    assert!(
        light
            .orbitals
            .as_ref()
            .unwrap()
            .alpha
            .coefficients
            .is_empty()
    );

    let small_import = imported(full);
    assert!(small_import.heavy_bytes() <= MAX_IMPORTED_HEAVY_BYTES);
    add_result(&mut doc, &start, small_import.clone());
    let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
    assert_eq!(reopened.science.results[1].result.as_ref(), &small_import);
    assert_eq!(reopened.science.attachments, doc.science.attachments);

    let (mut large, ids) = chain(120);
    let start = launch(&large, ids[0], 0, 1).unwrap();
    let mut large_import = imported(result(&start.frame, Task::Hessian, true));
    // Repetitive data triggers the codec's stored fallback, crossing the
    // imported budget without several slow passes over dense random arrays.
    large_import.hessian_hartree_per_bohr2.fill(0.25);
    large_import
        .orbitals
        .as_mut()
        .unwrap()
        .alpha
        .coefficients
        .fill(0.25);
    for mode in large_import
        .vibrations
        .as_mut()
        .unwrap()
        .modes
        .chunks_exact_mut(3 * ids.len())
    {
        mode.fill(0.);
        mode[0] = 1.;
    }
    let heavy_bytes = large_import.heavy_bytes();
    assert!(heavy_bytes > MAX_IMPORTED_HEAVY_BYTES);
    add_result(&mut large, &start, large_import.clone());
    let reopened = Document::from_native_file(&large.file_json().unwrap()).unwrap();
    let mut light = large_import.light(&[]);
    light.omitted_bytes = Some(heavy_bytes);
    assert_eq!(reopened.science.results[0].result.as_ref(), &light);
    reconciled_edit(&mut large, &mut History::default(), |doc| {
        assert!(set_keep_full(doc, 0, true));
    });
    let reopened = Document::from_native_file(&large.file_json().unwrap()).unwrap();
    let stored = &reopened.science.results[0];
    assert!(stored.keep_full);
    assert_eq!(stored.result.as_ref(), &large_import);
}

#[test]
fn internal_round_trip_keeps_full_data() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Hessian, true);
    add_result(&mut doc, &start, full.clone());
    assert!(!doc.science.results[0].keep_full);

    let internal: Document = serde_json::from_slice(&serde_json::to_vec(&doc).unwrap()).unwrap();
    // The fixture already uses exactly representable f32 and q16 array values.
    assert_eq!(internal.science.results, doc.science.results);
    assert_eq!(internal.science.results[0].result.as_ref(), &full);

    let native = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
    assert_eq!(native.science.results[0].result.as_ref(), &full.light(&[]));
}

#[test]
fn keep_full_toggle_shares_the_payload() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Hessian, true));
    let old = doc.science.results[0].result.clone();

    assert!(set_keep_full(&mut doc, 0, true));
    assert!(Arc::ptr_eq(&old, &doc.science.results[0].result));
    let bytes = doc.file_json().unwrap();
    let saved: Value = serde_json::from_slice(&bytes).unwrap();
    let stored = &saved["science"]["results"][0];
    assert_eq!(stored["keep_full"], true);
    assert!(
        stored["result"]["hessian_hartree_per_bohr2"]
            .as_str()
            .unwrap()
            .starts_with("f32:")
    );

    let reopened = Document::from_native_file(&bytes).unwrap();
    let stored = &reopened.science.results[0];
    assert!(stored.keep_full);
    assert!(!stored.result.keep_full);
    assert_eq!(stored.result, old);
}

#[test]
fn save_without_results_keeps_legacy_version() {
    let (doc, _) = water();
    let legacy = doc.file_version();
    assert!(legacy < CALCULATIONS);
    let json: Value = serde_json::from_slice(&doc.file_json().unwrap()).unwrap();
    assert_eq!(json["version"], legacy);
    assert!(json.get("science").is_none());
    let reopened = Document::from_native_file(&serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(reopened.file_version(), legacy);
    assert!(reopened.science.is_empty());
    let drawing = Document::default();
    let reopened = Document::from_native_file(&drawing.file_json().unwrap()).unwrap();
    assert_eq!(reopened.file_version(), drawing.file_version());
}

#[test]
fn older_reader_refuses_result_files() {
    fn read_with_version(bytes: &[u8], maximum: u32) -> Result<Document, String> {
        #[derive(Deserialize)]
        struct Probe {
            version: Option<u64>,
        }
        if let Ok(Probe {
            version: Some(version),
        }) = serde_json::from_slice(bytes)
            && version > u64::from(maximum)
        {
            return Err(format!(
                "This drawing was made with a newer version of ReShiki (document version {version}). Update ReShiki to open it."
            ));
        }
        Document::from_json(bytes)
    }
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    let bytes = doc.file_json().unwrap();
    assert_eq!(VERSION, 42);
    assert_eq!(Document::from_json(&bytes).unwrap().version, 42);
    assert_eq!(read_with_version(&bytes, VERSION).unwrap().version, 42);
    assert_eq!(
        read_with_version(&bytes, VERSION - 1).unwrap_err(),
        "This drawing was made with a newer version of ReShiki (document version 42). Update ReShiki to open it."
    );
    let mut newer: Value = serde_json::from_slice(&bytes).unwrap();
    newer["version"] = json!(43);
    assert_eq!(
        Document::from_json(&serde_json::to_vec(&newer).unwrap()).unwrap_err(),
        "This drawing was made with a newer version of ReShiki (document version 43). Update ReShiki to open it."
    );
}

#[test]
fn newer_version_is_refused_before_reading_science() {
    let mut newer = serde_json::to_value(Document::default()).unwrap();
    newer["version"] = json!(43);
    newer["science"] = json!({"results": "invalid"});
    assert_eq!(
        Document::from_native_file(&serde_json::to_vec(&newer).unwrap()).unwrap_err(),
        "This drawing was made with a newer version of ReShiki (document version 43). Update ReShiki to open it."
    );
    newer["version"] = json!(VERSION);
    let error = Document::from_native_file(&serde_json::to_vec(&newer).unwrap()).unwrap_err();
    assert!(error.contains("a list of calculation records"), "{error}");
}

#[test]
fn native_file_excerpt_uses_compact_arrays() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let mut full = result(&start.frame, Task::Hessian, true);
    full.keep_full = true;
    add_result(&mut doc, &start, full);
    let bytes = doc.file_json().unwrap();
    let text = std::str::from_utf8(&bytes).unwrap();
    assert!(text.contains("\"modes\": \"q16:"));
    assert!(text.contains("\"coefficients\": \"f32:"));
    assert!(!text.contains("\"modes\": ["));
    let json: Value = serde_json::from_slice(&bytes).unwrap();
    let saved = &json["science"]["results"][0]["result"];
    assert!(
        saved["vibrations"]["modes"]
            .as_str()
            .unwrap()
            .starts_with("q16:")
    );
    assert!(
        saved["orbitals"]["alpha"]["coefficients"]
            .as_str()
            .unwrap()
            .starts_with("f32:")
    );
    assert!(
        saved["hessian_hartree_per_bohr2"]
            .as_str()
            .unwrap()
            .starts_with("f32:")
    );
}

#[test]
fn chemical_edit_makes_results_stale_and_undo_restores() {
    let (mut doc, ids) = ethanol();
    let start = launch(&doc, ids[0], 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    assert_eq!(results_for(&doc, ids[0])[0].freshness, Freshness::Current);
    let mut history = History::default();
    reconciled_edit(&mut doc, &mut history, |doc| {
        doc.atom_mut(ids[2]).unwrap().element = "S".into();
    });
    assert_eq!(results_for(&doc, ids[0])[0].freshness, Freshness::Stale);
    assert!(history.undo(&mut doc));
    assert_eq!(results_for(&doc, ids[0])[0].freshness, Freshness::Current);
    assert!(history.redo(&mut doc));
    assert_eq!(results_for(&doc, ids[0])[0].freshness, Freshness::Stale);
    assert!(history.undo(&mut doc));
    reconciled_edit(&mut doc, &mut history, |doc| doc.translate(&ids, 30., -20.));
    assert_eq!(results_for(&doc, ids[0])[0].freshness, Freshness::Current);
}

#[test]
fn singlet_toggle_makes_results_stale() {
    let (mut doc, ids) = ethanol();
    let atom = ids[0];
    doc.atom_mut(atom).unwrap().radical_electrons = 2;
    doc.models[0].fingerprint = Some(crate::models3d::fingerprint(&doc, &ids));
    let (charge, multiplicity) = drawn_state(&doc, atom);
    let start = launch(&doc, atom, charge, multiplicity).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    assert_eq!(results_for(&doc, atom)[0].freshness, Freshness::Current);

    let mut history = History::default();
    reconciled_edit(&mut doc, &mut history, |doc| {
        doc.atom_mut(atom).unwrap().singlet = true;
    });
    assert_eq!(results_for(&doc, atom)[0].freshness, Freshness::Stale);
    assert!(history.undo(&mut doc));
    assert_eq!(results_for(&doc, atom)[0].freshness, Freshness::Current);
}

#[test]
fn late_result_after_edit_arrives_stale() {
    let (mut doc, ids) = ethanol();
    let start = launch(&doc, ids[0], 0, 1).unwrap();
    let finished = result(&start.frame, Task::Energy, false);
    let mut history = History::default();
    reconciled_edit(&mut doc, &mut history, |doc| {
        doc.atom_mut(ids[2]).unwrap().element = "S".into();
    });
    let mut edited = doc.clone();
    assert_eq!(
        add_result(&mut edited, &start, finished.clone()).freshness,
        Freshness::Stale
    );
    assert!(history.undo(&mut doc));
    assert_eq!(
        add_result(&mut doc, &start, finished.clone()).freshness,
        Freshness::Current
    );
    reconciled_edit(&mut doc, &mut history, |doc| doc.delete(&ids));
    let before = doc.clone();
    assert!(matches!(
        attach(&mut doc, &start, vec![], finished),
        Err(ScienceError::Gone)
    ));
    assert_eq!(doc, before);
}

#[test]
fn deleting_atoms_unlinks_and_reused_id_never_relinks() {
    let (mut doc, ids) = ethanol();
    let legacy = doc.file_version();
    let start = launch(&doc, ids[0], 0, 1).unwrap();
    let attached = add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    let mut history = History::default();
    reconciled_edit(&mut doc, &mut history, |doc| doc.delete(&[ids[2]]));
    assert_eq!(
        doc.science.attachment(attached.attachment).unwrap().atoms,
        [Some(ids[0]), Some(ids[1]), None, None, None]
    );
    assert_eq!(results_for(&doc, ids[0])[0].freshness, Freshness::Stale);
    assert_eq!(doc.next_id(), ids[2]);
    reconciled_edit(&mut doc, &mut history, |doc| {
        let replacement = doc.add_atom("O", Point::new(72., 0.));
        assert_eq!(replacement, ids[2]);
        doc.add_bond(ids[1], replacement, 1, "plain");
    });
    let link = doc.science.attachment(attached.attachment).unwrap();
    assert_eq!(link.atoms[2], None);
    assert_eq!(link.freshness(&doc), Freshness::Stale);
    assert_eq!(results_for(&doc, ids[2])[0].freshness, Freshness::Stale);
    reconciled_edit(&mut doc, &mut history, |doc| doc.delete(&ids));
    assert!(doc.science.is_empty());
    assert_eq!(doc.file_version(), Document::default().file_version());
    assert!(doc.file_version() <= legacy);
}

#[test]
fn history_counts_shared_payloads_once() {
    let (mut doc, ids) = chain(100);
    let start = launch(&doc, ids[0], 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Hessian, true));
    let heap = start.frame.heap_bytes() + doc.science.results[0].result.heap_bytes();
    assert!(heap > 2_000_000);
    let mut plain = doc.clone();
    plain.science = Science::default();
    let container = doc.science.container_bytes() - plain.science.container_bytes();
    let mut history = History::with_limit(40);
    let mut plain_history = History::with_limit(40);
    for _ in 0..30 {
        reconciled_edit(&mut doc, &mut history, |doc| doc.translate(&ids, 1., 0.));
        reconciled_edit(&mut plain, &mut plain_history, |doc| {
            doc.translate(&ids, 1., 0.)
        });
    }
    assert_eq!(history.frames(), 30);
    assert_eq!(
        history.bytes(),
        plain_history.bytes() + 30 * container + heap
    );
    assert!(history.bytes() < heap * 3);
    // The shared payloads once, plus the lists each of 30 snapshots and the
    // live document own.
    let lists = doc.science.container_bytes();
    assert_eq!(history.science_bytes(&doc), heap + 31 * lists);
    assert!(history.undo_frames().all(|snapshot| Arc::ptr_eq(
        &snapshot.science.results[0].result,
        &doc.science.results[0].result,
    )));
    // MAX_SCIENCE_BYTES is fixed: verify the exact accounting used by fit_science
    // instead of allocating hundreds of MiB solely to cross its production limit.
    assert!(history.fit_science(&doc));
    assert_eq!(history.frames(), 30);
    let mut separate = doc.clone();
    separate.science.results[0].result = Arc::new((*doc.science.results[0].result).clone());
    assert_eq!(
        history.science_bytes(&separate),
        heap + doc.science.results[0].result.heap_bytes() + 31 * lists
    );
    let bytes = history.bytes();
    assert_eq!(history.release(usize::MAX, 0), bytes);
    assert_eq!(history.frames(), 0);
    assert_eq!(history.bytes(), 0);
    assert_eq!(history.science_bytes(&doc), heap + lists);
    assert_eq!(history.science_bytes(&plain), 0);
}

#[test]
fn history_commit_keeps_science_within_budget() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Hessian, true);
    let bytes = full.heap_bytes() + start.frame.heap_bytes();
    add_result(&mut doc, &start, full);
    let mut history = History::default();
    let lists = doc.science.container_bytes();
    for step in 1..=3 {
        let before = doc.clone();
        doc.translate(&[oxygen], 1., 0.);
        assert!(history.commit(before, &doc));
        assert_eq!(history.science_bytes(&doc), bytes + (step + 1) * lists);
    }
    assert!(history.fit_science(&doc));
    assert_eq!(history.frames(), 3);
}

#[test]
fn combined_documents_over_budget_are_refused() {
    let (mut doc, ids) = chain(50);
    let start = launch(&doc, ids[0], 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Hessian, true));
    doc.validate().unwrap();
    let shared = doc.science.results[0].clone();
    let count = MAX_SCIENCE_BYTES / shared.result.heap_bytes() + 1;
    assert!(count + doc.science.attachments.len() + doc.science.frames.len() < MAX_SCIENCE_RECORDS);
    doc.science.results.resize(count, shared);
    assert!(doc.science.heap_bytes() > MAX_SCIENCE_BYTES);
    assert_eq!(
        doc.validate().unwrap_err(),
        ScienceError::TooLarge.to_string()
    );

    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    let mut file = serde_json::to_value(&doc).unwrap();
    let attachment = &doc.science.attachments[0];
    // One frame and these attachments fill the record budget. The first
    // result exceeds it, before the decoder reaches the malformed tail.
    file["science"]["attachments"] = json!(
        (1..MAX_SCIENCE_RECORDS)
            .map(|id| Attachment {
                id: id as u64,
                ..attachment.clone()
            })
            .collect::<Vec<_>>()
    );
    file["science"]["results"]
        .as_array_mut()
        .unwrap()
        .push(Value::Null);
    let error = Document::from_native_file(&serde_json::to_vec(&file).unwrap()).unwrap_err();
    assert!(error.contains("exceed"), "{error}");
}

#[test]
fn clipboard_selection_carries_no_science() {
    let (mut source, ids) = ethanol();
    let start = launch(&source, ids[0], 0, 1).unwrap();
    add_result(
        &mut source,
        &start,
        result(&start.frame, Task::Energy, false),
    );
    assert!(
        editing::selection(&source, &source.all_ids())
            .science
            .is_empty()
    );
    let (mut target, oxygen) = water();
    let mut inserted = vec![];
    reconciled_edit(&mut target, &mut History::default(), |doc| {
        inserted = editing::append(doc, &source, Point::new(150., 0.));
    });
    assert_eq!(inserted.len(), 3);
    assert!(!inserted.contains(&oxygen));
    let attachment = &target.science.attachments[0];
    assert_eq!(
        attachment.atoms,
        [
            Some(inserted[0]),
            Some(inserted[1]),
            Some(inserted[2]),
            None,
            None
        ]
    );
    assert_eq!(attachment.freshness(&target), Freshness::Current);
    assert_eq!(
        results_for(&target, inserted[0])[0].freshness,
        Freshness::Current
    );
    assert!(Arc::ptr_eq(
        &target.science.results[0].result,
        &source.science.results[0].result
    ));
    assert!(Arc::ptr_eq(
        &target.science.frames[0],
        &source.science.frames[0]
    ));
    let mut duplicated = source.clone();
    reconciled_edit(&mut duplicated, &mut History::default(), |doc| {
        assert_eq!(editing::append(doc, &source, Point::new(150., 0.)).len(), 3);
    });
    assert!(Arc::ptr_eq(
        &duplicated.science.results[0].result,
        &duplicated.science.results[1].result
    ));
    assert_eq!(
        History::default().science_bytes(&duplicated),
        start.frame.heap_bytes()
            + source.science.results[0].result.heap_bytes()
            + duplicated.science.container_bytes()
    );
}

fn transition_state_drawing() -> (Document, u64) {
    use crate::{arrows::ArrowStyle, document::Arrow};
    use reshiki_qcdata::{
        Endpoints, RunSummary, Trajectory, TransitionStateContext, TransitionStateMapping,
        TransitionStateRun,
    };

    let (mut doc, oxygen) = water();
    let arrow = doc.next_id();
    doc.arrows.push(Arrow::new(
        arrow,
        Point::new(80., 0.),
        Point::new(160., 0.),
        Default::default(),
        ArrowStyle::default(),
    ));
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let mut finished = result(&start.frame, Task::Energy, false);
    add_result(&mut doc, &start, finished.clone());
    finished.task = Task::TransitionState;
    finished.endpoints = Some(Endpoints {
        reactant: start.frame.hash(),
        product: start.frame.hash(),
    });
    finished.trajectory = Some(Trajectory {
        rmsd_to_best_angstrom: Vec::new(),
        numbers: start.frame.numbers.clone(),
        isotopes: start.frame.isotopes.clone(),
        masses_amu: start.frame.masses_amu.clone(),
        charge: start.frame.charge,
        multiplicity: start.frame.multiplicity,
        positions_angstrom: vec![],
        energies_hartree: vec![-7.5, -7.4, -7.6],
        labels: vec![],
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
    });
    finished.run = Some(RunSummary {
        seconds: 1.,
        scf_iterations: vec![],
        finished: None,
        search: None,
        transition_state: Some(Box::new(TransitionStateRun {
            arrow: Some(arrow),
            free_energy: false,
            endpoint_energies_hartree: None,
            endpoint_thermo: None,
            reached_positions_angstrom: vec![],
            context: Some(TransitionStateContext {
                reactant_chemistry: "a".repeat(64),
                product_chemistry: "b".repeat(64),
                title: "Saved water reaction".into(),
                reactant: "water".into(),
                endpoint_complex: None,
            }),
            saddle_converged: false,
            imaginary: 0,
            imaginary_cm1: None,
            climbing_image: 1,
            mapping: TransitionStateMapping {
                formed: vec![],
                broken: vec![],
                hydrogen_moves: vec![],
                automatic: true,
            },
            diagnostics: vec![],
        })),
    });
    add_result(&mut doc, &start, finished);
    (doc, arrow)
}

fn saved_ts(doc: &Document) -> &reshiki_qcdata::TransitionStateRun {
    doc.science.results[1]
        .result
        .run
        .as_ref()
        .unwrap()
        .transition_state
        .as_ref()
        .unwrap()
}

#[test]
fn native_insertion_remaps_saved_ts_arrow_and_unbinds_missing_arrow() {
    let (source, arrow) = transition_state_drawing();
    let source = Document::from_native_file(&source.file_json().unwrap()).unwrap();
    let host_arrow = source.arrows[0].clone();
    type Append = fn(&mut Document, &Document, Point) -> Vec<u64>;
    let appenders: [Append; 2] = [editing::append, editing::append_preserving_layout];
    for insert in appenders {
        for include_arrow in [true, false] {
            let mut source = source.clone();
            if !include_arrow {
                source.arrows.clear();
            }
            // The destination already owns the source arrow's old ID.
            let (mut doc, _) = water();
            doc.arrows.push(host_arrow.clone());
            assert_eq!(doc.arrows[0].id, arrow);
            let ids = insert(&mut doc, &source, Point::new(250., 0.));
            assert_eq!(ids.len(), 1 + usize::from(include_arrow));
            assert_eq!(doc.science.attachments[0].atoms[0], Some(ids[0]));
            let expected = include_arrow.then(|| ids[1]);
            assert_eq!(saved_ts(&doc).arrow, expected);
            assert_eq!(saved_ts(&doc).context, saved_ts(&source).context);
            assert_eq!(saved_ts(&source).arrow, Some(arrow));
            assert!(Arc::ptr_eq(
                &doc.science.results[0].result,
                &source.science.results[0].result
            ));
            doc.validate().unwrap();
            let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
            assert_eq!(saved_ts(&reopened).arrow, expected);
            assert_eq!(saved_ts(&reopened).context, saved_ts(&source).context);
        }
    }
}

#[test]
fn deleting_ts_arrow_unbinds_saved_result_before_id_reuse() {
    let (mut doc, arrow) = transition_state_drawing();
    let original = doc.clone();
    let mut history = History::default();
    reconciled_edit(&mut doc, &mut history, |doc| doc.delete(&[arrow]));
    assert_eq!(saved_ts(&doc).arrow, None);
    assert_eq!(saved_ts(&original).arrow, Some(arrow));
    assert!(Arc::ptr_eq(
        &doc.science.results[0].result,
        &original.science.results[0].result
    ));
    assert_eq!(doc.next_id(), arrow);
    reconciled_edit(&mut doc, &mut history, |doc| {
        doc.arrows.push(original.arrows[0].clone());
    });
    assert_eq!(saved_ts(&doc).arrow, None);
    let reopened = Document::from_native_file(&doc.file_json().unwrap()).unwrap();
    assert_eq!(saved_ts(&reopened).arrow, None);
    assert!(history.undo(&mut doc));
    assert!(history.undo(&mut doc));
    assert_eq!(saved_ts(&doc).arrow, Some(arrow));

    let mut orphaned = original.clone();
    orphaned.arrows.clear();
    let reopened = Document::from_native_file(&orphaned.file_json().unwrap()).unwrap();
    assert_eq!(saved_ts(&reopened).arrow, None);
}

#[test]
fn clipboard_figures_keep_only_their_linked_results_and_pinned_rows() {
    use crate::models3d::surface::{Figure, OrbitalRef};

    let (mut source, atoms) = water_pair();
    let first = launch(&source, atoms[0], 0, 1).unwrap();
    let full = result(&first.frame, Task::Energy, true);
    let key = result_key(&full);
    add_result(&mut source, &first, full.clone());
    source.science.results[0].keep_full = true;
    let second = launch(&source, atoms[1], 0, 1).unwrap();
    add_result(&mut source, &second, full);
    assert_eq!(result_key(&source.science.results[1].result), key);
    let mut optimized_frame = (*first.frame).clone();
    optimized_frame.positions_angstrom[0][0] += 0.1;
    let mut optimized = result(&optimized_frame, Task::Optimize, false);
    optimized.input_frame = Some(first.frame.hash());
    attach(&mut source, &first, vec![optimized_frame], optimized).unwrap();
    source.models[0].surface = Some(Figure {
        orbital: OrbitalRef {
            result: key.clone(),
            spin: Spin::Alpha,
            index: 4,
        },
        iso: 0.05,
        look: Default::default(),
        snapshot: None,
    });
    assert_eq!(source.science.results.len(), 3);
    assert_eq!(source.science.frames.len(), 2);

    let science = for_figures(&source);
    assert_eq!(science.results.len(), 1);
    assert_eq!(science.attachments.len(), 1);
    assert_eq!(science.frames.len(), 1);
    let stored = &science.results[0];
    assert!(!stored.keep_full);
    assert_eq!(stored.attachment, source.science.results[0].attachment);
    assert_eq!(result_key(&stored.result), key);
    let orbitals = stored.result.orbitals.as_ref().unwrap();
    assert!(orbitals.basis.is_some());
    assert_eq!(orbitals.alpha.rows, [4]);
    assert!(stored.result.hessian_hartree_per_bohr2.is_empty());
    assert!(stored.result.gradient_hartree_per_bohr.is_empty());
    science.validate().unwrap();

    let part = editing::selection(&source, &[atoms[0]]);
    assert_eq!(part.models.len(), 1);
    assert_eq!(part.science, science);
    part.validate().unwrap();
    assert!(editing::selection(&source, &[atoms[1]]).science.is_empty());
    assert_eq!(source.science.results.len(), 3);
    assert!(source.science.results[0].keep_full);
}

#[test]
fn remapping_ids_refreshes_only_matching_surface_snapshot_signatures() {
    use crate::models3d::surface::{Figure, OrbitalRef, Snapshot, draw::view_signature};

    let (mut original, oxygen) = water();
    let start = launch(&original, oxygen, 0, 1).unwrap();
    let result = result(&start.frame, Task::Energy, false);
    let key = result_key(&result);
    add_result(&mut original, &start, result);
    original.models[0].surface = Some(Figure {
        orbital: OrbitalRef {
            result: key,
            spin: Spin::Alpha,
            index: 0,
        },
        iso: 0.05,
        look: Default::default(),
        snapshot: None,
    });
    let view = view_signature(&original, &original.models[0]);
    original.models[0].surface.as_mut().unwrap().snapshot = Some(Snapshot {
        view: view.clone(),
        reach: 3.,
        shapes: "saved vector surface".into(),
    });

    for matches in [true, false] {
        let mut doc = original.clone();
        if !matches {
            doc.models[0].size += 10.;
        }
        let mapping = doc.all_ids().into_iter().map(|id| (id, id + 100)).collect();
        editing::remap_ids(&mut doc, &mapping).unwrap();
        let signature = view_signature(&doc, &doc.models[0]);
        let snapshot = doc.models[0]
            .surface
            .as_ref()
            .unwrap()
            .snapshot
            .as_ref()
            .unwrap();
        assert_ne!(signature, view);
        if matches {
            assert_eq!(snapshot.view, signature);
        } else {
            assert_eq!(snapshot.view, view);
            assert_ne!(snapshot.view, signature);
        }
        assert_eq!(snapshot.reach, 3.);
        assert_eq!(snapshot.shapes, "saved vector surface");
        doc.validate().unwrap();
    }
}

#[test]
fn calculated_fragment_can_still_be_joined() {
    use crate::{
        joining::Prepared,
        templates::{Anchor, Connection},
    };

    let (mut doc, fragment) = chain(2);
    let host_point = Point::new(180., 0.);
    let host = doc.add_atom("C", host_point);
    let neighbor = doc.add_atom("C", Point::new(222., 0.));
    doc.add_bond(host, neighbor, 1, "plain");
    let start = launch(&doc, fragment[0], 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    assert_eq!(
        results_for(&doc, fragment[0])[0].freshness,
        Freshness::Current
    );

    let (mut joined, _) = Prepared::new(&doc, &fragment)
        .unwrap()
        .place(
            host_point,
            None,
            5.,
            Anchor::Atom(fragment[0]),
            Connection::Connect,
        )
        .unwrap();
    assert!(joined.bond_position(host, fragment[0]).is_some());
    assert!(
        transaction::reconcile(&mut joined, doc)
            .unwrap()
            .commit(&mut joined, &mut History::default(), false)
            .recorded
    );
    assert!(joined.science.is_empty());
    assert!(results_for(&joined, fragment[0]).is_empty());
    joined.validate().unwrap();
}

#[test]
fn expansion_of_another_molecule_ignores_unrelated_results() {
    use crate::{atom_text, generic};

    let (mut doc, oxygen) = water();
    let carbon = doc.add_atom("C", Point::new(180., 0.));
    let alternative = doc.add_atom("*", Point::new(222., 0.));
    doc.add_bond(carbon, alternative, 1, "plain");
    let mut doc = atom_text::apply(&doc, alternative, "[Cl,Br]", atom_text::Mode::Auto).unwrap();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));

    let input =
        generic::prepare_component_expansion_input(&doc, &[carbon], generic::Limits::default())
            .unwrap();
    let expanded = generic::expand_prepared(input).unwrap();
    assert_eq!(expanded.count(), 2);
    let expanded = expanded.take().unwrap();
    let mut elements: Vec<_> = expanded
        .atoms
        .iter()
        .map(|atom| atom.element.as_str())
        .collect();
    elements.sort_unstable();
    assert_eq!(elements, ["Br", "C", "C", "Cl"]);
    assert!(expanded.science.is_empty());
    assert_eq!(results_for(&doc, oxygen)[0].freshness, Freshness::Current);
    expanded.validate().unwrap();
}

#[test]
fn apply_geometry_moves_model_not_drawing() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let old_model = doc.models[0].clone();
    let drawing = doc.atoms.clone();
    let mut final_frame = (*start.frame).clone();
    for (i, xyz) in final_frame.positions_angstrom.iter_mut().enumerate() {
        *xyz = [i as f64 * 0.93 + 0.123_456_789_123, -0.765_432_109_876, 0.2];
    }
    let mut optimized = result(&final_frame, Task::Optimize, false);
    optimized.input_frame = Some(start.frame.hash());
    let mut attached = None;
    reconciled_edit(&mut doc, &mut History::default(), |doc| {
        attached = Some(attach(doc, &start, vec![final_frame.clone()], optimized).unwrap());
    });
    let attachment = attached.unwrap().attachment;
    let final_hash = final_frame.hash();
    let mut history = History::default();
    reconciled_edit(&mut doc, &mut history, |doc| {
        apply_geometry(doc, attachment, &final_hash).unwrap()
    });
    assert_eq!(history.frames(), 1);
    assert_eq!(doc.atoms, drawing);
    assert_eq!(
        doc.models[0].atoms[0].xyz,
        final_frame.positions_angstrom[0].map(|x| x as f32)
    );
    for (hydrogen, xyz) in doc.models[0]
        .hydrogens
        .iter()
        .zip(&final_frame.positions_angstrom[1..])
    {
        assert_eq!(hydrogen.xyz, xyz.map(|x| x as f32));
    }
    let applied = launch(&doc, oxygen, 0, 1).unwrap();
    assert!(Arc::ptr_eq(
        &applied.frame,
        doc.science.frame(&final_hash).unwrap()
    ));
    assert_eq!(applied.frame.as_ref(), &final_frame);
    assert!(history.undo(&mut doc));
    assert_eq!(doc.models[0], old_model);
    assert!(history.redo(&mut doc));
    let mut mismatch = doc.clone();
    reconciled_edit(&mut mismatch, &mut History::default(), |doc| {
        doc.models[0].hydrogens.pop();
    });
    assert!(matches!(
        apply_geometry(&mut mismatch, attachment, &final_hash),
        Err(ScienceError::ModelMismatch)
    ));
    reconciled_edit(&mut doc, &mut history, |doc| {
        doc.atom_mut(oxygen).unwrap().element = "S".into();
    });
    assert!(matches!(
        apply_geometry(&mut doc, attachment, &final_hash),
        Err(ScienceError::Stale)
    ));
}

#[test]
fn optimization_view_applies_the_final_geometry() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let mut final_frame = (*start.frame).clone();
    final_frame.positions_angstrom[0][0] += 0.234_567_891;
    let final_hash = final_frame.hash();
    let mut optimized = result(&final_frame, Task::Optimize, false);
    optimized.input_frame = Some(start.frame.hash());
    attach(&mut doc, &start, vec![final_frame.clone()], optimized).unwrap();

    let views = results_for(&doc, oxygen);
    assert_eq!(views.len(), 1);
    let view = &views[0];
    assert_eq!(view.attachment.frame, start.frame.hash());
    assert_eq!(view.frame.as_ref(), &final_frame);
    let (attachment, frame) = (view.attachment.id, view.stored.result.frame);
    apply_geometry(&mut doc, attachment, &frame).unwrap();
    let model = &doc.models[0];
    let coordinates: Vec<_> = model
        .atoms
        .iter()
        .map(|a| a.xyz)
        .chain(model.hydrogens.iter().map(|h| h.xyz))
        .collect();
    let expected: Vec<_> = final_frame
        .positions_angstrom
        .iter()
        .map(|p| p.map(|x| x as f32))
        .collect();
    assert_eq!(coordinates, expected);
    assert_ne!(
        coordinates[0],
        start.frame.positions_angstrom[0].map(|x| x as f32)
    );
    assert_eq!(launch(&doc, oxygen, 0, 1).unwrap().frame.hash(), final_hash);
}

#[test]
fn apply_geometry_refuses_out_of_range_coordinates_and_swapped_hydrogens() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let mut far = (*start.frame).clone();
    far.positions_angstrom[1][0] = 20_000.;
    let hash = far.hash();
    let mut optimized = result(&far, Task::Optimize, false);
    optimized.input_frame = Some(start.frame.hash());
    let attachment = attach(&mut doc, &start, vec![far], optimized)
        .unwrap()
        .attachment;
    let original = doc.models[0].clone();
    assert!(matches!(
        apply_geometry(&mut doc, attachment, &hash),
        Err(ScienceError::ModelMismatch)
    ));
    assert_eq!(doc.models[0], original);

    let (mut doc, ids) = ethanol();
    let start = launch(&doc, ids[0], 0, 1).unwrap();
    let attachment =
        add_result(&mut doc, &start, result(&start.frame, Task::Energy, false)).attachment;
    let hash = start.frame.hash();
    apply_geometry(&mut doc, attachment, &hash).unwrap();
    doc.models[0].hydrogens.reverse();
    let reversed = doc.models[0].clone();
    assert!(matches!(
        apply_geometry(&mut doc, attachment, &hash),
        Err(ScienceError::ModelMismatch)
    ));
    assert_eq!(doc.models[0], reversed);
}

#[test]
fn make_lighter_and_keep_full() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let full = result(&start.frame, Task::Hessian, true);
    add_result(&mut doc, &start, full.clone());
    let mut imported = imported(full);
    imported.keep_full = true;
    add_result(&mut doc, &start, imported);
    let light_sizes = doc.science.saved_sizes();
    reconciled_edit(&mut doc, &mut History::default(), |doc| {
        assert!(set_keep_full(doc, 0, true));
    });
    let full_sizes = doc.science.saved_sizes();
    assert!(full_sizes[0] > light_sizes[0]);
    assert_eq!(full_sizes[1], light_sizes[1]);
    let imported = Arc::clone(&doc.science.results[1].result);
    let file_bytes = doc.science.file_bytes();
    reconciled_edit(&mut doc, &mut History::default(), |doc| {
        assert_eq!(make_lighter(doc), 1)
    });
    assert!(!doc.science.results[0].keep_full);
    let light = &doc.science.results[0].result;
    assert!(light.hessian_hartree_per_bohr2.is_empty());
    assert!(light.gradient_hartree_per_bohr.is_empty());
    assert!(light.vibrations.as_ref().unwrap().modes.is_empty());
    assert!(
        light
            .orbitals
            .as_ref()
            .unwrap()
            .alpha
            .coefficients
            .is_empty()
    );
    assert_eq!(doc.science.saved_sizes(), light_sizes);
    assert!(Arc::ptr_eq(&doc.science.results[1].result, &imported));
    assert!(doc.science.file_bytes() < file_bytes);
    assert_eq!(make_lighter(&mut doc), 0);
}

#[test]
fn invalid_science_is_refused_on_read() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    let valid: Value = serde_json::from_slice(&doc.file_json().unwrap()).unwrap();
    let mut missing = valid.clone();
    let result = &mut missing["science"]["results"][0]["result"];
    result["input_frame"] = result["frame"].clone();
    result["frame"] = json!("00".repeat(32));
    let mut duplicate = valid.clone();
    let frame = duplicate["science"]["frames"][0].clone();
    duplicate["science"]["frames"]
        .as_array_mut()
        .unwrap()
        .push(frame);
    let mut mismatch = valid.clone();
    mismatch["science"]["attachments"][0]["atoms"]
        .as_array_mut()
        .unwrap()
        .pop();
    let mut unknown = valid;
    unknown["science"]["unexpected"] = json!(true);
    for (bad, message) in [
        (missing, "Calculation data refers to a missing frame"),
        (duplicate, "Calculation data repeats a frame"),
        (mismatch, "Invalid calculation attachment"),
        (unknown, "unknown field `unexpected`"),
    ] {
        let error = Document::from_json(&serde_json::to_vec(&bad).unwrap()).unwrap_err();
        assert!(error.contains(message), "{error}");
    }
}

#[test]
fn saved_size_budgets() {
    println!("| Atoms | Drawing-only KB | Light result KB | Full result KB | Light increment KB |");
    println!("| ---: | ---: | ---: | ---: | ---: |");
    for n in [50, 200] {
        let (mut doc, ids) = chain(n);
        let drawing_bytes = doc.file_json().unwrap().len();
        let start = launch(&doc, ids[0], 0, 1).unwrap();
        let mut final_frame = (*start.frame).clone();
        for (i, xyz) in final_frame.positions_angstrom.iter_mut().enumerate() {
            xyz[2] += (i % 7) as f64 * 0.013;
        }
        let mut optimized = result(&final_frame, Task::Optimize, true);
        optimized.input_frame = Some(start.frame.hash());
        assert_eq!(
            optimized
                .orbitals
                .as_ref()
                .unwrap()
                .alpha
                .energies_hartree
                .len(),
            n * 4
        );
        reconciled_edit(&mut doc, &mut History::default(), |doc| {
            attach(doc, &start, vec![final_frame], optimized).unwrap();
        });
        let light_bytes = doc.file_json().unwrap().len();
        let increment = light_bytes - drawing_bytes;
        assert!(
            increment <= n * 1_000,
            "{n} atoms: light increment {increment} bytes"
        );
        reconciled_edit(&mut doc, &mut History::default(), |doc| {
            assert!(set_keep_full(doc, 0, true));
        });
        let full_bytes = doc.file_json().unwrap().len();
        assert!(full_bytes > light_bytes);
        println!(
            "| {n} | {:.2} | {:.2} | {:.2} | {:.2} |",
            drawing_bytes as f64 / 1_000.,
            light_bytes as f64 / 1_000.,
            full_bytes as f64 / 1_000.,
            increment as f64 / 1_000.
        );
    }
}

fn wedged_halocarbon() -> (Document, u64, Launch) {
    let mut doc = Document::default();
    let carbon = doc.add_atom("C", Point::default());
    let fluorine = doc.add_atom("F", Point::new(0., -28.));
    let chlorine = doc.add_atom("Cl", Point::new(-28., 14.));
    let bromine = doc.add_atom("Br", Point::new(28., 14.));
    doc.add_bond(carbon, fluorine, 1, "wedge");
    doc.add_bond(carbon, chlorine, 1, "plain");
    doc.add_bond(carbon, bromine, 1, "plain");
    let atoms = [
        Placed {
            id: carbon,
            xyz: [0., 0., 0.],
        },
        Placed {
            id: fluorine,
            xyz: [0., 1., 0.8],
        },
        Placed {
            id: chlorine,
            xyz: [-1., -0.5, 0.],
        },
        Placed {
            id: bromine,
            xyz: [1., -0.5, 0.],
        },
    ];
    let hydrogens = vec![Hydrogen {
        parent: carbon,
        xyz: [0., 0., -1.],
    }];
    doc.models.push(Model3d::new(
        &doc,
        atoms.to_vec(),
        hydrogens,
        ForceField::Mmff94s,
    ));
    crate::models3d::require_version(&mut doc);
    let start = launch(&doc, carbon, 0, 1).unwrap();
    (doc, carbon, start)
}

fn butene(cis: bool) -> (Document, [u64; 4], Launch) {
    let mut doc = Document::default();
    let right_y = if cis { 1. } else { -1. };
    let geometry = [
        [-1.5, 1., 0.],
        [-0.67, 0., 0.],
        [0.67, 0., 0.],
        [1.5, right_y, 0.],
    ];
    let ids = geometry.map(|[x, y, _]| doc.add_atom("C", Point::new(x * 28., -y * 28.)));
    doc.add_bond(ids[0], ids[1], 1, "plain");
    doc.add_bond(ids[2], ids[1], 2, "plain");
    doc.add_bond(ids[2], ids[3], 1, "plain");
    // Geometry order differs from drawing order, and the double bond points left.
    let atoms = ids
        .into_iter()
        .zip(geometry)
        .rev()
        .map(|(id, xyz)| Placed { id, xyz })
        .collect();
    doc.models
        .push(Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s));
    crate::models3d::require_version(&mut doc);
    let start = launch(&doc, ids[0], 0, 1).unwrap();
    (doc, ids, start)
}

#[test]
fn stereo_check_reports_inverted_centre_and_flipped_double_bond() {
    let (mut doc, carbon, start) = wedged_halocarbon();
    assert!(
        stereo_changes(&doc, &start.atoms, &start.frame)
            .unwrap()
            .is_empty()
    );
    let mut mirrored = (*start.frame).clone();
    for xyz in &mut mirrored.positions_angstrom {
        xyz[0] = -xyz[0];
    }
    assert_eq!(
        stereo_changes(&doc, &start.atoms, &mirrored).unwrap(),
        vec![StereoChange::Centre {
            atom: carbon,
            inverted: true
        }],
    );
    let mut flat = (*start.frame).clone();
    for xyz in &mut flat.positions_angstrom {
        xyz[2] = 0.;
    }
    assert_eq!(
        stereo_changes(&doc, &start.atoms, &flat).unwrap(),
        vec![StereoChange::Centre {
            atom: carbon,
            inverted: false
        }],
    );
    assert!(matches!(
        stereo_changes(&doc, &start.atoms[..start.atoms.len() - 1], &start.frame),
        Err(ScienceError::ModelMismatch),
    ));

    // An explicit winding can coexist with a wavy bond in imported drawings.
    reconciled_edit(&mut doc, &mut History::default(), |doc| {
        doc.atoms[0].stereo = Some(crate::document::AtomStereo {
            winding: "ccw".into(),
            neighbors: start.atoms.iter().flatten().copied().skip(1).collect(),
        });
        doc.bonds[0].display = "wavy".into();
        doc.bonds[0].reverse();
    });
    let molecule = crate::chemistry::document::prepare(&doc).unwrap();
    assert_eq!(molecule.state.metadata.atoms[0].chiral_tag, 2);
    assert!(
        stereo_changes(&doc, &start.atoms, &mirrored)
            .unwrap()
            .is_empty()
    );

    for (cis, expected_stereo) in [(true, 2), (false, 3)] {
        let (doc, ids, start) = butene(cis);
        let molecule = crate::chemistry::document::prepare(&doc).unwrap();
        let bond = &molecule.state.metadata.bonds[1];
        assert_eq!(bond.stereo, expected_stereo);
        assert_eq!(bond.stereo_atoms, [3, 0]);
        assert!(
            stereo_changes(&doc, &start.atoms, &start.frame)
                .unwrap()
                .is_empty()
        );

        let mut flipped = (*start.frame).clone();
        flipped.positions_angstrom[0][1] *= -1.;
        assert_eq!(
            stereo_changes(&doc, &start.atoms, &flipped).unwrap(),
            vec![StereoChange::DoubleBond {
                atoms: [ids[2], ids[1]]
            }],
        );
    }
}

/// A drawing and a hand-built heavy-atom frame; no 3D model is needed.
fn ibuprofen_stereo(wedge: bool) -> (Document, Vec<u64>, Frame) {
    let geometry = [
        [0., 0., 0.], // Stereocentre.
        [0., 1., 0.8],
        [-1., -0.5, 0.], // Carboxyl carbon and oxygens.
        [-2., -0.3, 0.],
        [-1.1, -1.8, 0.2],
        [1., -0.5, 0.], // Phenyl ring, para substituted.
        [1.6, -1.5, 0.],
        [2.9, -1.5, 0.],
        [3.5, -0.5, 0.],
        [2.9, 0.5, 0.],
        [1.6, 0.5, 0.],
        [4.8, -0.5, 0.], // Isobutyl group.
        [5.5, 0.5, 0.1],
        [6.8, 0.5, 0.2],
        [5., 1.8, -0.2],
    ];
    let mut doc = Document::default();
    let ids: Vec<_> = geometry
        .iter()
        .enumerate()
        .map(|(index, &[x, y, _])| {
            let element = if matches!(index, 3 | 4) { "O" } else { "C" };
            doc.add_atom(element, Point::new(x as f32 * 28., -y as f32 * 28.))
        })
        .collect();
    doc.add_bond(ids[0], ids[1], 1, if wedge { "wedge" } else { "plain" });
    for (a, b, order) in [
        (0, 2, 1),
        (0, 5, 1),
        (2, 3, 2),
        (2, 4, 1),
        (5, 6, 2),
        (6, 7, 1),
        (7, 8, 2),
        (8, 9, 1),
        (9, 10, 2),
        (10, 5, 1),
        (8, 11, 1),
        (11, 12, 1),
        (12, 13, 1),
        (12, 14, 1),
    ] {
        doc.add_bond(ids[a], ids[b], order, "plain");
    }
    let frame = Frame {
        numbers: (0..ids.len())
            .map(|index| if matches!(index, 3 | 4) { 8 } else { 6 })
            .collect(),
        positions_angstrom: geometry.to_vec(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    (doc, ids, frame)
}

#[test]
fn tetrahedral_stereo_ibuprofen_uses_frame_cip_and_retains_flat_targets() {
    let (doc, ids, frame) = ibuprofen_stereo(false);
    let before = doc.clone();
    let map: Vec<_> = ids.iter().copied().map(Some).collect();
    let drawing = tetrahedral_stereo(&doc, &ids, None).unwrap();
    assert_eq!(
        drawing,
        TetrahedralStereo {
            undefined: vec![UndefinedCentre {
                atom: ids[0],
                cip: None,
                winding: None,
            }],
            has_defined: false,
        }
    );
    let spatial = tetrahedral_stereo(&doc, &ids, Some((&map, &frame))).unwrap();
    assert!(!spatial.has_defined);
    assert_eq!(spatial.undefined.len(), 1);
    let centre = &spatial.undefined[0];
    assert_eq!(centre.atom, ids[0]);
    assert!(matches!(centre.cip.as_deref(), Some("R" | "S")));
    assert!(matches!(centre.winding, Some(1 | 2)));

    let mut mirror = frame.clone();
    for xyz in &mut mirror.positions_angstrom {
        xyz[0] = -xyz[0];
    }
    let mirrored = tetrahedral_stereo(&doc, &ids, Some((&map, &mirror))).unwrap();
    assert_eq!(mirrored.undefined[0].atom, ids[0]);
    assert!(matches!(
        mirrored.undefined[0].cip.as_deref(),
        Some("R" | "S")
    ));
    assert_ne!(mirrored.undefined[0].cip, centre.cip);
    assert_ne!(mirrored.undefined[0].winding, centre.winding);

    let mut flat = frame.clone();
    for xyz in &mut flat.positions_angstrom {
        xyz[2] = 0.;
    }
    assert_eq!(
        tetrahedral_stereo(&doc, &ids, Some((&map, &flat))).unwrap(),
        drawing
    );
    assert_eq!(doc, before);
    assert!(matches!(
        tetrahedral_stereo(&doc, &ids, Some((&map[..map.len() - 1], &frame))),
        Err(ScienceError::ModelMismatch)
    ));

    // Frame order need not match the drawing's atom order.
    let mut reversed = frame.clone();
    reversed.numbers.reverse();
    reversed.positions_angstrom.reverse();
    let reverse_map: Vec<_> = map.iter().rev().copied().collect();
    assert_eq!(
        tetrahedral_stereo(&doc, &ids, Some((&reverse_map, &reversed))).unwrap(),
        spatial
    );

    let (defined, ids, frame) = ibuprofen_stereo(true);
    let map: Vec<_> = ids.iter().copied().map(Some).collect();
    let expected = TetrahedralStereo {
        undefined: vec![],
        has_defined: true,
    };
    assert_eq!(tetrahedral_stereo(&defined, &ids, None).unwrap(), expected);
    assert_eq!(
        tetrahedral_stereo(&defined, &ids, Some((&map, &frame))).unwrap(),
        expected
    );
}

#[test]
fn tetrahedral_stereo_wavy_touch_overrides_explicit_winding() {
    let (mut doc, ids, frame) = ibuprofen_stereo(true);
    doc.atom_mut(ids[0]).unwrap().stereo = Some(crate::document::AtomStereo {
        winding: "ccw".into(),
        neighbors: vec![ids[1], ids[2], ids[5]],
    });
    doc.bonds[0].display = "wavy".into();
    doc.bonds[0].reverse();
    assert_eq!(
        crate::chemistry::document::prepare(&doc)
            .unwrap()
            .state
            .metadata
            .atoms[0]
            .chiral_tag,
        2
    );
    let map: Vec<_> = ids.iter().copied().map(Some).collect();
    let report = tetrahedral_stereo(&doc, &ids, Some((&map, &frame))).unwrap();
    assert!(!report.has_defined);
    assert_eq!(report.undefined.len(), 1);
    assert_eq!(report.undefined[0].atom, ids[0]);
    assert!(matches!(
        report.undefined[0].cip.as_deref(),
        Some("R" | "S")
    ));
    assert!(matches!(report.undefined[0].winding, Some(1 | 2)));
}

#[test]
fn tetrahedral_stereo_keeps_defined_and_undefined_centres_separate() {
    let mut doc = Document::default();
    let geometry = [
        [0., 0., 0.],
        [0., 1., 0.8],
        [-1., -0.5, 0.],
        [1., -0.5, 0.],
        [1., -1.5, 0.8],
        [2., 0., 0.],
    ];
    let ids: Vec<_> = ["C", "F", "Cl", "C", "Br", "I"]
        .into_iter()
        .zip(geometry)
        .map(|(element, [x, y, _])| {
            doc.add_atom(element, Point::new(x as f32 * 28., -y as f32 * 28.))
        })
        .collect();
    for (a, b, display) in [
        (0, 1, "wedge"),
        (0, 2, "plain"),
        (0, 3, "plain"),
        (3, 4, "plain"),
        (3, 5, "plain"),
    ] {
        doc.add_bond(ids[a], ids[b], 1, display);
    }
    let frame = Frame {
        numbers: vec![6, 9, 17, 6, 35, 53],
        positions_angstrom: geometry.to_vec(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: vec![],
    };
    let map: Vec<_> = ids.iter().copied().map(Some).collect();
    let report = tetrahedral_stereo(&doc, &ids, Some((&map, &frame))).unwrap();
    assert!(report.has_defined);
    assert_eq!(report.undefined.len(), 1);
    assert_eq!(report.undefined[0].atom, ids[3]);
    assert!(matches!(
        report.undefined[0].cip.as_deref(),
        Some("R" | "S")
    ));
    let drawn = tetrahedral_stereo(&doc, &ids, None).unwrap();
    assert!(drawn.has_defined);
    assert_eq!(drawn.undefined[0].atom, ids[3]);
    assert_eq!(drawn.undefined[0].cip, None);
}

#[test]
fn launch_refuses_a_model_of_several_molecules() {
    let mut doc = Document::default();
    let first = doc.add_atom("O", Point::new(0., 0.));
    let second = doc.add_atom("O", Point::new(80., 0.));
    let atoms = vec![
        Placed {
            id: first,
            xyz: [0., 0., 0.],
        },
        Placed {
            id: second,
            xyz: [3., 0., 0.],
        },
    ];
    let model =
        Model3d::new(&doc, atoms, vec![], ForceField::Mmff94s).with_cell(&doc, None, vec![]);
    doc.models.push(model);
    crate::models3d::require_version(&mut doc);
    assert!(matches!(
        launch(&doc, first, 0, 1),
        Err(ScienceError::Fragments)
    ));
}

#[test]
fn stereo_group_change_makes_results_stale() {
    use crate::stereo_groups::{Kind, StereoGroup};
    let (mut doc, carbon, _) = wedged_halocarbon();
    doc.atom_mut(carbon).unwrap().stereo_group = Some(StereoGroup {
        kind: Kind::And,
        number: 1,
    });
    let start = launch(&doc, carbon, 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    assert_eq!(results_for(&doc, carbon)[0].freshness, Freshness::Current);
    let mut history = History::default();
    reconciled_edit(&mut doc, &mut history, |doc| {
        doc.atom_mut(carbon).unwrap().stereo_group = Some(StereoGroup {
            kind: Kind::Or,
            number: 1,
        });
    });
    assert_eq!(results_for(&doc, carbon)[0].freshness, Freshness::Stale);
    assert!(history.undo(&mut doc));
    assert_eq!(results_for(&doc, carbon)[0].freshness, Freshness::Current);
}

#[test]
fn a_result_for_other_atoms_is_refused() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    let mut methane = (*start.frame).clone();
    methane.numbers[0] = 6;
    let mut optimized = result(&methane, Task::Optimize, false);
    optimized.input_frame = Some(start.frame.hash());
    let before = doc.clone();
    assert!(matches!(
        attach(&mut doc, &start, vec![methane], optimized),
        Err(ScienceError::Unknown)
    ));
    assert_eq!(doc, before);
    let mut heavy_water = (*start.frame).clone();
    heavy_water.isotopes = vec![0, 2, 2];
    let mut deuterated = result(&heavy_water, Task::Optimize, false);
    deuterated.input_frame = Some(start.frame.hash());
    assert!(matches!(
        attach(&mut doc, &start, vec![heavy_water], deuterated),
        Err(ScienceError::Unknown)
    ));
    let mut weighed = (*start.frame).clone();
    weighed.masses_amu = vec![16.0, 2.0, 1.0];
    let mut reweighed = result(&weighed, Task::Optimize, false);
    reweighed.input_frame = Some(start.frame.hash());
    assert!(matches!(
        attach(&mut doc, &start, vec![weighed], reweighed),
        Err(ScienceError::Unknown)
    ));
    // An empty isotope list and all zeros both mean natural abundance.
    let mut moved = (*start.frame).clone();
    moved.isotopes = vec![0, 0, 0];
    moved.positions_angstrom[0][0] += 0.01;
    let mut optimized = result(&moved, Task::Optimize, false);
    optimized.input_frame = Some(start.frame.hash());
    assert!(attach(&mut doc, &start, vec![moved], optimized).is_ok());
    let mut short = start.clone();
    short.atoms.pop();
    assert!(matches!(
        attach(
            &mut doc,
            &short,
            vec![],
            result(&start.frame, Task::Energy, false)
        ),
        Err(ScienceError::Unknown)
    ));
}

#[test]
fn history_science_bytes_count_snapshot_attachments() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    let mut history = History::default();
    let payloads: usize = doc.science.payloads().map(|(_, bytes)| bytes).sum();
    for step in 0..5 {
        reconciled_edit(&mut doc, &mut history, |doc| {
            doc.atom_mut(oxygen).unwrap().position = Point::new(step as f32, 0.);
        });
    }
    let containers = doc.science.container_bytes();
    // Five snapshots and the live document each own their lists and attachments.
    assert_eq!(history.science_bytes(&doc), payloads + 6 * containers);
}

#[test]
fn oversized_baselines_are_refused_on_read() {
    let (mut doc, oxygen) = water();
    let start = launch(&doc, oxygen, 0, 1).unwrap();
    add_result(&mut doc, &start, result(&start.frame, Task::Energy, false));
    let mut json: Value = serde_json::from_slice(&doc.file_json().unwrap()).unwrap();
    json["science"]["attachments"][0]["baseline"] = json!("0".repeat(10_000));
    assert!(Document::from_json(&serde_json::to_vec(&json).unwrap()).is_err());
}

mod import;
