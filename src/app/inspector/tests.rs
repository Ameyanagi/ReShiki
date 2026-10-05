use super::*;
use reshiki::editing::Transform;
#[test]
fn background_properties_use_their_tabs_snapshot_and_selection() {
    use crate::app::tabs::tests::Front;
    for stale in [false, true] {
        let (mut app, _) = App::new();
        app.tab.busy = false;
        let atom = app.tab.doc.add_atom("N", crate::app::Point::default());
        app.tab.selected = vec![atom];
        let id = app.tab.id;
        let _ = app.update(Message::InspectorAction(Action::RefreshProperties));
        let key = app.tab.inspector_ui.pending.clone().unwrap();
        if stale {
            let before = app.tab.doc.clone();
            app.tab.doc.add_atom("O", crate::app::Point::new(42., 0.));
            app.changed(before);
        }
        let before = app.tab.doc.clone();
        let front = Front::new(&mut app);
        let _ = app.update(Message::Tab(
            id,
            Box::new(Message::InspectorAction(Action::PropertiesCalculated(
                key.clone(),
                Box::new(Err("Property notice".into())),
            ))),
        ));
        front.assert_unchanged(&app);
        let tab = &app.tabs.background[0];
        assert_eq!(tab.doc, before);
        assert!(tab.inspector_ui.pending.is_none());
        if stale {
            assert!(tab.inspector_ui.properties.is_none());
        } else {
            assert_eq!(tab.inspector_ui.properties.as_ref().unwrap().0, key);
            assert_eq!(
                tab.inspector_ui
                    .properties
                    .as_ref()
                    .unwrap()
                    .1
                    .as_ref()
                    .unwrap_err(),
                "Property notice"
            );
            assert!(!tab.history.can_undo());
        }
    }
}

#[test]
fn collapsed_sections_do_not_build_hidden_content() {
    let (mut app, _) = App::new();
    let builds = std::cell::Cell::new(0);
    for (default, override_value, expected) in [
        (false, None, 0),
        (true, Some(false), 0),
        (false, Some(true), 1),
        (true, None, 1),
    ] {
        app.tab.inspector_ui.expanded.clear();
        if let Some(expanded) = override_value {
            app.tab
                .inspector_ui
                .expanded
                .insert(Section::Molecule, expanded);
        }
        builds.set(0);
        let _ = app.inspector_section_lazy(Section::Molecule, "Properties", "", default, || {
            builds.set(builds.get() + 1);
            text("Expanded content")
        });
        assert_eq!(builds.get(), expected);
    }
}

#[test]
fn selection_summary_and_alignment_keep_group_and_point_semantics() {
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let c = app.tab.doc.add_atom("C", Default::default());
    let o = app
        .tab
        .doc
        .add_atom("O", reshiki::document::Point::new(42., 0.));
    let point = app
        .tab
        .doc
        .add_atom("*", reshiki::document::Point::new(21., 24.));
    app.tab.doc.add_bond(c, o, 1, "plain");
    assert_eq!(app.selection_summary(), "No selection");
    assert_eq!(app.alignment_count(), 0);
    app.tab.selected = vec![o];
    assert_eq!(app.selection_summary(), "1 atom");
    assert_eq!(app.alignment_count(), 1);
    assert!(!app.can_group());
    app.tab.selected = vec![c, o, point];
    assert_eq!(app.selection_summary(), "2 atoms · 1 point · 1 bond");
    assert_eq!(app.alignment_count(), 2);
    assert!(app.can_group());
    let _ = app.update(Message::Group);
    assert_eq!(app.selection_summary(), "1 group · 1 bond");
    assert_eq!(app.alignment_count(), 1);
    assert!(!app.can_group());
    let _ = app.update(Message::Ungroup);
    assert_eq!(app.selection_summary(), "2 atoms · 1 point · 1 bond");
    assert_eq!(app.alignment_count(), 2);
}

