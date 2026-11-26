use super::*;
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
