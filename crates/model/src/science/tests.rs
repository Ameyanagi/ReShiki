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
            solvent: None,
            electronic_temperature_k: None,
            accuracy: None,
            max_iterations: None,
            gfnff: None,
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
        orbitals: Some(Orbitals {
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
        }),
        trajectory: None,
        keep_full: false,
        omitted_bytes: None,
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
    assert_eq!(doc.science.attachments.len(), 2);
    assert!(Arc::ptr_eq(
        doc.science.frame(&start.frame.hash()).unwrap(),
        &start.frame
    ));
    assert_eq!(
        doc.science.frame(&final_frame.hash()).unwrap().as_ref(),
        &final_frame
    );
    assert_eq!(doc.science.results[0].energy_hartree, Some(-7.5));
    assert_eq!(doc.science.results[1].energy_hartree, Some(-7.6));
    assert_eq!(
        doc.science.attachment(ids[0]).unwrap().frame,
        final_frame.hash()
    );
    assert_eq!(results_for(&doc, oxygen).len(), 2);
    assert_eq!(history.frames(), 2);
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
    assert_eq!(reopened.science.results[0].as_ref(), &full.light(&[]));
    assert!(
        reopened.science.results[0]
            .vibrations
            .as_ref()
            .unwrap()
            .modes
            .is_empty()
    );
    assert!(
        reopened.science.results[0]
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
    assert_eq!(reopened.science.results[1].as_ref(), &small_import);
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
    assert_eq!(reopened.science.results[0].as_ref(), &light);
    let stored = Arc::clone(&large.science.results[0]);
    reconciled_edit(&mut large, &mut History::default(), |doc| {
        assert!(set_keep_full(doc, &stored, true));
    });
    let reopened = Document::from_native_file(&large.file_json().unwrap()).unwrap();
    let mut kept = large_import;
    kept.keep_full = true;
    assert_eq!(reopened.science.results[0].as_ref(), &kept);
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
    let saved = &json["science"]["results"][0];
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
    assert!(results_for(&doc, ids[2]).is_empty());
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
    let heap = doc.science.heap_bytes();
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
    assert_eq!(history.science_bytes(&doc), heap);
    assert!(
        history
            .undo_frames()
            .all(|snapshot| Arc::ptr_eq(&snapshot.science.results[0], &doc.science.results[0]))
    );
    // MAX_SCIENCE_BYTES is fixed: verify the exact accounting used by fit_science
    // instead of allocating hundreds of MiB solely to cross its production limit.
    assert!(history.fit_science(&doc));
    assert_eq!(history.frames(), 30);
    let mut separate = doc.clone();
    separate.science.results[0] = Arc::new((*doc.science.results[0]).clone());
    assert_eq!(
        history.science_bytes(&separate),
        heap + doc.science.results[0].heap_bytes()
    );
    let bytes = history.bytes();
    assert_eq!(history.release(usize::MAX, 0), bytes);
    assert_eq!(history.frames(), 0);
    assert_eq!(history.bytes(), 0);
    assert_eq!(history.science_bytes(&doc), heap);
    assert_eq!(history.science_bytes(&plain), 0);
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
        &target.science.results[0],
        &source.science.results[0]
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
        &duplicated.science.results[0],
        &duplicated.science.results[1]
    ));
    assert_eq!(
        History::default().science_bytes(&duplicated),
        source.science.heap_bytes()
    );
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
    let mut history = History::default();
    reconciled_edit(&mut doc, &mut history, |doc| {
        apply_geometry(doc, attachment).unwrap()
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
        doc.science.frame(&final_frame.hash()).unwrap()
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
        apply_geometry(&mut mismatch, attachment),
        Err(ScienceError::ModelMismatch)
    ));
    reconciled_edit(&mut doc, &mut history, |doc| {
        doc.atom_mut(oxygen).unwrap().element = "S".into();
    });
    assert!(matches!(
        apply_geometry(&mut doc, attachment),
        Err(ScienceError::Stale)
    ));
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
    let xtb = Arc::clone(&doc.science.results[0]);
    let light_bytes = doc.science.saved_bytes(&xtb).unwrap();
    reconciled_edit(&mut doc, &mut History::default(), |doc| {
        assert!(set_keep_full(doc, &xtb, true));
    });
    let xtb = Arc::clone(&doc.science.results[0]);
    assert!(doc.science.saved_bytes(&xtb).unwrap() > light_bytes);
    let imported = Arc::clone(&doc.science.results[1]);
    let file_bytes = doc.science.file_bytes();
    reconciled_edit(&mut doc, &mut History::default(), |doc| {
        assert_eq!(make_lighter(doc), 1)
    });
    let light = &doc.science.results[0];
    assert!(!light.keep_full);
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
    assert_eq!(doc.science.saved_bytes(light).unwrap(), light_bytes);
    assert!(Arc::ptr_eq(&doc.science.results[1], &imported));
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
    missing["science"]["results"][0]["frame"] = json!("00".repeat(32));
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
        let result = Arc::clone(&doc.science.results[0]);
        reconciled_edit(&mut doc, &mut History::default(), |doc| {
            assert!(set_keep_full(doc, &result, true));
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
