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
    fn event(
        &mut self,
        app: &App,
        event: iced::Event,
        cursor: iced::mouse::Cursor,
    ) -> (iced::event::Status, Vec<Message>) {
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let (_, statuses) = ui.update(
            &[event],
            cursor,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        self.cache = ui.into_cache();
        (statuses[0], messages)
    }
    fn redraw(&mut self, app: &mut App) {
        let (_, messages) = self.event(
            app,
            iced::Event::Window(iced::window::Event::RedrawRequested(
                std::time::Instant::now(),
            )),
            iced::mouse::Cursor::Unavailable,
        );
        for message in messages {
            let _ = app.update(message);
        }
    }
}

fn finish_prediction(app: &mut App) {
    let key = app.tab.nmr.pending.clone().unwrap();
    let request = engine::prepare(&app.tab.doc, &app.tab.selected).unwrap();
    let report = Arc::new(engine::predict(&request, app.tab.nmr.nucleus).unwrap());
    let _ = app.nmr_action(Action::Finished(key, Ok(report)));
}

fn fully_visible(node: &reshiki::accessibility::Node) {
    let visible = node.visible_bounds.expect("Visible result row");
    for (expected, actual) in [
        (node.bounds.x, visible.x),
        (node.bounds.y, visible.y),
        (node.bounds.width, visible.width),
        (node.bounds.height, visible.height),
    ] {
        assert!(
            (expected - actual).abs() < 0.01,
            "Whole row must fit, allowing rectangle intersection rounding: {node:?}"
        );
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
        let compact = ui.snapshot(&app);
        assert!(
            !compact
                .nodes
                .iter()
                .any(|n| n.id.starts_with("nmr.height."))
        );
        assert_eq!(
            compact
                .nodes
                .iter()
                .find(|n| n.id == "nmr.method")
                .unwrap()
                .expanded,
            Some(false)
        );
        let _ = app.nmr_action(Action::Details);
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

#[tokio::test]
#[ignore = "Opt-in real renderer, viewport and pointer routing check"]
async fn floating_nmr_preserves_canvas_and_owns_only_its_pointer_input() {
    use crate::canvas::{Edit, Tool};
    use iced::{Event, Point as ScreenPoint, Rectangle, Size, mouse};
    for size in [Size::new(940., 620.), Size::new(1280., 820.)] {
        for inspector in [false, true] {
            let mut app = app();
            app.tab.doc = Document::from_json(include_bytes!(
                "../../../tests/fixtures/nmr/ethyl-acetate.rsk"
            ))
            .unwrap();
            app.tab.saved = app.tab.doc.clone();
            app.tab.busy = false;
            app.tab.selected.clear();
            app.inspector_open = inspector;
            app.inspector_tab = super::super::InspectorTab::Properties;
            let mut ui = NmrUi::new(size).await;
            ui.redraw(&mut app);
            let _ = app.update(Message::Fit);
            let camera = app.tab.camera;
            let viewport = app.viewport;
            let drawing = app.tab.doc.clone();
            let revision = app.tab.revision;
            let assert_view = |app: &App| {
                assert_eq!(
                    app.viewport, viewport,
                    "Actual drawing viewport {size:?}/{inspector}"
                );
                assert_eq!(app.tab.camera.zoom, camera.zoom);
                assert_eq!(app.tab.camera.center, camera.center);
                assert!(
                    app.tab.fit_to_view,
                    "A later real window resize still refits"
                );
            };
            let _ = app.nmr_action(Action::Open);
            finish_prediction(&mut app);
            ui.redraw(&mut app);
            assert_view(&app);
            let snapshot = ui.snapshot(&app);
            let node = snapshot
                .nodes
                .iter()
                .find(|n| n.id == "nmr.site.1H.2")
                .unwrap();
            fully_visible(node);
            let point = node.bounds.center();
            app.tool = Tool::Atom;
            for event in [
                mouse::Event::CursorMoved { position: point },
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                let (status, messages) =
                    ui.event(&app, Event::Mouse(event), mouse::Cursor::Available(point));
                assert_eq!(status, iced::event::Status::Captured);
                assert!(
                    !messages.iter().any(|m| matches!(m, Message::Canvas(_))),
                    "Palette click reached canvas: {messages:?}"
                );
                for message in messages {
                    let _ = app.update(message);
                }
            }
            assert_eq!(app.tab.selected, [2]);
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.revision, revision);
            assert!(!app.tab.history.can_undo());

            // Empty palette padding owns wheel and the complete pressed gesture,
            // including a release outside. No hidden drawing pan or atom appears.
            let close = snapshot.nodes.iter().find(|n| n.id == "nmr.close").unwrap();
            let padding = ScreenPoint::new(
                close.bounds.x + close.bounds.width + 2.,
                close.bounds.center_y(),
            );
            for event in [
                mouse::Event::CursorMoved { position: padding },
                mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0., y: -4. },
                },
                mouse::Event::ButtonPressed(mouse::Button::Left),
            ] {
                let (status, messages) =
                    ui.event(&app, Event::Mouse(event), mouse::Cursor::Available(padding));
                assert_eq!(status, iced::event::Status::Captured);
                assert!(messages.is_empty(), "Empty palette area: {messages:?}");
            }
            let outside = ScreenPoint::new(160., 200.);
            for event in [
                mouse::Event::CursorMoved { position: outside },
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                let (status, messages) =
                    ui.event(&app, Event::Mouse(event), mouse::Cursor::Available(outside));
                assert_eq!(status, iced::event::Status::Captured);
                assert!(
                    messages.is_empty(),
                    "Owned gesture escaped palette: {messages:?}"
                );
            }
            assert_view(&app);
            for action in [
                Action::Details,
                Action::Height(360.),
                Action::Collapse,
                Action::Collapse,
                Action::Details,
            ] {
                let _ = app.nmr_action(action);
                ui.redraw(&mut app);
                assert_view(&app);
                for node in ui
                    .snapshot(&app)
                    .nodes
                    .iter()
                    .filter(|n| n.id.starts_with("nmr.") && n.enabled)
                {
                    if let Some(visible) = node.visible_bounds {
                        assert!(Rectangle::with_size(size).contains(visible.position()));
                        assert!(visible.x + visible.width <= size.width);
                        assert!(visible.y + visible.height <= size.height);
                    }
                }
            }
            let _ = app.nmr_action(Action::Predict(Nucleus::C13));
            finish_prediction(&mut app);
            ui.redraw(&mut app);
            assert_view(&app);
            let carbon = ui.snapshot(&app);
            for id in [1, 2, 4, 5] {
                let node = carbon
                    .nodes
                    .iter()
                    .find(|n| n.id == format!("nmr.site.13C.{id}"))
                    .unwrap();
                fully_visible(node);
            }
            let _ = app.nmr_action(Action::Close);
            ui.redraw(&mut app);
            assert_view(&app);
            let _ = app.nmr_action(Action::Open);
            assert!(!app.tab.nmr.details, "Reopening starts compact");
            finish_prediction(&mut app);
            ui.redraw(&mut app);
            assert_view(&app);

            // Outside the palette, the same real wheel event still belongs to
            // the drawing. This verifies the palette is nonmodal.
            let (_, messages) = ui.event(
                &app,
                Event::Mouse(mouse::Event::CursorMoved { position: outside }),
                mouse::Cursor::Available(outside),
            );
            for message in messages {
                let _ = app.update(message);
            }
            let (_, messages) = ui.event(
                &app,
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0., y: -1. },
                }),
                mouse::Cursor::Available(outside),
            );
            assert!(
                messages
                    .iter()
                    .any(|m| matches!(m, Message::Canvas(Edit::Pan(..)))),
                "Outside drawing input blocked: {messages:?}"
            );
            assert_eq!(app.tab.doc, drawing);
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer scrolling check"]
async fn compact_nmr_scrolls_to_later_sites_without_scrolling_the_drawing() {
    use iced::{Event, Size, mouse};
    let mut app = app();
    app.tab.doc = Document::default();
    let mut previous = None;
    for index in 0..10 {
        let id = app
            .tab
            .doc
            .add_atom("C", Point::new(index as f32 * 42., 0.));
        if let Some(previous) = previous {
            app.tab.doc.add_bond(previous, id, 1, "plain");
        }
        previous = Some(id);
    }
    app.tab.selected.clear();
    app.tab.busy = false;
    app.inspector_open = true;
    let mut ui = NmrUi::new(Size::new(940., 620.)).await;
    ui.redraw(&mut app);
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    ui.redraw(&mut app);
    let camera = app.tab.camera;
    let report = app.tab.nmr.result.as_ref().unwrap();
    assert_eq!(report.rows.len(), 10);
    let first = format!("nmr.site.13C.{}", report.rows[0].atom_id);
    let last = format!("nmr.site.13C.{}", report.rows.last().unwrap().atom_id);
    let before = ui.snapshot(&app);
    assert!(
        before
            .nodes
            .iter()
            .find(|n| n.id == last)
            .unwrap()
            .visible_bounds
            .is_none()
    );
    let point = before
        .nodes
        .iter()
        .find(|n| n.id == first)
        .unwrap()
        .bounds
        .center();
    let (status, messages) = ui.event(
        &app,
        Event::Mouse(mouse::Event::WheelScrolled {
            delta: mouse::ScrollDelta::Lines { x: 0., y: -20. },
        }),
        mouse::Cursor::Available(point),
    );
    assert_eq!(status, iced::event::Status::Captured);
    assert!(
        messages.is_empty(),
        "Scrolling table reached drawing: {messages:?}"
    );
    let after = ui.snapshot(&app);
    assert!(
        after
            .nodes
            .iter()
            .find(|n| n.id == last)
            .unwrap()
            .visible_bounds
            .is_some()
    );
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(app.tab.camera.center, camera.center);
}