#[test]
fn ring_controls_reject_large_selections_without_dropping_nonchemical_members() {
    let (mut app, _) = App::new();
    app.tab.doc = reshiki::rings::Preset::Benzene.document(42., false);
    app.tab.selected = app.tab.doc.all_ids();
    assert!(app.has_selected_ring());
    for _ in 0..20 {
        let id = app.tab.doc.add_atom("*", Default::default());
        app.tab.selected.push(id);
    }
    assert!(
        app.has_selected_ring(),
        "Dummy points do not change the ring atoms"
    );
    for _ in 0..3 {
        let id = app.tab.doc.add_atom("C", Default::default());
        app.tab.selected.push(id);
    }
    assert!(
        !app.has_selected_ring(),
        "Nine chemical atoms cannot be one supported ring"
    );
}

#[test]
#[ignore = "release-mode inspector benchmark"]
fn inspector_workloads() {
    use std::{hint::black_box, time::Instant};
    fn measure(name: &str, mut work: impl FnMut()) {
        for _ in 0..3 {
            work();
        }
        let mut samples = Vec::new();
        for _ in 0..30 {
            let start = Instant::now();
            work();
            samples.push(start.elapsed().as_secs_f64() * 1000.);
        }
        samples.sort_by(f64::total_cmp);
        println!("{name},{:.4},{:.4}", samples[15], samples[28]);
    }
    println!("workload,median_ms,p95_ms");
    let gallery: Document = serde_json::from_str(include_str!(
        "../../../assets/examples/shortcut-examples.rsk"
    ))
    .unwrap();
    for copies in [1, 4] {
        let (mut app, _) = App::new();
        app.tab.doc = Document::default();
        for copy in 0..copies {
            reshiki::editing::append(
                &mut app.tab.doc,
                &gallery,
                reshiki::document::Point::new(copy as f32 * 1800., 0.),
            );
        }
        let endpoint = app
            .tab
            .doc
            .add_atom("O", reshiki::document::Point::new(-100., -100.));
        app.inspector_open = true;
        app.inspector_tab = InspectorTab::Properties;
        for selection in ["none", "single", "all"] {
            app.tab.selected = match selection {
                "none" => vec![],
                "single" => vec![endpoint],
                _ => app.tab.doc.all_ids(),
            };
            for expanded in [false, true] {
                app.tab
                    .inspector_ui
                    .expanded
                    .insert(Section::Molecule, expanded);
                app.tab
                    .inspector_ui
                    .expanded
                    .insert(Section::Transform, expanded);
                app.tab
                    .inspector_ui
                    .expanded
                    .insert(Section::Groups, expanded);
                let name = format!(
                    "gallery_{copies}x_{selection}_{}",
                    if expanded { "expanded" } else { "collapsed" }
                );
                measure(&format!("{name}_panel"), || {
                    black_box(app.properties_panel());
                });
                measure(&format!("{name}_view"), || {
                    black_box(app.view());
                });
            }
        }
    }
}

async fn calculate(app: &mut App) {
    let key = app.property_request_key();
    let doc = app.property_document(&key);
    let _ = app.inspector_action(Action::RefreshProperties);
    let result = app
        .engine
        .execute(Request::molecule("analyze", doc))
        .await
        .unwrap()
        .analysis
        .unwrap();
    let _ = app.inspector_action(Action::PropertiesCalculated(key, Box::new(Ok(result))));
}

#[tokio::test]
async fn whole_drawing_properties_are_lazy_cached_and_never_modify_labels() {
    let (mut app, _) = App::new();
    app.tab.doc.add_atom("O", Default::default());
    app.tab.selected.clear();
    let before = app.tab.doc.clone();
    let subscriptions =
        |app: &App| iced::advanced::subscription::into_recipes(app.properties_subscription()).len();
    app.inspector_open = false;
    assert_eq!(subscriptions(&app), 0);
    app.inspector_open = true;
    assert_eq!(subscriptions(&app), 1);
    calculate(&mut app).await;
    assert_eq!(app.property_analysis().unwrap().formula, "H2O");
    assert_eq!(app.tab.doc, before);
    assert_eq!(subscriptions(&app), 0);
    app.tab.revision += 1;
    assert!(app.property_analysis().is_none());
}

