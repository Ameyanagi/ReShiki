use super::*;

fn analysis(smiles: &str) -> Analysis {
    let state = reshiki::chemistry::smiles::parse(smiles).unwrap();
    let properties = reshiki::chemistry::properties(&state.graph.atom_facts().unwrap()).unwrap();
    let mut analysis = crate::app::tests::completed_analysis();
    analysis.formula = properties.formula;
    analysis.display_formula = properties.display_formula;
    analysis.mass_percent = properties.mass_percent;
    analysis.mass = properties.mass;
    analysis.exact_mass = properties.exact_mass;
    analysis
}

#[test]
fn elemental_analysis_formats_combustion_elements_and_isotope_mass() {
    let (mut app, _) = App::new();
    let elemental_analysis = |a: &Analysis| app.elemental_analysis(a);
    assert_eq!(
        elemental_analysis(&analysis("CCO")).as_deref(),
        Some("Anal. Calcd for C₂H₆O: C, 52.14; H, 13.13.")
    );
    let deuterated = analysis("[2H]OC([2H])([2H])[2H]");
    let hydrogen = deuterated
        .mass_percent
        .iter()
        .find(|(symbol, _)| symbol == "H")
        .unwrap()
        .1;
    assert!((hydrogen - 4. * 2.014101778 / deuterated.mass * 100.).abs() < 1e-12);
    assert_eq!(
        elemental_analysis(&deuterated).as_deref(),
        Some("Anal. Calcd for CD₄O: C, 33.30; H, 22.34.")
    );
    assert_eq!(elemental_analysis(&analysis("O=O")), None);
    let mut reordered = analysis("C");
    reordered.mass_percent = vec![
        ("S".into(), 4.),
        ("O".into(), 50.),
        ("N".into(), 3.),
        ("H".into(), 2.),
        ("C".into(), 41.),
    ];
    assert_eq!(
        elemental_analysis(&reordered).as_deref(),
        Some("Anal. Calcd for CH₄: C, 41.00; H, 2.00; N, 3.00; S, 4.00.")
    );
    let mut radical_cation = analysis("c1ccccc1");
    radical_cation.display_formula = "[C₆H₆]•⁺".into();
    assert!(
        app.elemental_analysis(&radical_cation)
            .unwrap()
            .contains("[C₆H₆]•⁺:")
    );
    app.tab.doc.drawing_style.charge_before_radical = true;
    assert!(
        app.elemental_analysis(&radical_cation)
            .unwrap()
            .contains("[C₆H₆]⁺•:")
    );
}

#[test]
fn property_rows_define_methods_and_preserve_exact_mass_precision() {
    let analysis = analysis("CCO");
    let rows = property_rows(&analysis, false);
    let expected = [
        ("Weight (g/mol)", Some("RDKit"), "46.069"),
        ("Exact mass (Da)", Some("Monoisotopic mass"), "46.04186"),
        ("cLogP", Some("Wildman–Crippen"), "0.00"),
        (
            "TPSA (Å²)",
            Some("N and O contributions (Ertl method)"),
            "0.00",
        ),
        (
            "H-bond donors",
            Some("donor count from RDKit's Lipinski-style rules"),
            "0",
        ),
        (
            "H-bond acceptors",
            Some("acceptor count from RDKit's Lipinski-style rules"),
            "0",
        ),
        ("Rings", None, "0"),
        ("Unpaired electrons", Some("radical marks"), "0"),
    ];
    for ((label, hint, value), (expected_label, definition, expected_value)) in
        rows.iter().zip(expected)
    {
        assert_eq!(*label, expected_label);
        assert_eq!(value, expected_value);
        match definition {
            Some(definition) => assert!(hint.as_ref().unwrap().contains(definition)),
            None => assert!(hint.is_none()),
        }
    }
    let weight_hint = rows[0].1.as_ref().unwrap();
    assert!(weight_hint.contains(reshiki::chemistry::RDKIT_VERSION));
    assert!(weight_hint.contains("IUPAC 2023 longest-lived mass numbers"));
    assert!(weight_hint.contains("Labelled atoms use their nuclide mass"));
    let precise = property_rows(&analysis, true);
    assert_eq!(precise[0].2, "46.07");
    assert_eq!(precise[1], rows[1]);
}

#[test]
fn molecular_weight_precision_applies_to_status_and_summary_with_display_formula() {
    let (mut app, _) = App::new();
    let carbon = app.tab.doc.add_atom("C", Default::default());
    app.tab.doc.atom_mut(carbon).unwrap().isotope = 13;
    app.tab.analysis = Some(analysis("[13CH4]"));
    for (two_decimal_weight, weight) in [(false, "17.035"), (true, "17.04")] {
        app.view_action(super::super::view_settings::Action::TwoDecimalWeight(
            two_decimal_weight,
        ));
        let _ = app.show_molecular_properties();
        assert_eq!(
            app.status,
            format!("Molecular properties · ¹³CH₄ · {weight} g/mol")
        );
        assert_eq!(
            app.property_summary(),
            format!("Whole drawing · ¹³CH₄ · {weight}\u{a0}g/\u{2060}mol")
        );
    }
}

#[tokio::test]
async fn copy_analysis_routes_the_displayed_text_to_the_clipboard() {
    use iced::futures::StreamExt;

    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.analysis = Some(analysis("CCO"));
    let mut effects = iced_runtime::task::into_stream(app.update(Message::CopyAnalysis)).unwrap();
    let expected = "Anal. Calcd for C2H6O: C, 52.14; H, 13.13.";
    assert!(matches!(
        effects.next().await,
        Some(iced_runtime::Action::Clipboard(iced_runtime::clipboard::Action::Write { contents, .. }))
            if contents == expected
    ));
    assert!(effects.next().await.is_none());

    app.tab.analysis = Some(analysis("O=O"));
    assert_eq!(app.copy_analysis().units(), 0);
    app.tab.analysis = Some(analysis("CCO"));
    app.copy_as_busy = true;
    assert_eq!(app.copy_analysis().units(), 0);
    assert_eq!(app.status, "A clipboard operation is already in progress");
}
