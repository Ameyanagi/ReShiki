use super::*;
use reshiki::document::Point;
fn app() -> App {
    let (mut app, _) = App::new();
    let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    let c = app.tab.doc.add_atom("O", Point::new(63., 36.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.doc.add_bond(b, c, 1, "plain");
    app.tab.selected = vec![b];
    app
}
#[test]
fn pending_prediction_survives_move_but_chemical_edits_reject_stale_result() {
    let mut app = app();
    let _ = app.nmr_action(Action::Predict(Nucleus::H1));
    let key = app.tab.nmr.pending.clone().unwrap();
    let request = engine::prepare(&app.tab.doc, &app.tab.selected).unwrap();
    let before = app.tab.doc.clone();
    app.tab.doc.atoms[0].position.x += 42.;
    app.changed(before);
    assert_eq!(app.tab.nmr.pending.as_ref(), Some(&key));
    let before = app.tab.doc.clone();
    app.tab.doc.atoms[0].element = "N".into();
    app.changed(before);
    assert!(app.tab.nmr.pending.is_none());
    let report = engine::Report {
        nucleus: Nucleus::H1,
        atom_ids: request.atom_ids,
        fingerprint: request.fingerprint,
        rows: vec![],
        method: String::new(),
        data_version: String::new(),
        conditions: String::new(),
        limitations: String::new(),
        attribution: String::new(),
    };
    let _ = app.nmr_action(Action::Finished(key, Ok(Arc::new(report))));
    assert!(app.tab.nmr.result.is_none());
}
#[test]
fn foreground_or_background_result_never_lands_in_another_tab() {
    use crate::app::tabs::tests::Front;
    let mut app = app();
    let _ = app.nmr_action(Action::Predict(Nucleus::H1));
    let key = app.tab.nmr.pending.clone().unwrap();
    let id = app.tab.id;
    let front = Front::new(&mut app);
    let _ = app.update(Message::Tab(
        id,
        Box::new(Message::Nmr(Action::Finished(
            key,
            Err("Expected dataset diagnostic".into()),
        ))),
    ));
    front.assert_unchanged(&app);
    assert!(app.tabs.background[0].nmr.pending.is_none());
    assert_eq!(
        app.tabs.background[0].nmr.notice.as_deref(),
        Some("Expected dataset diagnostic")
    );
}

struct NmrUi {
    renderer: iced::Renderer,
    cache: iced_runtime::user_interface::Cache,
    size: iced::Size,
}
impl NmrUi {
    async fn new(size: iced::Size) -> Self {
        use iced::advanced::renderer::Headless;
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap(),
            cache: iced_runtime::user_interface::Cache::new(),
            size,
        }
    }
    fn operate<T: 'static>(
        &mut self,
        app: &App,
        operation: &mut dyn iced::advanced::widget::Operation<T>,
    ) {
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        ui.operate(
            &self.renderer,
            &mut iced::advanced::widget::operation::black_box(operation),
        );
        self.cache = ui.into_cache();
    }
    fn snapshot(&mut self, app: &App) -> reshiki::accessibility::Snapshot {
        let mut collect =
            reshiki::accessibility::Collect::new(iced::Rectangle::with_size(self.size));
        self.operate(app, &mut collect);
        let snapshot = collect.snapshot().clone();
        assert!(snapshot.duplicate_ids.is_empty());
        snapshot
    }
    fn activate(&mut self, app: &App, id: &str) -> Option<Message> {
        use iced::advanced::widget::{Operation, operation};
        let mut activate = reshiki::accessibility::Activate::<Message>::new(id);
        self.operate(app, &mut activate);
        match activate.finish() {
            operation::Outcome::Some(message) => Some(message),
            _ => None,
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer/native semantics check"]
async fn rendered_nmr_controls_publish_values_and_live_native_actions() {
    use super::super::InspectorTab;
    use iced::{Rectangle, Size};
    use reshiki::accessibility::{
        Role,
        tree::{NativeTree, Request},
    };
    for size in [Size::new(940., 620.), Size::new(1280., 820.)] {
        let mut app = app();
        app.tab.doc = Document::from_json(include_bytes!(
            "../../../tests/fixtures/nmr/ethyl-acetate.rsk"
        ))
        .unwrap();
        app.tab.selected.clear();
        let _ = app.inspector_action(super::super::inspector::Action::Section(
            super::super::inspector::Section::Molecule,
            true,
        ));
        app.inspector_open = true;
        app.inspector_tab = InspectorTab::Properties;
        app.tab.nmr.available_height = size.height;
        let mut ui = NmrUi::new(size).await;
        let snapshot = ui.snapshot(&app);
        let entry = snapshot
            .nodes
            .iter()
            .find(|n| n.id == "nmr.open")
            .unwrap_or_else(|| panic!("Entry absent from actual UI: {snapshot:?}"));
        assert_eq!(entry.role, Role::Button);
        assert!(entry.enabled && entry.visible_bounds.is_some());
        assert!(entry.name.contains("selected molecule"));
        assert!(matches!(
            ui.activate(&app, "nmr.open"),
            Some(Message::Nmr(Action::Open))
        ));

        // Supply a real observed-data report, using the same completion path as
        // background prediction, without executing an external UI task here.
        app.tab.selected = vec![2];
        let _ = app.nmr_action(Action::Open);
        let key = app.tab.nmr.pending.clone().unwrap();
        let request = engine::prepare(&app.tab.doc, &app.tab.selected).unwrap();
        let report = Arc::new(engine::predict(&request, Nucleus::H1).unwrap());
        let _ = app.nmr_action(Action::Finished(key, Ok(report)));
        let snapshot = ui.snapshot(&app);
        let find = |id: &str| snapshot.nodes.iter().find(|n| n.id == id).unwrap();
        assert_eq!(find("nmr.predict.1H").checked, Some(true));
        assert_eq!(find("nmr.predict.13C").checked, Some(false));
        assert_eq!(find("nmr.collapse").expanded, Some(true));
        let method = find("nmr.method").value.as_deref().unwrap();
        assert!(method.contains(engine::DATA_VERSION));
        assert!(method.contains("HOSE spherical environments"));
        assert!(method.contains("3 of 3 atom-linked sites supported"));
        assert!(method.contains("273–323"));
        assert!(method.contains("Connectivity-only"));
        assert!(method.contains("not a calibrated confidence interval"));
        let site = find("nmr.site.1H.2");
        assert_eq!(site.role, Role::ToggleButton);
        assert_eq!(site.checked, Some(true));
        for text in [
            "¹H",
            "atom #2",
            "unresolved group",
            "4.160 ppm",
            "sphere 3",
            "17 independent references",
        ] {
            assert!(site.name.contains(text), "{text}: {}", site.name);
        }
        assert!(site.value.as_deref().unwrap().contains("CDCl3"));
        let mut native = NativeTree::default();
        let tree = native
            .update(&snapshot, "ReShiki", Rectangle::with_size(size), 2.)
            .unwrap();
        for (id, expected) in [
            ("nmr.predict.1H", "Predict(H1)"),
            ("nmr.predict.13C", "Predict(C13)"),
            ("nmr.collapse", "Collapse"),
            ("nmr.close", "Close"),
            ("nmr.method", "Details"),
            ("nmr.copy", "Copy"),
            ("nmr.export", "Export"),
            ("nmr.site.1H.2", "Select(1)"),
            ("nmr.height.less", "Height(230.0)"),
            ("nmr.height.more", "Height(270.0)"),
        ] {
            let node = find(id);
            assert!(node.enabled && node.visible_bounds.is_some(), "{id}");
            let native_id = tree
                .nodes
                .iter()
                .find(|(_, n)| n.author_id() == Some(id))
                .unwrap()
                .0;
            assert_eq!(
                native.resolve(&accesskit::ActionRequest {
                    action: accesskit::Action::Click,
                    target_tree: accesskit::TreeId::ROOT,
                    target_node: native_id,
                    data: None,
                }),
                Some(Request::Activate(id.into()))
            );
            let Some(Message::Nmr(action)) = ui.activate(&app, id) else {
                panic!("{id}");
            };
            assert_eq!(format!("{action:?}"), expected);
        }
        let _ = app.nmr_action(Action::Height(140.));
        assert!(
            !ui.snapshot(&app)
                .nodes
                .iter()
                .find(|n| n.id == "nmr.height.less")
                .unwrap()
                .enabled
        );
        assert!(ui.activate(&app, "nmr.height.less").is_none());
        let _ = app.nmr_action(Action::Height(360.));
        assert!(ui.activate(&app, "nmr.height.more").is_none());
        let _ = app.nmr_action(Action::Collapse);
        assert_eq!(
            ui.snapshot(&app)
                .nodes
                .iter()
                .find(|n| n.id == "nmr.collapse")
                .unwrap()
                .expanded,
            Some(false)
        );
        assert!(ui.activate(&app, "nmr.copy").is_none());
        let _ = app.nmr_action(Action::Collapse);
        let _ = app.nmr_action(Action::Predict(Nucleus::C13));
        let waiting = ui.snapshot(&app);
        assert!(
            !waiting
                .nodes
                .iter()
                .find(|n| n.id == "nmr.predict.1H")
                .unwrap()
                .enabled
        );
        assert!(ui.activate(&app, "nmr.copy").is_none());
        assert!(
            waiting
                .nodes
                .iter()
                .find(|n| n.id == "nmr.method")
                .unwrap()
                .value
                .as_deref()
                .unwrap()
                .contains("Calculating")
        );

        // Real unsupported CH2 / exchangeable OH metadata is exposed too.
        let mut unsupported = super::tests::app();
        let _ = unsupported.nmr_action(Action::Open);
        let key = unsupported.tab.nmr.pending.clone().unwrap();
        let request = engine::prepare(&unsupported.tab.doc, &unsupported.tab.selected).unwrap();
        let report = Arc::new(engine::predict(&request, Nucleus::H1).unwrap());
        let _ = unsupported.nmr_action(Action::Finished(key, Ok(report)));
        let unsupported_tree = ui.snapshot(&unsupported);
        assert!(
            unsupported_tree
                .nodes
                .iter()
                .any(|n| n.id.starts_with("nmr.site.1H.") && n.name.contains("Unsupported:"))
        );
        assert!(
            unsupported_tree
                .nodes
                .iter()
                .any(|n| n.name.contains("Exchangeable"))
        );
    }
}