#[test]
fn cached_property_errors_stop_polling_until_the_request_key_changes() {
    let (mut app, _) = App::new();
    let first = app.tab.doc.add_atom("C", Default::default());
    let second = app
        .tab
        .doc
        .add_atom("O", reshiki::document::Point::new(42., 0.));
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Properties;
    let subscriptions =
        |app: &App| iced::advanced::subscription::into_recipes(app.properties_subscription()).len();
    let key = app.property_request_key();
    assert_eq!(key.atoms, [first, second]);
    app.tab.inspector_ui.properties = Some((key, Err("Cannot analyze this drawing".into())));
    for selected in [vec![], vec![second, first], vec![first, second]] {
        app.tab.selected = selected;
        assert_eq!(app.property_request_key().atoms, [first, second]);
        assert!(app.property_analysis().is_none());
        assert_eq!(subscriptions(&app), 0, "A cached error must not retry");
    }
    app.tab.selected = vec![first];
    assert_eq!(subscriptions(&app), 1, "A new fragment must calculate");
    app.tab.selected = vec![first, second];
    app.tab.revision += 1;
    assert_eq!(subscriptions(&app), 1, "A revised drawing must calculate");
    app.tab.inspector_ui.properties = Some((app.property_request_key(), Err("New error".into())));
    assert_eq!(subscriptions(&app), 0);
    app.tab.file_epoch += 1;
    assert_eq!(subscriptions(&app), 1, "A new file must calculate");
    app.tab.selected = vec![u64::MAX];
    assert_eq!(
        subscriptions(&app),
        0,
        "Artwork is not a molecular fragment"
    );
}

#[test]
fn inner_curve_is_one_undo_step_and_keeps_chemical_orders() {
    let (mut app, _) = App::new();
    let ids = reshiki::editing::ring(
        &mut app.tab.doc,
        reshiki::document::Point::default(),
        5,
        false,
        0.,
    );
    app.tab.selected = ids[..3].to_vec();
    let original = app.tab.doc.clone();
    let _ = app.inspector_action(Action::RingArc);
    assert_eq!(app.tab.doc.bonds.iter().filter(|b| b.ring_arc).count(), 2);
    assert!(app.tab.history.undo(&mut app.tab.doc));
    assert_eq!(app.tab.doc, original);
    assert!(app.tab.history.redo(&mut app.tab.doc));
    assert_eq!(reshiki::ring_arcs::render(&app.tab.doc).primitives.len(), 1);
}

#[tokio::test]
async fn selected_properties_use_only_the_fragment_without_changing_the_drawing() {
    let (mut app, _) = App::new();
    let result = app
        .engine
        .execute(Request::import_smiles("CCO.CN"))
        .await
        .unwrap();
    app.tab.doc = result.document.unwrap();
    app.tab.analysis = result.analysis;
    let whole = app.tab.analysis.as_ref().unwrap().formula.clone();
    let before = app.tab.doc.clone();
    app.tab.selected = app.tab.doc.atoms[..3].iter().map(|a| a.id).collect();
    calculate(&mut app).await;
    assert_eq!(app.property_analysis().unwrap().formula, "C2H6O");
    assert_eq!(app.property_analysis().unwrap().smiles, "CCO");
    app.tab.selected.pop();
    assert!(
        app.property_analysis().is_none(),
        "Old results must disappear immediately"
    );
    calculate(&mut app).await;
    assert_eq!(app.property_analysis().unwrap().formula, "C2H6");
    app.tab.selected.clear();
    assert_eq!(app.property_analysis().unwrap().formula, whole);
    app.tab.selected = vec![u64::MAX];
    assert!(
        app.property_analysis().is_none(),
        "Artwork selection must not show whole-drawing values"
    );
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert_eq!(app.tab.revision, 0);
}

