use super::*;
use crate::app::qc_export::{Action as ExportAction, Field};
use reshiki::qc_input::{Job, Program};
use reshiki::qcdata::{Endpoints, Provenance, Status, Task, Vibrations};

fn source() -> Source {
    let frame = |z| {
        Arc::new(Frame {
            numbers: vec![1, 1],
            positions_angstrom: vec![[0., 0., 0.], [0.8, 0., z]],
            isotopes: vec![],
            masses_amu: vec![],
            charge: 0,
            multiplicity: 1,
            drawing_atoms: vec![],
        })
    };
    let reactant = frame(0.);
    let ts = frame(0.2);
    let product = frame(0.4);
    let result = CalcResult {
        frame: ts.hash(),
        input_frame: Some(reactant.hash()),
        task: Task::TransitionState,
        endpoints: Some(Endpoints {
            reactant: reactant.hash(),
            product: product.hash(),
        }),
        settings: reshiki::qcdata::Settings::Imported { keywords: vec![] },
        provenance: Provenance {
            program: "ReShiki".into(),
            program_version: None,
            method: Some("GFN2-xTB".into()),
            basis: None,
            source: reshiki::qcdata::Source::Calculated {
                engine_revision: "test".into(),
            },
        },
        status: Status::Converged,
        energy_hartree: Some(-1.),
        energy_terms: Default::default(),
        gradient_hartree_per_bohr: vec![],
        hessian_hartree_per_bohr2: vec![0.; 36],
        charges: vec![],
        dipole_e_bohr: None,
        bond_orders: vec![],
        orbitals: None,
        vibrations: Some(Vibrations {
            frequencies_cm1: vec![-350.],
            ir_intensities_km_mol: vec![],
            modes: vec![],
            reduced_masses_amu: vec![],
            masses_amu: vec![],
        }),
        thermo: None,
        trajectory: None,
        keep_full: false,
        omitted_bytes: None,
        run: None,
    };
    Source {
        name: "test TS".into(),
        stem: "test".into(),
        frame: ts,
        result: Arc::new(result),
        reference: None,
        reactant: Some(reactant),
        product: Some(product),
    }
}

#[test]
fn ts_source_writes_the_saved_saddle_endpoints_and_hessian() {
    let source = source();
    let [template, ..] = qc_input::builtin_templates();
    let mut settings = Settings::from_template(&template, Program::Orca, 0, 1, "TS".into());
    settings.job = Job::TransitionState;
    let output = source
        .output(&settings, "test", "ReShiki test", EnergyUnit::KcalPerMol)
        .unwrap();
    assert!(output.main.text.contains("OptTS"));
    assert!(output.main.text.contains("imaginary frequency -350 cm-1"));
    assert!(
        output
            .companions
            .iter()
            .any(|file| file.name == "test_xtb.hess")
    );
    assert!(output.main.text.contains("0.200000"));

    settings.program = Program::Gaussian;
    settings.job = Job::Qst3;
    let output = source
        .output(&settings, "test", "ReShiki test", EnergyUnit::KcalPerMol)
        .unwrap();
    assert!(output.main.text.contains("QST3"));
    for z in ["0.000000", "0.200000", "0.400000"] {
        assert!(output.main.text.contains(z));
    }
}

