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
    let output = source.output(&settings, "test", "ReShiki test").unwrap();
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
    let output = source.output(&settings, "test", "ReShiki test").unwrap();
    assert!(output.main.text.contains("QST3"));
    for z in ["0.000000", "0.200000", "0.400000"] {
        assert!(output.main.text.contains(z));
    }
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
    let output = source.output(&settings, "test", "ReShiki test").unwrap();
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
fn path_job_changes_update_the_file_suffix_and_preserve_the_reaction_title() {
    let mut source = source();
    source.name = "Allyl vinyl ether → pent-4-enal".into();
    source.stem = reaction_stem("  Allyl   vinyl ether  ");
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

#[test]
fn reaction_file_stems_keep_formula_digits_and_cannot_contain_path_segments() {
    assert_eq!(reaction_stem("C5H8O"), "c5h8o");
    assert_eq!(
        reaction_stem("../Allyl vinyl ether\\input"),
        "allyl-vinyl-ether-input"
    );
    assert_eq!(reaction_stem(" / "), "reaction");
}