#[tokio::test]
async fn stale_property_results_cannot_replace_a_new_selection_or_document() {
    let (mut app, _) = App::new();
    let result = app
        .engine
        .execute(Request::import_smiles("CCO"))
        .await
        .unwrap();
    app.tab.doc = result.document.unwrap();
    let analysis = result.analysis.unwrap();
    app.tab.selected = vec![app.tab.doc.atoms[0].id];
    let old = app.property_key().unwrap();
    let _ = app.inspector_action(Action::RefreshProperties);
    app.tab.selected = vec![app.tab.doc.atoms[2].id];
    let _ = app.inspector_action(Action::PropertiesCalculated(
        old,
        Box::new(Ok(analysis.clone())),
    ));
    assert!(app.tab.inspector_ui.pending.is_none());
    assert!(app.property_analysis().is_none());
    let old = app.property_key().unwrap();
    let _ = app.inspector_action(Action::RefreshProperties);
    app.tab.file_epoch += 1;
    let _ = app.inspector_action(Action::PropertiesCalculated(old, Box::new(Ok(analysis))));
    assert!(app.property_analysis().is_none());
    assert!(app.tab.inspector_ui.pending.is_none());
}

#[test]
fn aromatic_shortcut_keeps_tool_size_and_selected_ring_topology() {
    let (mut app, _) = App::new();
    for size in 3..=8 {
        let _ = app.update(Message::RingSize(size));
        let _ = app.update(Message::AromaticRing(false));
        let _ = app.update(Message::ToggleAromaticRing);
        assert_eq!(app.ring_size, size);
        assert!(app.aromatic_ring);
        let _ = app.update(Message::ToggleAromaticRing);
        assert_eq!(app.ring_size, size);
        assert!(!app.aromatic_ring);
    }
    app.tab.selected = reshiki::editing::ring(
        &mut app.tab.doc,
        reshiki::document::Point::default(),
        5,
        false,
        5.,
    );
    app.tool = Tool::Select;
    let before = app.tab.doc.clone();
    let _ = app.update(Message::ToggleAromaticRing);
    assert_eq!(app.tab.doc.atoms.len(), 5);
    assert!(app.tab.doc.bonds.iter().all(|b| b.order == 4));
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[test]
fn tilted_ring_and_centroid_are_atomic_undoable_edits() {
    let (mut app, _) = App::new();
    app.tab.selected = reshiki::editing::ring(
        &mut app.tab.doc,
        reshiki::document::Point::new(100., 100.),
        5,
        false,
        5.,
    );
    let planar = app.tab.doc.clone();
    let ring = app.tab.selected.clone();
    let _ = app.update(Message::Transform(Transform::TiltX(60.)));
    let tilted = app.tab.doc.clone();
    assert!(tilted.atoms.iter().any(|a| a.depth.abs() > 1.));
    assert_eq!(tilted.bonds, planar.bonds);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, planar);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, tilted);
    app.tab.selected = ring;
    let _ = app.update(Message::InspectorAction(Action::Centroid));
    assert_eq!(app.tab.doc.atoms.len(), 6);
    let centroid = app.tab.selected[0];
    assert_eq!(app.tab.doc.atom(centroid).unwrap().centroid.len(), 5);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, tilted);
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc.atom(centroid).unwrap().element, "*");
    app.tab.doc.validate().unwrap();
}