#[test]
fn ts_export_titles_follow_the_current_app_energy_unit() {
    let mut source = source();
    let energy = |delta| -1. + (delta - 26.5) / 627.509_474_063;
    Arc::make_mut(&mut source.result).trajectory = Some(reshiki::qcdata::Trajectory {
        rmsd_to_best_angstrom: vec![],
        numbers: source.frame.numbers.clone(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        positions_angstrom: vec![],
        energies_hartree: vec![energy(0.), energy(30.2), energy(-8.2)],
        labels: vec![],
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
    });
    let (mut app, _) = App::new();
    let _ = app.open_qc_export_source(source);
    for (unit, barrier, reaction) in [
        (EnergyUnit::KjPerMol, "110.9 kJ/mol", "−34.3 kJ/mol"),
        (EnergyUnit::Ev, "1.15 eV", "−0.36 eV"),
    ] {
        app.appearance.energy_unit = unit;
        for program in [
            Program::Gaussian,
            Program::Orca,
            Program::Gamess,
            Program::Xyz,
        ] {
            send(&mut app, ExportAction::Program(program));
            let output = app.qc_output().unwrap();
            assert!(output.main.text.contains(&format!("ΔE‡ {barrier}")));
            assert_eq!(
                output.main.text.contains(&format!("ΔE {reaction}")),
                program != Program::Gamess
            );
            assert!(!output.main.text.contains("kcal/mol"));
            assert!(output.main.text.contains("0.200000"));
        }
    }
}

#[test]
fn gamess_title_fits_eighty_columns_with_all_energies_and_the_complex_reference() {
    use reshiki::qcdata::{RunSummary, Thermo, TransitionStateMapping, TransitionStateRun};

    let mut source = source();
    source.reference = Some(super::super::view::OPTIMIZED_COMPLEX);
    let result = Arc::make_mut(&mut source.result);
    let correction = Thermo {
        temperature_k: 298.15,
        pressure_pa: 101_325.,
        zero_point_energy_hartree: 0.01,
        enthalpy_correction_hartree: 0.01,
        entropy_hartree_per_k: 0.,
        gibbs_correction_hartree: 0.01,
        heat_capacity_hartree_per_k: None,
        frequency_scale: 1.,
        rotor_cutoff_cm1: 50.,
        imaginary_cutoff_cm1: 20.,
        symmetry_number: 1,
        point_group: None,
    };
    result.thermo = Some(correction.clone());
    result.trajectory = Some(reshiki::qcdata::Trajectory {
        rmsd_to_best_angstrom: vec![],
        numbers: source.frame.numbers.clone(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        positions_angstrom: vec![],
        energies_hartree: vec![-1.1, -1., -1.2],
        labels: vec![],
        minimizer_energies_hartree: vec![],
        cluster_sizes: vec![],
    });
    result.run = Some(RunSummary {
        seconds: 1.,
        scf_iterations: vec![],
        finished: None,
        search: None,
        transition_state: Some(Box::new(TransitionStateRun {
            arrow: None,
            context: None,
            free_energy: true,
            endpoint_energies_hartree: Some([-1.1, -1.2]),
            endpoint_thermo: Some([Some(correction.clone()), Some(correction)]),
            reached_positions_angstrom: vec![],
            saddle_converged: true,
            imaginary: 1,
            imaginary_cm1: Some(-350.),
            climbing_image: 1,
            mapping: TransitionStateMapping {
                formed: vec![],
                broken: vec![],
                hydrogen_moves: vec![],
                automatic: false,
            },
            diagnostics: vec![],
        })),
    });
    assert!(super::super::view::reaction_energy_kcal(result).is_some());
    assert!(super::super::view::reaction_gibbs_kcal(result).is_some());
    let [template, ..] = qc_input::builtin_templates();
    let mut settings = Settings::from_template(
        &template,
        Program::Gamess,
        0,
        1,
        "Long reactant name → first product + second product".into(),
    );
    settings.job = Job::TransitionState;
    let output = source
        .output(&settings, "test", "ReShiki test", EnergyUnit::default())
        .unwrap();
    let mut lines = output.main.text.lines();
    assert!(lines.any(|line| line.trim() == "$DATA"));
    let title = lines.next().unwrap();
    assert!(title.len() <= 80, "{title}");
    assert!(title.contains("ΔE‡ 62.8 kcal/mol"));
    assert!(title.contains("ΔG‡ 62.8 kcal/mol"));
    assert!(title.contains("optimized complex"));
    assert!(
        !output
            .warnings
            .iter()
            .any(|warning| warning.contains("80 columns"))
    );
}

#[test]
fn dialog_defaults_to_transition_state_and_keeps_the_source_when_the_drawing_changes() {
    let (mut app, _) = App::new();
    let _ = app.open_qc_export_source(source());
    let text = app.qc_output().unwrap().main.text;
    assert!(text.contains("Opt=(TS"));
    assert!(text.contains("0.200000"));
    app.tab
        .doc
        .add_atom("C", reshiki::document::Point::default());
    assert_eq!(app.qc_output().unwrap().main.text, text);
}

#[test]
fn each_program_has_its_own_default_ts_file_name() {
    let (mut app, _) = App::new();
    let _ = app.open_qc_export_source(source());
    for (program, name) in [
        (Program::Gaussian, "test-ts.gjf"),
        (Program::Orca, "test-ts-orca.inp"),
        (Program::Gamess, "test-ts-gamess.inp"),
        (Program::Xyz, "test-ts.xyz"),
    ] {
        send(&mut app, ExportAction::Program(program));
        assert_eq!(app.qc_output().unwrap().main.name, name);
    }
}

#[test]
fn reopened_light_source_exports_without_hessian_and_explains_it_for_orca() {
    let mut source = source();
    Arc::make_mut(&mut source.result)
        .hessian_hartree_per_bohr2
        .clear();
    let [template, ..] = qc_input::builtin_templates();
    let mut settings = Settings::from_template(&template, Program::Orca, 0, 1, "TS".into());
    settings.job = Job::TransitionState;
    let output = source
        .output(&settings, "test", "ReShiki test", EnergyUnit::KcalPerMol)
        .unwrap();
    assert!(output.companions.is_empty());
    assert!(
        output
            .warnings
            .iter()
            .any(|warning| warning.contains("No xTB Hessian"))
    );
    assert!(source.has_endpoints());
}

fn send(app: &mut App, action: ExportAction) {
    let _ = app.update(Message::QcExport(action));
}

fn reaction_app(named: bool) -> (App, u64) {
    let (mut doc, arrow) = super::super::card::tests::claisen();
    if named {
        let reaction = doc.reactions[0].clone();
        for (atoms, name) in [
            (&reaction.reactants[0].atoms, "Allyl vinyl ether"),
            (&reaction.products[0].atoms, "pent-4-enal"),
        ] {
            let part = reshiki::editing::analysis_document(&doc, atoms);
            let prepared = reshiki::chemistry::document::prepare(&part).unwrap();
            let smiles = reshiki::molecule_names::canonical_identity(&prepared.state).unwrap();
            reshiki::molecule_names::show(
                &mut doc,
                atoms,
                name.into(),
                smiles,
                Default::default(),
                false,
            )
            .unwrap();
        }
    }
    let plan = reshiki::reactions::ts_plan::plan(&doc, arrow).unwrap();
    let mut numbers: Vec<_> = plan
        .start_atoms
        .iter()
        .map(|id| {
            reshiki::editing::ELEMENTS
                .iter()
                .position(|&element| element == doc.atom(*id).unwrap().element)
                .unwrap() as u8
                + 1
        })
        .collect();
    numbers.resize(plan.atoms, 1);
    let mut atoms: Vec<_> = plan.start_atoms.iter().copied().map(Some).collect();
    atoms.resize(plan.atoms, None);
    let frame = |z| Frame {
        numbers: numbers.clone(),
        positions_angstrom: (0..numbers.len())
            .map(|index| [index as f64 * 1.5, 0., z])
            .collect(),
        isotopes: vec![],
        masses_amu: vec![],
        charge: 0,
        multiplicity: 1,
        drawing_atoms: atoms.clone(),
    };
    let reactant = frame(0.);
    let ts = frame(0.2);
    let product = frame(0.4);
    let mut result = (*source().result).clone();
    result.frame = ts.hash();
    result.input_frame = Some(reactant.hash());
    result.endpoints = Some(Endpoints {
        reactant: reactant.hash(),
        product: product.hash(),
    });
    result.hessian_hartree_per_bohr2 = vec![0.; 9 * numbers.len().pow(2)];
    let launch = reshiki::science::Launch {
        frame: Arc::new(reactant),
        atoms,
        baseline: reshiki::science::baseline(&doc, &plan.start_atoms),
    };
    reshiki::science::attach(&mut doc, &launch, vec![ts, product], result).unwrap();
    let (mut app, _) = App::new();
    app.tab.doc = doc;
    app.tab.busy = false;
    (app, arrow)
}

#[test]
fn arrow_export_uses_its_reaction_title_when_another_result_is_shown() {
    let (mut app, arrow) = reaction_app(true);
    let mut other = app.tab.doc.science.results[0].clone();
    let mut result = (*other.result).clone();
    result.task = Task::Energy;
    result.frame = result.endpoints.unwrap().product;
    result.endpoints = None;
    other.result = Arc::new(result);
    app.tab.doc.science.results.push(other.clone());
    app.tab.xtb.palette.show(&other.result);
    let title = app.xtb_reaction_label(arrow).title();
    assert_eq!(title, "Allyl vinyl ether → pent-4-enal");

    let _ = app.xtb_ts_export_arrow(arrow);
    let output = app.qc_output().unwrap();
    assert_eq!(output.main.name, "allyl-vinyl-ether-ts.gjf");
    assert!(output.main.text.contains(&title));
    assert!(output.main.text.contains("0.200000"));
    assert!(Arc::ptr_eq(
        &app.xtb_shown().unwrap().stored.result,
        &other.result
    ));

    app.tab.doc.reactions.clear();
    let after = app.qc_output().unwrap();
    assert_eq!(after.main.name, output.main.name);
    assert_eq!(after.main.text, output.main.text);
}

#[test]
fn reopened_palette_export_uses_the_reactant_formula_without_a_linked_name() {
    let (mut app, arrow) = reaction_app(false);
    let result = app.tab.doc.science.results[0].result.clone();
    app.tab.xtb.palette.show(&result);
    assert!(app.xtb_shown().unwrap().ts.is_none());

    let _ = app.xtb_ts_export();
    let output = app.qc_output().unwrap();
    assert_eq!(output.main.name, "c5h8o-ts.gjf");
    assert!(
        output
            .main
            .text
            .contains(&app.xtb_reaction_label(arrow).title())
    );
}

#[test]
fn late_cached_name_updates_a_fresh_export_stem_and_keeps_the_saved_title() {
    use reshiki::qcdata::{RunSummary, TransitionStateContext, TransitionStateRun};
    use reshiki::reactions::Role;
    let (mut app, arrow) = reaction_app(false);
    app.inspector_open = false;
    let doc = &app.tab.doc;
    let reaction = &doc.reactions[0];
    let atoms = reaction.reactants[0].atoms.clone();
    let smiles = reshiki::naming::selected_identity(doc, &atoms)
        .unwrap()
        .smiles;
    let plan = reshiki::reactions::ts_plan::plan(doc, arrow).unwrap();
    let title = app.xtb_reaction_label(arrow).title();
    let context = TransitionStateContext {
        reactant_chemistry: super::super::endpoint_chemistry(doc, reaction, Role::Reactant),
        product_chemistry: super::super::endpoint_chemistry(doc, reaction, Role::Product),
        title: title.clone(),
        reactant: "C5H8O".into(),
        endpoint_complex: None,
    };
    let mapping = super::super::card::BondChanges::from_plan(doc, &plan).0;
    Arc::make_mut(&mut app.tab.doc.science.results[0].result).run = Some(RunSummary {
        seconds: 1.,
        scf_iterations: vec![],
        finished: None,
        search: None,
        transition_state: Some(Box::new(TransitionStateRun {
            arrow: Some(arrow),
            context: Some(context),
            free_energy: false,
            endpoint_energies_hartree: None,
            endpoint_thermo: None,
            reached_positions_angstrom: vec![],
            saddle_converged: true,
            imaginary: 1,
            imaginary_cm1: Some(-350.),
            climbing_image: 1,
            mapping,
            diagnostics: vec![],
        })),
    });
    app.tab
        .naming
        .caption_names
        .insert(smiles, "Allyl vinyl ether".into());
    assert_eq!(
        app.xtb_reaction_label(arrow).first_reactant(),
        Some("Allyl vinyl ether")
    );
    assert_eq!(
        app.start_caption_names().units(),
        0,
        "a hidden card only reads known names"
    );
    let _ = app.xtb_ts_export_arrow(arrow);
    let output = app.qc_output().unwrap();
    assert_eq!(output.main.name, "allyl-vinyl-ether-ts.gjf");
    assert!(output.main.text.contains(&title));
    assert!(app.tab.naming.caption_pending.is_none());
    let saved = super::super::saved_search(&app.tab.doc.science.results[0].result)
        .unwrap()
        .context
        .as_ref()
        .unwrap();
    assert_eq!(saved.title, title);
    assert_eq!(saved.reactant, "C5H8O");

    app.tab.doc.atom_mut(atoms[0]).unwrap().map_num = 0;
    let _ = app.xtb_ts_export_arrow(arrow);
    let output = app.qc_output().unwrap();
    assert_eq!(
        output.main.name, "c5h8o-ts.gjf",
        "stale geometry keeps its saved reactant name"
    );
    assert!(output.main.text.contains(&title));
}

#[test]
fn path_job_changes_update_the_file_suffix_and_preserve_the_reaction_title() {
    let mut source = source();
    source.name = "Allyl vinyl ether → pent-4-enal".into();
    source.stem = reaction_stem("  Allyl   vinyl ether  ", "C5H8O");
    let (mut app, _) = App::new();
    let _ = app.open_qc_export_source(source);
    for (job, suffix) in [
        (Job::TransitionState, "ts"),
        (Job::Qst2, "qst2"),
        (Job::Qst3, "qst3"),
        (Job::Irc, "irc"),
    ] {
        send(&mut app, ExportAction::Job(job));
        let output = app.qc_output().unwrap();
        assert_eq!(output.main.name, format!("allyl-vinyl-ether-{suffix}.gjf"));
        assert!(output.main.text.contains("Allyl vinyl ether → pent-4-enal"));
    }
    send(&mut app, ExportAction::Program(Program::Orca));
    send(&mut app, ExportAction::Job(Job::NebTs));
    let output = app.qc_output().unwrap();
    assert_eq!(output.main.name, "allyl-vinyl-ether-neb-orca.inp");
    assert!(
        output
            .companions
            .iter()
            .any(|file| file.name.starts_with("allyl-vinyl-ether-neb-orca"))
    );

    send(
        &mut app,
        ExportAction::Field(Field::Title, "Refined reaction".into()),
    );
    let output = app.qc_output().unwrap();
    assert_eq!(output.main.name, "allyl-vinyl-ether-neb-orca.inp");
    assert!(output.main.text.contains("Refined reaction"));
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn orca_neb_companions_follow_chosen_names_when_reactant_stems_collide() {
    let directory = tempfile::tempdir().unwrap();
    for (name, z) in [("sample_a", 0.2), ("sample-a", 0.6), ("sample a", 1.)] {
        let mut source = source();
        source.name = name.into();
        source.stem = reaction_stem(name, "H2");
        Arc::make_mut(&mut source.frame).positions_angstrom[1][2] = z;
        let product = Arc::make_mut(source.product.as_mut().unwrap());
        product.positions_angstrom[1][2] = z + 0.2;
        let result = Arc::make_mut(&mut source.result);
        result.frame = source.frame.hash();
        result.endpoints.as_mut().unwrap().product = product.hash();

        let (mut app, _) = App::new();
        let _ = app.open_qc_export_source(source);
        send(&mut app, ExportAction::Program(Program::Orca));
        send(&mut app, ExportAction::Job(Job::NebTs));
        assert_eq!(app.qc_output().unwrap().main.name, "sample-a-neb-orca.inp");
        let path = directory.path().join(format!("{name}.inp"));
        app.qc_input().unwrap().write(&path).unwrap();
        let input = std::fs::read_to_string(&path).unwrap();
        for (suffix, keyword, expected_z) in [
            ("product", "NEB_End_XYZFile", z + 0.2),
            ("ts", "NEB_TS_XYZFile", z),
        ] {
            let name = format!("{name}_{suffix}.xyz");
            assert!(input.contains(&format!("{keyword} \"{name}\"")));
            let text = std::fs::read_to_string(directory.path().join(name)).unwrap();
            let frame = qc_input::parse_xyz(&text).unwrap();
            assert!((frame.positions_angstrom[1][2] - expected_z).abs() < 1e-8);
        }
        assert!(!input.contains("sample-a-neb-orca_product.xyz"));
        assert!(!input.contains("sample-a-neb-orca_ts.xyz"));
    }
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 9);
    let first = std::fs::read_to_string(directory.path().join("sample_a_ts.xyz")).unwrap();
    assert!((qc_input::parse_xyz(&first).unwrap().positions_angstrom[1][2] - 0.2).abs() < 1e-8);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn orca_hessian_companions_follow_chosen_names_and_keep_existing_files() {
    let directory = tempfile::tempdir().unwrap();
    for (job, stem, keyword) in [
        (Job::TransitionState, "refined_ts", "InHessName"),
        (Job::Irc, "refined_irc", "Hess_Filename"),
    ] {
        let (mut app, _) = App::new();
        let _ = app.open_qc_export_source(source());
        send(&mut app, ExportAction::Program(Program::Orca));
        send(&mut app, ExportAction::Job(job));
        let path = directory.path().join(format!("{stem}.inp"));
        app.qc_input().unwrap().write(&path).unwrap();
        let input = std::fs::read_to_string(&path).unwrap();
        let name = format!("{stem}_xtb.hess");
        assert!(input.contains(&format!("{keyword} \"{name}\"")));
        let companion = directory.path().join(&name);
        assert!(
            std::fs::read_to_string(&companion)
                .unwrap()
                .contains("$hessian")
        );

        std::fs::write(&companion, "keep existing Hessian").unwrap();
        std::fs::write(&path, "keep existing input").unwrap();
        assert_eq!(
            app.qc_input().unwrap().write(&path),
            Err(format!(
                "Input not saved; {name} already exists beside it and was not replaced."
            ))
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "keep existing input"
        );
        assert_eq!(
            std::fs::read_to_string(companion).unwrap(),
            "keep existing Hessian"
        );
    }
}

#[test]
fn reaction_file_stems_keep_formula_digits_and_cannot_contain_path_segments() {
    assert_eq!(reaction_stem("C5H8O", "C5H8O"), "c5h8o");
    assert_eq!(
        reaction_stem("../Allyl vinyl ether\\input", "C5H8O"),
        "allyl-vinyl-ether-input"
    );
    assert_eq!(
        reaction_stem("  Allyl...__vinyl---ether  ", "C5H8O"),
        "allyl-vinyl-ether"
    );
    assert_eq!(reaction_stem(" / ", "C5H8O"), "c5h8o");
    assert_eq!(reaction_stem(&"a".repeat(65), "C5H8O"), "c5h8o");
    assert_eq!(reaction_stem(&"碳".repeat(22), "C5H8O"), "c5h8o");
    assert_eq!(reaction_stem(" / ", ""), "reaction");
}