#[test]
fn semantic_attachment_creation_is_undoable() -> Result<(), String> {
    for kind in [
        reshiki::attachments::Kind::MultiCenter,
        reshiki::attachments::Kind::Variable,
    ] {
        let (mut app, _) = App::new();
        app.tab.selected = reshiki::editing::ring(
            &mut app.tab.doc,
            reshiki::document::Point::new(100., 100.),
            6,
            true,
            5.,
        );
        let before = app.tab.doc.clone();
        let _ = app.update(Message::InspectorAction(Action::Attachment(kind)));
        let id = *app.tab.selected.first().ok_or("No point selected")?;
        let point = app.tab.doc.atom(id).ok_or("Missing point")?;
        assert_eq!(point.attachment, Some(kind));
        assert_eq!(point.centroid.len(), 6);
        let after = app.tab.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.tab.doc, before);
        let _ = app.update(Message::Redo);
        assert_eq!(app.tab.doc, after);
        app.tab.doc.validate()?;
    }
    Ok(())
}

#[test]
fn a_selection_opens_at_most_one_section() {
    let (mut app, _) = App::new();
    assert_eq!(app.opened_section(), None);
    let ring = reshiki::editing::ring(
        &mut app.tab.doc,
        reshiki::document::Point::default(),
        6,
        false,
        0.,
    );
    let arrow = app.tab.doc.next_id();
    app.tab.doc.arrows.push(reshiki::document::Arrow::new(
        arrow,
        reshiki::document::Point::new(80., 0.),
        reshiki::document::Point::new(160., 0.),
        Default::default(),
        Default::default(),
    ));
    assert_eq!(app.opened_section(), Some(Section::Molecule));
    app.tab.selected = ring.iter().copied().chain([arrow]).collect();
    assert!(app.alignment_count() >= 2);
    assert_eq!(app.opened_section(), Some(Section::Bonds));
    app.tab.selected = vec![ring[0]];
    assert_eq!(app.opened_section(), Some(Section::Atoms));
    app.tab.selected = vec![ring[0], ring[2]];
    assert_eq!(app.opened_section(), None);
    app.tab.selected = vec![arrow];
    assert_eq!(app.opened_section(), None);
}

#[test]
fn inspector_preferences_preserve_drawing_selection_and_history() {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Default::default());
    let b = app
        .tab
        .doc
        .add_atom("N", reshiki::document::Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![a, b];
    let before = app.tab.doc.clone();
    let revision = app.tab.revision;
    for action in [
        Action::Section(Section::Atoms, true),
        Action::Section(Section::Transform, true),
        Action::Figure(FigureFormat::Png),
        Action::Chemical(ChemicalFormat::Cdxml),
    ] {
        let _ = app.update(Message::InspectorAction(action));
    }
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.selected, [a, b]);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
    assert_eq!(app.tab.inspector_ui.figure, FigureFormat::Png);
    assert_eq!(app.tab.inspector_ui.chemical, ChemicalFormat::Cdxml);
    let _ = app.properties_panel();
    let _ = app.export_panel();
}

#[test]
fn figure_menu_lists_vector_formats_before_raster_and_closes_after_a_choice() {
    let kinds: Vec<_> = FigureFormat::ALL.iter().map(|f| f.kind()).collect();
    assert_eq!(kinds.first(), Some(&"Vector"));
    assert_eq!(
        kinds.iter().position(|k| *k == "Raster"),
        Some(kinds.len() - 1)
    );
    let (mut app, _) = App::new();
    let _ = app.update(Message::InspectorAction(Action::FigureMenu(true)));
    assert!(app.tab.inspector_ui.figure_menu);
    let _ = app.export_panel();
    let _ = app.update(Message::InspectorAction(Action::Figure(FigureFormat::Svg)));
    assert!(!app.tab.inspector_ui.figure_menu);
    assert_eq!(app.tab.inspector_ui.figure, FigureFormat::Svg);
    // Leaving the tab by shortcut must not leave it open behind the tab.
    let _ = app.update(Message::InspectorAction(Action::FigureMenu(true)));
    let _ = app.update(Message::Inspector(InspectorTab::Import));
    assert!(!app.tab.inspector_ui.figure_menu);
}
