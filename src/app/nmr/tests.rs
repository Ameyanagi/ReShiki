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
    fn panel_bounds(&mut self, app: &App) -> iced::Rectangle {
        use iced::advanced::{layout, widget::Tree};
        let mut panel = palette::floating(
            app.nmr_panel(true),
            app.tab.nmr.position,
            Size::new(app.tab.nmr.width, self.size.height),
            !app.tab.nmr.collapsed,
        );
        let mut tree = Tree::new(&panel);
        let node = panel.as_widget_mut().layout(
            &mut tree,
            &self.renderer,
            &layout::Limits::new(iced::Size::ZERO, self.size),
        );
        let child = node.children().first().unwrap();
        iced::Rectangle {
            x: child.bounds().x,
            y: child.bounds().y,
            ..iced::Rectangle::with_size(child.size())
        }
    }
    fn canvas_bounds(&self, app: &App) -> iced::Rectangle {
        use iced::advanced::{Layout, layout, widget::Tree};
        fn canvas(layout: Layout<'_>, size: Size) -> Option<iced::Rectangle> {
            if layout.bounds().size() == size && layout.children().next().is_none() {
                return Some(layout.bounds());
            }
            layout.children().find_map(|child| canvas(child, size))
        }
        // The actual drawing leaf has the size reported by its viewport
        // sensor. Do not infer its origin from toolbar or inspector constants.
        let mut view = app.view();
        let mut tree = Tree::new(view.as_widget());
        let node = view.as_widget_mut().layout(
            &mut tree,
            &self.renderer,
            &layout::Limits::new(self.size, self.size),
        );
        canvas(Layout::new(&node), app.viewport).expect("Actual drawing canvas layout")
    }
    fn resize_glyph(&mut self, app: &App, rect: iced::Rectangle) -> iced::Rectangle {
        use iced::advanced::{Layout, layout, widget::Tree};
        // Inspect the actual final footer glyph, independently of its hit area.
        let mut panel = app.nmr_panel(true);
        let mut tree = Tree::new(&panel);
        let node = panel
            .as_widget_mut()
            .layout(
                &mut tree,
                &self.renderer,
                &layout::Limits::new(iced::Size::ZERO, rect.size()),
            )
            .move_to(rect.position());
        let measured = Layout::new(&node).children().next().unwrap();
        let footer = measured.children().last().unwrap();
        let commands = footer.children().last().unwrap();
        assert_eq!(
            commands.children().len(),
            5,
            "Details, space, Copy, Export, corner"
        );
        commands.children().last().unwrap().bounds()
    }
    fn scroll_results(&mut self, app: &App, lines: f32) {
        use iced::advanced::widget::{Id, Operation, operation::Scrollable};
        #[derive(Default)]
        struct Body(Option<iced::Rectangle>);
        impl Operation for Body {
            fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
                operate(self);
            }
            fn scrollable(
                &mut self,
                id: Option<&Id>,
                bounds: iced::Rectangle,
                _: iced::Rectangle,
                _: iced::Vector,
                _: &mut dyn Scrollable,
            ) {
                if id == Some(&Id::from("nmr-results")) {
                    self.0 = Some(bounds);
                }
            }
        }
        let mut body = Body::default();
        self.operate(app, &mut body);
        let position = body.0.expect("Production results scroll viewport").center();
        let (_, messages) = self.events(
            app,
            &[
                iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }),
                iced::Event::Mouse(iced::mouse::Event::WheelScrolled {
                    delta: iced::mouse::ScrollDelta::Lines { x: 0., y: lines },
                }),
            ],
            iced::mouse::Cursor::Available(position),
        );
        assert!(
            !messages
                .iter()
                .any(|message| matches!(message, Message::Canvas(_)))
        );
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
        let (statuses, messages) = self.events(app, &[event], cursor);
        (statuses[0], messages)
    }
    fn events(
        &mut self,
        app: &App,
        events: &[iced::Event],
        cursor: iced::mouse::Cursor,
    ) -> (Vec<iced::event::Status>, Vec<Message>) {
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let (_, statuses) = ui.update(
            events,
            cursor,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        self.cache = ui.into_cache();
        (statuses, messages)
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
    let Some(key) = app.tab.nmr.pending.clone() else {
        assert!(app.tab.nmr.result.is_some());
        return;
    };
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
        app.tab.nmr.available = size;
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
            ("nmr.mode", "Dock"),
            ("nmr.labels", "Labels"),
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
        let _ = app.nmr_action(Action::Resize(Size::new(320., 180.)));
        assert!(
            !ui.snapshot(&app)
                .nodes
                .iter()
                .find(|n| n.id == "nmr.size.less")
                .unwrap()
                .enabled
        );
        assert!(ui.activate(&app, "nmr.size.less").is_none());
        let _ = app.nmr_action(Action::Resize(Size::new(600., 640.)));
        assert!(ui.activate(&app, "nmr.size.more").is_none());
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
                close.bounds.center_y() + 50.,
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
                Action::Resize(Size::new(600., 640.)),
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
            // Compact windows scroll the plot and table together; every table
            // row must still become fully readable without changing the view.
            ui.scroll_results(&app, -100.);
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
            // the drawing. Reopening retains the resized dimensions, so the
            // point used before resizing may now be inside the palette.
            let previous_probe = outside;
            let canvas = ui.canvas_bounds(&app);
            let panel = ui.panel_bounds(&app);
            let right = (panel.x + panel.width).clamp(canvas.x, canvas.x + canvas.width);
            let bottom = (panel.y + panel.height).clamp(canvas.y, canvas.y + canvas.height);
            let uncovered = [
                Rectangle {
                    width: (panel.x - canvas.x).clamp(0., canvas.width),
                    ..canvas
                },
                Rectangle {
                    x: right,
                    width: canvas.x + canvas.width - right,
                    ..canvas
                },
                Rectangle {
                    height: (panel.y - canvas.y).clamp(0., canvas.height),
                    ..canvas
                },
                Rectangle {
                    y: bottom,
                    height: canvas.y + canvas.height - bottom,
                    ..canvas
                },
            ]
            .into_iter()
            .filter(|rect| rect.width > 0. && rect.height > 0.)
            .max_by(|a, b| (a.width * a.height).total_cmp(&(b.width * b.height)))
            .unwrap_or_else(|| {
                panic!("No uncovered canvas: window={size:?}, inspector={inspector}, canvas={canvas:?}, panel={panel:?}")
            });
            let outside = uncovered.center();
            assert!(
                canvas.contains(outside) && !panel.contains(outside),
                "Invalid wheel probe {outside:?}: canvas={canvas:?}, panel={panel:?}"
            );
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
                "Outside drawing input blocked: window={size:?}, inspector={inspector}, canvas={canvas:?}, panel={panel:?}, probe={outside:?}, previous_probe={previous_probe:?}, previous_probe_inside_panel={}, messages={messages:?}",
                panel.contains(previous_probe)
            );
            assert_view(&app);
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.revision, revision);
            assert!(!app.tab.history.can_undo());
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

fn ethyl_acetate() -> App {
    let (mut app, _) = App::new();
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/nmr/ethyl-acetate.rsk"
    ))
    .unwrap();
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected.clear();
    app.tab.busy = false;
    app
}

#[test]
fn hybrid_docking_preserves_report_and_camera_and_restores_a_hidden_inspector() {
    let mut app = ethyl_acetate();
    app.inspector_open = false;
    app.set_viewport(Size::new(800., 600.));
    let _ = app.update(Message::Fit);
    let camera = app.tab.camera;
    let _ = app.nmr_action(Action::Open);
    finish_prediction(&mut app);
    assert!(!app.tab.nmr.docked && !app.inspector_open);
    assert!(app.tab.nmr.plot && !app.tab.nmr.details && !app.tab.nmr.labels);
    let report = app.tab.nmr.result.clone().unwrap();
    let tsv = engine::to_tsv(&report);
    let doc = app.tab.doc.clone();
    let _ = app.nmr_action(Action::Move(iced::Point::new(180., 160.)));
    let _ = app.nmr_action(Action::Dock);
    assert!(app.inspector_open);
    assert_eq!(app.inspector_tab, InspectorTab::Nmr);
    app.set_viewport(Size::new(500., 600.));
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(app.tab.camera.center, camera.center);
    assert!(app.tab.fit_to_view);
    assert!(Arc::ptr_eq(&report, app.tab.nmr.result.as_ref().unwrap()));
    // Only the dock layout is exempted; a real later resize still fits.
    app.set_viewport(Size::new(600., 450.));
    assert_ne!(app.tab.camera.zoom, camera.zoom);
    let camera = app.tab.camera;
    let _ = app.nmr_action(Action::Undock);
    app.set_viewport(Size::new(900., 450.));
    assert!(!app.inspector_open && !app.tab.nmr.docked);
    assert_eq!(app.tab.nmr.position, Some(iced::Point::new(180., 160.)));
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(engine::to_tsv(app.tab.nmr.result.as_ref().unwrap()), tsv);
    assert_eq!(app.tab.doc, doc);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn assignment_labels_survive_geometry_but_close_and_chemical_edits_clear_them() {
    let mut app = ethyl_acetate();
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    let _ = app.nmr_action(Action::Labels);
    assert_eq!(app.nmr_canvas().unwrap().mode, LabelMode::Atom);
    let _ = app.nmr_action(Action::LabelMode(LabelMode::Both));
    let before = app.tab.doc.clone();
    let ids = app.tab.doc.atoms.iter().map(|a| a.id).collect::<Vec<_>>();
    app.tab.doc.translate(&ids, 40., -20.);
    app.changed(before);
    assert!(app.tab.nmr.labels);
    assert_eq!(app.nmr_canvas().unwrap().mode, LabelMode::Both);
    let _ = app.nmr_action(Action::Close);
    assert!(app.nmr_canvas().is_none());
    let _ = app.nmr_action(Action::Open);
    assert!(!app.tab.nmr.labels && !app.tab.nmr.details);
    assert_eq!(app.tab.nmr.label_mode, LabelMode::Both);
    finish_prediction(&mut app);
    let _ = app.nmr_action(Action::Labels);
    let before = app.tab.doc.clone();
    app.tab.doc.atom_mut(1).unwrap().element = "N".into();
    app.changed(before);
    assert!(app.tab.nmr.result.is_none());
    assert!(!app.tab.nmr.labels);
    assert!(app.nmr_canvas().is_none());
}

#[test]
fn nearby_plot_markers_cycle_distinct_original_atoms_without_editing() {
    let mut app = ethyl_acetate();
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    let doc = app.tab.doc.clone();
    let revision = app.tab.revision;
    for expected in [5, 1, 5] {
        let _ = app.nmr_action(Action::SelectMarker(vec![3, 0]));
        assert_eq!(app.tab.selected, [expected]);
    }
    let _ = app.nmr_action(Action::SelectMarker(vec![usize::MAX]));
    assert_eq!(app.tab.selected, [5]);
    assert_eq!(app.tab.doc, doc);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in real renderer hybrid NMR interaction check"]
async fn hybrid_nmr_docks_drags_resizes_and_links_plot_labels_without_canvas_edits() {
    use crate::canvas::{Edit, Tool};
    use iced::{Event, Rectangle, mouse};
    for size in [Size::new(940., 620.), Size::new(1280., 820.)] {
        for inspector in [false, true] {
            let mut app = ethyl_acetate();
            app.inspector_open = inspector;
            let mut ui = NmrUi::new(size).await;
            ui.redraw(&mut app);
            let _ = app.update(Message::Fit);
            let _ = app.nmr_action(Action::Predict(Nucleus::C13));
            finish_prediction(&mut app);
            ui.redraw(&mut app);
            let camera = app.tab.camera;
            let viewport = app.viewport;
            let drawing = app.tab.doc.clone();
            let revision = app.tab.revision;
            app.tool = Tool::Atom;
            let snapshot = ui.snapshot(&app);
            let marker = snapshot
                .nodes
                .iter()
                .find(|n| n.id == "nmr.marker.13C.4")
                .unwrap();
            assert!(marker.name.contains("170.700 ppm") && marker.name.contains("atom #4"));
            fully_visible(marker);
            let point = marker.bounds.center();
            for event in [
                mouse::Event::CursorMoved { position: point },
                mouse::Event::ButtonPressed(mouse::Button::Left),
                mouse::Event::ButtonReleased(mouse::Button::Left),
            ] {
                let (status, messages) =
                    ui.event(&app, Event::Mouse(event), mouse::Cursor::Available(point));
                assert_eq!(status, iced::event::Status::Captured);
                assert!(!messages.iter().any(|m| matches!(m, Message::Canvas(_))));
                for message in messages {
                    let _ = app.update(message);
                }
            }
            assert_eq!(app.tab.selected, [4]);
            let Message::Nmr(Action::Labels) = ui.activate(&app, "nmr.labels").unwrap() else {
                panic!("Label toggle must be real");
            };
            let _ = app.nmr_action(Action::Labels);
            assert_eq!(app.nmr_canvas().unwrap().mode, LabelMode::Atom);
            let _ = app.nmr_action(Action::LabelMenu(true));
            let Message::Nmr(Action::LabelMode(LabelMode::Shift)) =
                ui.activate(&app, "nmr.labels.ppm").unwrap()
            else {
                panic!("ppm format must be real");
            };
            let _ = app.nmr_action(Action::LabelMode(LabelMode::Shift));
            assert_eq!(
                ui.snapshot(&app)
                    .nodes
                    .iter()
                    .find(|n| n.id == "nmr.labels.mode")
                    .unwrap()
                    .value
                    .as_deref(),
                Some("ppm")
            );
            // Drag the actual non-button title area, then release outside the
            // old palette bounds. All gesture input remains owned by NMR.
            let rect = ui.panel_bounds(&app);
            let press = rect.position() + iced::Vector::new(14., 12.);
            let end = press - iced::Vector::new(100., 50.);
            for (point, event) in [
                (press, mouse::Event::CursorMoved { position: press }),
                (press, mouse::Event::ButtonPressed(mouse::Button::Left)),
                (end, mouse::Event::CursorMoved { position: end }),
                (end, mouse::Event::ButtonReleased(mouse::Button::Left)),
            ] {
                let (status, messages) =
                    ui.event(&app, Event::Mouse(event), mouse::Cursor::Available(point));
                assert_eq!(status, iced::event::Status::Captured);
                assert!(!messages.iter().any(|m| matches!(m, Message::Canvas(_))));
                for message in messages {
                    let _ = app.update(message);
                }
            }
            assert!(app.tab.nmr.position.is_some());
            let moved = ui.panel_bounds(&app);
            assert!(moved.x < rect.x && moved.y <= rect.y);
            let press = iced::Point::new(moved.x + moved.width - 3., moved.y + moved.height - 3.);
            let end = press + iced::Vector::new(60., 30.);
            for (point, event) in [
                (press, mouse::Event::CursorMoved { position: press }),
                (press, mouse::Event::ButtonPressed(mouse::Button::Left)),
                (end, mouse::Event::CursorMoved { position: end }),
                (end, mouse::Event::ButtonReleased(mouse::Button::Left)),
            ] {
                let (status, messages) =
                    ui.event(&app, Event::Mouse(event), mouse::Cursor::Available(point));
                assert_eq!(status, iced::event::Status::Captured);
                assert!(!messages.iter().any(|m| matches!(m, Message::Canvas(_))));
                for message in messages {
                    let _ = app.update(message);
                }
            }
            assert_eq!(app.tab.nmr.width, moved.width + 60.);
            assert!(app.tab.nmr.resized);
            ui.redraw(&mut app);
            assert_eq!(app.viewport, viewport);
            assert_eq!(app.tab.camera.zoom, camera.zoom);
            assert_eq!(app.tab.camera.center, camera.center);
            let _ = app.nmr_action(Action::Dock);
            ui.redraw(&mut app);
            assert_eq!(app.inspector_tab, InspectorTab::Nmr);
            assert_eq!(app.tab.camera.zoom, camera.zoom);
            assert_eq!(app.tab.camera.center, camera.center);
            let dock = ui.snapshot(&app);
            assert_eq!(
                dock.nodes
                    .iter()
                    .filter(|n| n.id == "nmr.site.13C.4")
                    .count(),
                1
            );
            assert_eq!(
                dock.nodes
                    .iter()
                    .filter(|n| n.id == "nmr.marker.13C.4")
                    .count(),
                1
            );
            assert!(matches!(
                ui.activate(&app, "nmr.mode"),
                Some(Message::Nmr(Action::Undock))
            ));
            let _ = app.nmr_action(Action::Undock);
            ui.redraw(&mut app);
            assert_eq!(app.inspector_open, inspector);
            assert_eq!(app.viewport, viewport);
            assert_eq!(app.tab.camera.zoom, camera.zoom);
            assert_eq!(app.tab.camera.center, camera.center);
            let _ = app.nmr_action(Action::Plot);
            assert!(
                !ui.snapshot(&app)
                    .nodes
                    .iter()
                    .any(|n| n.id.starts_with("nmr.marker."))
            );
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
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.revision, revision);
            assert!(!app.tab.history.can_undo());
            // Outside the moved palette, drawing wheel input is still live.
            let point = iced::Point::new(160., 200.);
            let _ = ui.event(
                &app,
                Event::Mouse(mouse::Event::CursorMoved { position: point }),
                mouse::Cursor::Available(point),
            );
            let (_, messages) = ui.event(
                &app,
                Event::Mouse(mouse::Event::WheelScrolled {
                    delta: mouse::ScrollDelta::Lines { x: 0., y: -1. },
                }),
                mouse::Cursor::Available(point),
            );
            assert!(
                messages
                    .iter()
                    .any(|m| matches!(m, Message::Canvas(Edit::Pan(..))))
            );
        }
    }
}

#[test]
fn reopening_from_properties_predicts_the_newly_selected_molecule() {
    let mut app = ethyl_acetate();
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    let _ = app.nmr_action(Action::Close);
    let before = app.tab.doc.clone();
    let other = app.tab.doc.add_atom("C", Point::new(500., 300.));
    app.changed(before);
    assert!(
        app.tab.nmr.result.is_some(),
        "Unrelated second molecule does not invalidate original report"
    );
    app.tab.selected = vec![other];
    let _ = app.nmr_action(Action::Open);
    assert!(app.tab.nmr.result.is_none());
    assert_eq!(app.tab.nmr.pending.as_ref().unwrap().ids, [other]);
    let _ = app.nmr_action(Action::Labels);
    assert!(
        !app.tab.nmr.labels,
        "A pending report has no assignments to show"
    );
}

#[tokio::test]
#[ignore = "Opt-in real renderer per-document dock ownership check"]
async fn tab_switch_restores_dock_and_never_duplicates_floating_nmr_controls() {
    let mut app = ethyl_acetate();
    app.inspector_open = false;
    let mut ui = NmrUi::new(Size::new(1280., 820.)).await;
    ui.redraw(&mut app);
    let _ = app.update(Message::Fit);
    let _ = app.nmr_action(Action::Dock);
    finish_prediction(&mut app);
    ui.redraw(&mut app);
    let docked_id = app.tab.id;
    let dock_camera = app.tab.camera;
    let dock_viewport = app.viewport;
    app.add_tab();
    ui.redraw(&mut app);
    assert!(!app.inspector_open);
    assert!(!app.tab.nmr.docked);
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../tests/fixtures/nmr/ethyl-acetate.rsk"
    ))
    .unwrap();
    app.tab.saved = app.tab.doc.clone();
    let _ = app.nmr_action(Action::Open);
    finish_prediction(&mut app);
    ui.redraw(&mut app);
    let floating_id = app.tab.id;
    let singleton = |snapshot: &reshiki::accessibility::Snapshot| {
        for id in ["nmr.close", "nmr.site.1H.2"] {
            assert_eq!(
                snapshot.nodes.iter().filter(|n| n.id == id).count(),
                1,
                "{id}"
            );
        }
    };
    singleton(&ui.snapshot(&app));
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Select(docked_id)));
    ui.redraw(&mut app);
    assert!(app.inspector_open && app.tab.nmr.docked);
    assert_eq!(app.inspector_tab, InspectorTab::Nmr);
    assert_eq!(app.viewport, dock_viewport);
    assert_eq!(app.tab.camera.zoom, dock_camera.zoom);
    assert_eq!(app.tab.camera.center, dock_camera.center);
    singleton(&ui.snapshot(&app));
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Select(
        floating_id,
    )));
    ui.redraw(&mut app);
    assert!(!app.inspector_open && !app.tab.nmr.docked);
    singleton(&ui.snapshot(&app));
    // A different document may open an ordinary equally wide inspector. A's
    // dock restoration still must not refit its saved larger-view camera.
    let _ = app.update(Message::Inspector(InspectorTab::Properties));
    ui.redraw(&mut app);
    assert_eq!(app.viewport, dock_viewport);
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Select(docked_id)));
    ui.redraw(&mut app);
    assert!(app.tab.fit_to_view);
    assert_eq!(app.inspector_tab, InspectorTab::Nmr);
    assert_eq!(app.viewport, dock_viewport);
    assert_eq!(app.tab.camera.zoom, dock_camera.zoom);
    assert_eq!(app.tab.camera.center, dock_camera.center);
    singleton(&ui.snapshot(&app));
    let _ = app.update(Message::Tabs(super::super::tabs::Action::Select(
        floating_id,
    )));
    ui.redraw(&mut app);
    assert!(app.inspector_open && !app.tab.nmr.docked);
    assert_eq!(app.inspector_tab, InspectorTab::Properties);
    singleton(&ui.snapshot(&app));
    // Defensive view ownership also covers a stale global inspector choice.
    app.inspector_open = true;
    app.inspector_tab = InspectorTab::Nmr;
    singleton(&ui.snapshot(&app));
}

#[tokio::test]
#[ignore = "Opt-in real renderer external gesture termination check"]
async fn drawing_gesture_ending_over_nmr_stops_without_erasing_after_release() {
    use crate::canvas::{Edit, Tool};
    use iced::{Event, mouse};
    for batched in [false, true] {
        let mut app = ethyl_acetate();
        let mut ui = NmrUi::new(Size::new(1280., 820.)).await;
        ui.redraw(&mut app);
        let _ = app.update(Message::Fit);
        let _ = app.nmr_action(Action::Open);
        finish_prediction(&mut app);
        ui.redraw(&mut app);
        app.tool = Tool::Erase;
        let rect = ui.panel_bounds(&app);
        let outside = iced::Point::new(160., 200.);
        let inside = iced::Point::new(rect.x + rect.width - 3., rect.y + 70.);
        let mut ended = 0;
        let events = [
            (outside, mouse::Event::CursorMoved { position: outside }),
            (outside, mouse::Event::ButtonPressed(mouse::Button::Left)),
            (inside, mouse::Event::CursorMoved { position: inside }),
            (inside, mouse::Event::ButtonReleased(mouse::Button::Left)),
        ];
        let batches = if batched {
            vec![(
                events
                    .into_iter()
                    .map(|(_, event)| Event::Mouse(event))
                    .collect::<Vec<_>>(),
                mouse::Cursor::Available(inside),
            )]
        } else {
            events
                .into_iter()
                .map(|(point, event)| (vec![Event::Mouse(event)], mouse::Cursor::Available(point)))
                .collect()
        };
        for (events, cursor) in batches {
            let (_, messages) = ui.events(&app, &events, cursor);
            ended += messages
                .iter()
                .filter(|m| matches!(m, Message::Canvas(Edit::EraseEnd)))
                .count();
            for message in messages {
                let _ = app.update(message);
            }
        }
        assert_eq!(ended, 1);
        let (_, messages) = ui.event(
            &app,
            Event::Mouse(mouse::Event::CursorMoved { position: outside }),
            mouse::Cursor::Available(outside),
        );
        assert!(
            !messages
                .iter()
                .any(|m| matches!(m, Message::Canvas(Edit::EraseTo(..))))
        );
        assert!(!app.tab.erase_stroke);
    }
}
#[tokio::test]
#[ignore = "Opt-in real renderer native mouse batch ordering check"]
async fn native_batched_palette_drags_keep_press_position_and_release_ownership() {
    use iced::{Event, mouse};
    let mut app = ethyl_acetate();
    let mut ui = NmrUi::new(Size::new(1280., 820.)).await;
    ui.redraw(&mut app);
    let _ = app.update(Message::Fit);
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    ui.redraw(&mut app);
    let camera = app.tab.camera;
    let viewport = app.viewport;
    let drawing = app.tab.doc.clone();
    let revision = app.tab.revision;
    for cross_batch in [false, true] {
        for resize in [false, true] {
            app.tab.nmr.position = Some(iced::Point::new(500., 220.));
            app.tab.nmr.width = 400.;
            app.tab.nmr.resized = false;
            ui.redraw(&mut app);
            let rect = ui.panel_bounds(&app);
            let press = if resize {
                iced::Point::new(rect.x + rect.width - 3., rect.y + rect.height - 3.)
            } else {
                rect.position() + iced::Vector::new(14., 12.)
            };
            let delta = if resize {
                iced::Vector::new(60., 30.)
            } else {
                iced::Vector::new(-100., -50.)
            };
            let end = press + delta;
            let hover = Event::Mouse(mouse::Event::CursorMoved { position: press });
            let mut events = vec![];
            if cross_batch {
                let _ = ui.events(&app, &[hover], mouse::Cursor::Available(press));
            } else {
                events.push(hover);
            }
            events.extend([
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::CursorMoved { position: end }),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ]);
            // Match iced_winit: all events share the batch's final cursor.
            let (statuses, messages) = ui.events(&app, &events, mouse::Cursor::Available(end));
            assert!(statuses.iter().all(|s| *s == iced::event::Status::Captured));
            assert!(!messages.iter().any(|m| matches!(m, Message::Canvas(_))));
            assert_eq!(
                messages
                    .iter()
                    .filter(|m| matches!(m, Message::Nmr(Action::Move(_) | Action::Resize(_))))
                    .count(),
                2,
                "Motion and release preserve the same press delta"
            );
            for message in messages {
                let _ = app.update(message);
            }
            if resize {
                assert_eq!(app.tab.nmr.width, rect.width + delta.x);
                assert_eq!(app.tab.nmr.height, rect.height + delta.y);
                assert_eq!(app.tab.nmr.position, Some(rect.position()));
            } else {
                assert_eq!(app.tab.nmr.position, Some(rect.position() + delta));
                assert_eq!(app.tab.nmr.width, rect.width);
            }
            let outside = iced::Point::new(160., 200.);
            let (_, messages) = ui.event(
                &app,
                Event::Mouse(mouse::Event::CursorMoved { position: outside }),
                mouse::Cursor::Available(outside),
            );
            assert!(!messages.iter().any(|m| matches!(m, Message::Nmr(_))));
            ui.redraw(&mut app);
            assert_eq!(app.viewport, viewport);
            assert_eq!(app.tab.camera.zoom, camera.zoom);
            assert_eq!(app.tab.camera.center, camera.center);
            assert_eq!(app.tab.doc, drawing);
            assert_eq!(app.tab.revision, revision);
            assert!(!app.tab.history.can_undo());
        }
    }
    // An owned press still terminates when the current cursor is masked.
    for masked in [
        mouse::Cursor::Unavailable,
        mouse::Cursor::Levitating(iced::Point::ORIGIN),
    ] {
        let rect = ui.panel_bounds(&app);
        let press = rect.position() + iced::Vector::new(14., 12.);
        let _ = ui.events(
            &app,
            &[
                Event::Mouse(mouse::Event::CursorMoved { position: press }),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(press),
        );
        let (status, messages) = ui.event(
            &app,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            masked,
        );
        assert_eq!(status, iced::event::Status::Captured);
        assert!(!messages.iter().any(|m| matches!(m, Message::Canvas(_))));
        let outside = iced::Point::new(160., 200.);
        let (_, messages) = ui.event(
            &app,
            Event::Mouse(mouse::Event::CursorMoved { position: outside }),
            mouse::Cursor::Available(outside),
        );
        assert!(!messages.iter().any(|m| matches!(m, Message::Nmr(_))));
    }
    let rect = ui.panel_bounds(&app);
    let press = rect.position() + iced::Vector::new(14., 12.);
    let _ = ui.events(
        &app,
        &[
            Event::Mouse(mouse::Event::CursorMoved { position: press }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        ],
        mouse::Cursor::Available(press),
    );
    let (status, _) = ui.event(
        &app,
        Event::Window(iced::window::Event::Unfocused),
        mouse::Cursor::Unavailable,
    );
    assert_eq!(status, iced::event::Status::Captured);
    let outside = iced::Point::new(160., 200.);
    let (_, messages) = ui.event(
        &app,
        Event::Mouse(mouse::Event::CursorMoved { position: outside }),
        mouse::Cursor::Available(outside),
    );
    assert!(!messages.iter().any(|m| matches!(m, Message::Nmr(_))));
}

#[tokio::test]
#[ignore = "Opt-in real renderer native child button batch ordering check"]
async fn native_batched_palette_buttons_use_ordered_press_and_release_targets() {
    use crate::canvas::Edit;
    use iced::{Event, mouse};
    let mut app = ethyl_acetate();
    let mut ui = NmrUi::new(Size::new(1280., 820.)).await;
    ui.redraw(&mut app);
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    ui.redraw(&mut app);
    let snapshot = ui.snapshot(&app);
    let point = |id| {
        snapshot
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .bounds
            .center()
    };
    let a = point("nmr.predict.13C");
    let b = point("nmr.mode");
    let outside = iced::Point::new(160., 200.);
    let (statuses, messages) = ui.events(
        &app,
        &[
            Event::Mouse(mouse::Event::CursorMoved { position: a }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            Event::Mouse(mouse::Event::CursorMoved { position: outside }),
        ],
        mouse::Cursor::Available(outside),
    );
    assert_eq!(statuses[1], iced::event::Status::Captured);
    assert_eq!(statuses[2], iced::event::Status::Captured);
    assert_eq!(
        messages
            .iter()
            .filter(|m| matches!(m, Message::Nmr(Action::Predict(Nucleus::C13))))
            .count(),
        1
    );
    assert!(!messages.iter().any(|m| matches!(
        m,
        Message::Nmr(Action::Move(_) | Action::Resize(_) | Action::Dock)
    )));
    assert!(
        !messages
            .iter()
            .any(|m| matches!(m, Message::Canvas(edit) if !matches!(edit, Edit::Hover(_))))
    );
    let (_, messages) = ui.events(
        &app,
        &[
            Event::Mouse(mouse::Event::CursorMoved { position: a }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::CursorMoved { position: b }),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
        mouse::Cursor::Available(b),
    );
    assert!(
        messages.is_empty(),
        "Pressing A and releasing B activates neither: {messages:?}"
    );
}

#[tokio::test]
#[ignore = "Opt-in real renderer visible resize affordance check"]
async fn visible_resize_glyph_drags_without_stealing_footer_controls() {
    use iced::{Event, mouse};
    let mut app = ethyl_acetate();
    let mut ui = NmrUi::new(Size::new(1280., 820.)).await;
    ui.redraw(&mut app);
    let _ = app.update(Message::Fit);
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    ui.redraw(&mut app);
    let camera = app.tab.camera;
    let viewport = app.viewport;
    let drawing = app.tab.doc.clone();
    let revision = app.tab.revision;
    for minimum in [false, true] {
        for corner in [false, true] {
            app.tab.nmr.position = Some(iced::Point::new(500., 220.));
            app.tab.nmr.width = if minimum { 320. } else { 400. };
            app.tab.nmr.height = 180.;
            app.tab.nmr.resized = minimum;
            ui.redraw(&mut app);
            let rect = ui.panel_bounds(&app);
            let glyph = ui.resize_glyph(&app, rect);
            let snapshot = ui.snapshot(&app);
            let export = snapshot
                .nodes
                .iter()
                .find(|n| n.id == "nmr.export")
                .unwrap();
            assert!((glyph.center_y() - export.bounds.center_y()).abs() < 0.01);
            assert!(glyph.x > export.bounds.x + export.bounds.width);
            let press = if corner {
                iced::Point::new(rect.x + rect.width - 3., rect.y + rect.height - 3.)
            } else {
                glyph.center()
            };
            let delta = iced::Vector::new(40., 20.);
            let end = press + delta;
            let (statuses, messages) = ui.events(
                &app,
                &[
                    Event::Mouse(mouse::Event::CursorMoved { position: press }),
                    Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    Event::Mouse(mouse::Event::CursorMoved { position: end }),
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                ],
                mouse::Cursor::Available(end),
            );
            assert!(statuses.iter().all(|s| *s == iced::event::Status::Captured));
            assert_eq!(messages.len(), 2, "Visible grip owns motion and release");
            for message in messages {
                let Message::Nmr(Action::Resize(size)) = message else {
                    panic!("Visible grip must resize, never move or edit canvas");
                };
                assert_eq!(size, Size::new(rect.width + delta.x, rect.height + delta.y));
                let _ = app.nmr_action(Action::Resize(size));
            }
            assert_eq!(app.tab.nmr.position, Some(rect.position()));
            ui.redraw(&mut app);
        }
    }
    // The visible footer controls beside the corner keep their own activation.
    for id in ["nmr.copy", "nmr.export", "nmr.method"] {
        app.tab.nmr.position = Some(iced::Point::new(500., 220.));
        app.tab.nmr.width = 400.;
        app.tab.nmr.resized = false;
        ui.redraw(&mut app);
        let snapshot = ui.snapshot(&app);
        let press = snapshot
            .nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .bounds
            .center();
        let (statuses, messages) = ui.events(
            &app,
            &[
                Event::Mouse(mouse::Event::CursorMoved { position: press }),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(press),
        );
        assert!(statuses.iter().all(|s| *s == iced::event::Status::Captured));
        assert_eq!(messages.len(), 1, "Footer button {id} activates once");
        assert!(matches!(
            (&messages[0], id),
            (Message::Nmr(Action::Copy), "nmr.copy")
                | (Message::Nmr(Action::Export), "nmr.export")
                | (Message::Nmr(Action::Details), "nmr.method")
        ));
    }
    // A grip-owned release terminates even when an overlay masks the cursor.
    for masked in [
        mouse::Cursor::Unavailable,
        mouse::Cursor::Levitating(iced::Point::ORIGIN),
    ] {
        let rect = ui.panel_bounds(&app);
        let press = ui.resize_glyph(&app, rect).center();
        let _ = ui.events(
            &app,
            &[
                Event::Mouse(mouse::Event::CursorMoved { position: press }),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(press),
        );
        let (status, messages) = ui.event(
            &app,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            masked,
        );
        assert_eq!(status, iced::event::Status::Captured);
        assert!(messages.is_empty());
        let outside = iced::Point::new(160., 200.);
        let (_, messages) = ui.event(
            &app,
            Event::Mouse(mouse::Event::CursorMoved { position: outside }),
            mouse::Cursor::Available(outside),
        );
        assert!(!messages.iter().any(|m| matches!(m, Message::Nmr(_))));
    }
    assert_eq!(app.viewport, viewport);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(app.tab.camera.center, camera.center);
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo());
}

#[tokio::test]
#[ignore = "Opt-in real renderer default placement and drawing clearance check"]
async fn default_and_reset_palette_clear_fixture_drawing_without_moving_camera() {
    use iced::Rectangle;
    for inspector in [true, false] {
        for zoom in [1.6, 2.5] {
            let mut app = ethyl_acetate();
            app.inspector_open = inspector;
            app.inspector_tab = InspectorTab::Properties;
            let mut ui = NmrUi::new(Size::new(1280., 820.)).await;
            ui.redraw(&mut app);
            // Keep a deliberate user view; opening NMR must not fit or pan it.
            app.tab.camera = crate::canvas::Camera {
                center: Point::new(282.8, 219.),
                zoom,
            };
            app.tab.fit_to_view = false;
            let camera = app.tab.camera;
            let viewport = app.viewport;
            let drawing = app.tab.doc.clone();
            let selected = app.tab.selected.clone();
            let revision = app.tab.revision;
            let _ = app.nmr_action(Action::Predict(Nucleus::C13));
            finish_prediction(&mut app);
            let _ = app.nmr_action(Action::Labels);
            let report = app.tab.nmr.result.clone().unwrap();
            let export = engine::to_tsv(&report);
            for reset in [false, true] {
                if reset {
                    let position = iced::Point::new(180., 160.);
                    let _ = app.nmr_action(Action::Move(position));
                    ui.redraw(&mut app);
                    assert_eq!(ui.panel_bounds(&app).position(), position);
                    let _ = app.nmr_action(Action::ResetPosition);
                }
                ui.redraw(&mut app);
                assert!(app.tab.nmr.position.is_none());
                let panel = ui.panel_bounds(&app);
                assert!((ui.size.width - panel.x - panel.width - 12.).abs() < 0.01);
                let window = Rectangle::with_size(ui.size);
                assert!(
                    window.contains(panel.position())
                        && window.contains(iced::Point::new(
                            panel.x + panel.width,
                            panel.y + panel.height,
                        ))
                );
                let canvas = ui.canvas_bounds(&app);
                let paper = app.guides.paper(canvas);
                let offset = Vector::new(paper.x, paper.y);
                // Use the production scene's bounds, including atom symbols
                // and bond strokes, projected from the actual canvas origin.
                let (lo, hi) = reshiki::scene::bounds(&reshiki::scene::primitives(&drawing));
                let lo = camera.screen(lo, paper) + offset;
                let hi = camera.screen(hi, paper) + offset;
                let molecule = Rectangle {
                    x: lo.x,
                    y: lo.y,
                    width: hi.x - lo.x,
                    height: hi.y - lo.y,
                };
                assert!(paper.contains(lo) && paper.contains(hi));
                assert!(
                    !panel.intersects(&molecule),
                    "Default/reset palette covers drawing: inspector={inspector}, zoom={zoom}, reset={reset}, panel={panel:?}, molecule={molecule:?}"
                );
                let assignments = crate::canvas::nmr::placements(
                    &drawing,
                    app.nmr_canvas().unwrap(),
                    camera,
                    paper,
                );
                assert_eq!(
                    assignments
                        .iter()
                        .map(|label| label.atom_id)
                        .collect::<Vec<_>>(),
                    [1, 2, 4, 5]
                );
                for label in assignments {
                    let bounds = Rectangle {
                        x: label.bounds.x + offset.x,
                        y: label.bounds.y + offset.y,
                        ..label.bounds
                    };
                    assert!(
                        paper.contains(bounds.position())
                            && paper.contains(iced::Point::new(
                                bounds.x + bounds.width,
                                bounds.y + bounds.height,
                            ))
                    );
                    assert!(
                        !panel.intersects(&bounds),
                        "Default/reset palette covers {}: inspector={inspector}, zoom={zoom}, reset={reset}, panel={panel:?}, label={bounds:?}",
                        label.text
                    );
                }
                let snapshot = ui.snapshot(&app);
                for id in [1, 2, 4, 5] {
                    fully_visible(
                        snapshot
                            .nodes
                            .iter()
                            .find(|node| node.id == format!("nmr.site.13C.{id}"))
                            .unwrap(),
                    );
                }
                assert_eq!(app.tab.camera.zoom, camera.zoom);
                assert_eq!(app.tab.camera.center, camera.center);
                assert!(!app.tab.fit_to_view);
                assert_eq!(app.viewport, viewport);
                assert_eq!(app.tab.doc, drawing);
                assert_eq!(app.tab.selected, selected);
                assert_eq!(app.tab.revision, revision);
                assert!(!app.tab.history.can_undo());
                assert!(Arc::ptr_eq(app.tab.nmr.result.as_ref().unwrap(), &report));
                assert_eq!(engine::to_tsv(app.tab.nmr.result.as_ref().unwrap()), export);
            }
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer mock-aligned measured layout check"]
async fn mock_aligned_panel_measures_rows_and_recovers_small_viewport_scrolling() {
    let mut app = ethyl_acetate();
    let mut ui = NmrUi::new(Size::new(1280., 820.)).await;
    ui.redraw(&mut app);
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    ui.redraw(&mut app);
    let drawing = app.tab.doc.clone();
    let camera = app.tab.camera;
    let revision = app.tab.revision;
    let original = ui.panel_bounds(&app);
    assert!((ui.size.width - original.x - original.width - 12.).abs() < 0.01);
    let snapshot = ui.snapshot(&app);
    for id in [1, 2, 4, 5] {
        fully_visible(
            snapshot
                .nodes
                .iter()
                .find(|n| n.id == format!("nmr.site.13C.{id}"))
                .unwrap(),
        );
        fully_visible(
            snapshot
                .nodes
                .iter()
                .find(|n| n.id == format!("nmr.marker.13C.{id}"))
                .unwrap(),
        );
    }
    assert!(!snapshot.nodes.iter().any(|n| matches!(
        n.id.as_str(),
        "nmr.size.less" | "nmr.size.more" | "nmr.position.reset" | "nmr.plot"
    )));
    for mode in [LabelMode::Atom, LabelMode::Shift, LabelMode::Both] {
        app.tab.nmr.labels = true;
        app.tab.nmr.label_mode = mode;
        assert_eq!(
            ui.panel_bounds(&app).size(),
            original.size(),
            "Labels never insert another row"
        );
    }
    let _ = app.nmr_action(Action::Resize(Size::new(320., 180.)));
    let compact = ui.snapshot(&app);
    let normal_row = compact
        .nodes
        .iter()
        .find(|n| n.id == "nmr.site.13C.1")
        .unwrap();
    fully_visible(normal_row);
    let normal_row_height = normal_row.bounds.height;
    // A long unsupported explanation must be measured as actual wrapped text
    // inside its owning row, rather than relying on the ordinary-row constant.
    let report = Arc::make_mut(app.tab.nmr.result.as_mut().unwrap());
    report.rows[0].statistics = None;
    report.rows[0].limitation = Some("Unsupported site: this atom has no qualified independent reference observations for its molecular environment; inspect the source conditions before interpreting the remaining predicted sites.".into());
    let wrapped = ui.snapshot(&app);
    let row = wrapped
        .nodes
        .iter()
        .find(|n| n.id == "nmr.site.13C.1")
        .unwrap();
    assert!(row.bounds.height > normal_row_height * 2.);
    fully_visible(row);
    let _ = app.nmr_action(Action::Details);
    let detailed = ui.snapshot(&app);
    fully_visible(
        detailed
            .nodes
            .iter()
            .find(|n| n.id == "nmr.site.13C.1")
            .unwrap(),
    );
    let _ = app.nmr_action(Action::Plot);
    let no_plot = ui.snapshot(&app);
    fully_visible(
        no_plot
            .nodes
            .iter()
            .find(|n| n.id == "nmr.site.13C.1")
            .unwrap(),
    );
    assert!(
        !no_plot
            .nodes
            .iter()
            .any(|n| n.id.starts_with("nmr.marker."))
    );

    // In a genuinely short viewport the entire body scrolls, including plot;
    // the fixed controls remain disjoint and the last result becomes readable.
    app.tab.nmr.details = false;
    app.tab.nmr.plot = true;
    app.tab.nmr.resized = false;
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(app.tab.camera.center, camera.center);
    // Exercise both sides of Iced's rounded scroll offset with actual
    // fractional viewport heights. Full visibility tolerance remains 0.01px.
    for height in [500., 500.25, 500.75] {
        ui.size = Size::new(940., height);
        ui.redraw(&mut app);
        let small_camera = app.tab.camera;
        let small = ui.snapshot(&app);
        let labels = small.nodes.iter().find(|n| n.id == "nmr.labels").unwrap();
        let footer = small.nodes.iter().find(|n| n.id == "nmr.copy").unwrap();
        fully_visible(labels);
        fully_visible(footer);
        assert!(labels.bounds.y + labels.bounds.height < footer.bounds.y);
        ui.scroll_results(&app, -100.);
        let scrolled = ui.snapshot(&app);
        fully_visible(
            scrolled
                .nodes
                .iter()
                .find(|n| n.id == "nmr.site.13C.5")
                .unwrap(),
        );
        for id in ["nmr.labels", "nmr.close", "nmr.copy", "nmr.export"] {
            fully_visible(scrolled.nodes.iter().find(|n| n.id == id).unwrap());
        }
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.tab.revision, revision);
        assert_eq!(app.tab.camera.zoom, small_camera.zoom);
        assert_eq!(app.tab.camera.center, small_camera.center);
    }
}

#[tokio::test]
#[ignore = "Opt-in real renderer label checkbox and dropdown ownership check"]
async fn label_checkbox_and_popup_use_real_checked_actions_and_own_batched_clicks() {
    use iced::{Event, mouse};
    let mut app = ethyl_acetate();
    let mut ui = NmrUi::new(Size::new(1280., 820.)).await;
    ui.redraw(&mut app);
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    ui.redraw(&mut app);
    let drawing = app.tab.doc.clone();
    let camera = app.tab.camera;
    let snapshot = ui.snapshot(&app);
    let toggle = snapshot
        .nodes
        .iter()
        .find(|n| n.id == "nmr.labels")
        .unwrap();
    assert_eq!(toggle.checked, Some(false));
    assert!(toggle.name.contains("assignment labels"));
    let point = toggle.bounds.center();
    let (_, messages) = ui.events(
        &app,
        &[
            Event::Mouse(mouse::Event::CursorMoved { position: point }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
        mouse::Cursor::Available(point),
    );
    assert_eq!(messages.len(), 1);
    assert!(matches!(messages[0], Message::Nmr(Action::Labels)));
    let _ = app.update(messages.into_iter().next().unwrap());
    assert_eq!(
        ui.snapshot(&app)
            .nodes
            .iter()
            .find(|n| n.id == "nmr.labels")
            .unwrap()
            .checked,
        Some(true)
    );
    // The actual checkbox retains keyboard focus after its pointer activation.
    let (_, keyboard_messages) = ui.event(
        &app,
        Event::Keyboard(iced::keyboard::Event::KeyPressed {
            key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
            modified_key: iced::keyboard::Key::Named(iced::keyboard::key::Named::Enter),
            physical_key: iced::keyboard::key::Physical::Code(iced::keyboard::key::Code::Enter),
            location: iced::keyboard::Location::Standard,
            modifiers: iced::keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        }),
        mouse::Cursor::Unavailable,
    );
    assert!(matches!(
        keyboard_messages.as_slice(),
        [Message::Nmr(Action::Labels)]
    ));
    for message in keyboard_messages {
        let _ = app.update(message);
    }
    assert!(!app.tab.nmr.labels);
    let message = ui.activate(&app, "nmr.labels").unwrap();
    let _ = app.update(message);
    let snapshot = ui.snapshot(&app);
    let anchor = snapshot
        .nodes
        .iter()
        .find(|node| node.id == "nmr.labels.mode")
        .unwrap()
        .bounds
        .center();
    let outside = iced::Point::new(160., 200.);
    let (_, messages) = ui.events(
        &app,
        &[
            Event::Mouse(mouse::Event::CursorMoved { position: anchor }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            Event::Mouse(mouse::Event::CursorMoved { position: outside }),
        ],
        mouse::Cursor::Available(outside),
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(message, Message::Nmr(Action::LabelMenu(true))))
            .count(),
        1
    );
    for message in messages {
        let _ = app.update(message);
    }
    assert!(app.tab.nmr.label_menu);
    let popup = ui.snapshot(&app);
    // FocusOverlay marks the live menu Foreground: native accessibility must
    // expose its choices, not the background anchor, while it is open.
    assert!(!popup.nodes.iter().any(|node| node.id == "nmr.labels.mode"));
    assert!(ui.activate(&app, "nmr.labels.mode").is_none());
    for (id, checked) in [
        ("nmr.labels.atom", true),
        ("nmr.labels.ppm", false),
        ("nmr.labels.both", false),
    ] {
        let choice = popup.nodes.iter().find(|node| node.id == id).unwrap();
        fully_visible(choice);
        assert_eq!(choice.checked, Some(checked));
    }
    let point = popup
        .nodes
        .iter()
        .find(|n| n.id == "nmr.labels.both")
        .unwrap()
        .bounds
        .center();
    let outside = iced::Point::new(160., 200.);
    let (_, messages) = ui.events(
        &app,
        &[
            Event::Mouse(mouse::Event::CursorMoved { position: point }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            Event::Mouse(mouse::Event::CursorMoved { position: outside }),
        ],
        mouse::Cursor::Available(outside),
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(message, Message::Nmr(Action::LabelMode(LabelMode::Both))))
            .count(),
        1
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, Message::Canvas(edit) if !matches!(edit, crate::canvas::Edit::Hover(_))))
    );
    for message in messages {
        let _ = app.update(message);
    }
    assert_eq!(app.tab.nmr.label_mode, LabelMode::Both);
    assert!(!app.tab.nmr.label_menu);
    let closed = ui.snapshot(&app);
    let anchor = closed
        .nodes
        .iter()
        .find(|node| node.id == "nmr.labels.mode")
        .unwrap();
    fully_visible(anchor);
    assert_eq!(anchor.expanded, Some(false));
    assert_eq!(anchor.value.as_deref(), Some("Both"));
    assert!(!closed.nodes.iter().any(|node| node.id == "nmr.labels.both"));
    assert_eq!(app.tab.doc, drawing);
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(app.tab.camera.center, camera.center);
}

#[tokio::test]
#[ignore = "Opt-in real renderer reset-position corner ownership check"]
async fn opening_and_resetting_palette_preserve_origin_when_resizing_visible_corner() {
    use iced::{Event, mouse};
    let mut app = ethyl_acetate();
    let mut ui = NmrUi::new(Size::new(1280., 820.)).await;
    ui.redraw(&mut app);
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    ui.redraw(&mut app);
    let camera = app.tab.camera;
    for reset in [false, true] {
        if reset {
            let _ = app.nmr_action(Action::ResetPosition);
        }
        assert!(app.tab.nmr.position.is_none());
        let rect = ui.panel_bounds(&app);
        let glyph = ui.resize_glyph(&app, rect);
        // The left side of the visible glyph also belongs to the handle.
        let press = iced::Point::new(glyph.x + 1., glyph.center_y());
        // The default corner is already at the window's right edge. Shrink
        // inward so reachability clamping cannot move the pinned origin.
        let delta = iced::Vector::new(-20., -20.);
        let end = press + delta;
        let (statuses, messages) = ui.events(
            &app,
            &[
                Event::Mouse(mouse::Event::CursorMoved { position: press }),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::CursorMoved { position: end }),
                Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            ],
            mouse::Cursor::Available(end),
        );
        assert!(statuses.iter().all(|s| *s == iced::event::Status::Captured));
        assert!(
            matches!(messages.first(), Some(Message::Nmr(Action::Move(position))) if *position == rect.position())
        );
        assert_eq!(
            messages
                .iter()
                .filter(|m| matches!(m, Message::Nmr(Action::Resize(_))))
                .count(),
            2
        );
        assert!(!messages.iter().any(|m| matches!(m, Message::Canvas(_))));
        for message in messages {
            let _ = app.update(message);
        }
        let resized = ui.panel_bounds(&app);
        assert_eq!(resized.position(), rect.position());
        assert_eq!(
            resized.size(),
            Size::new(rect.width + delta.x, rect.height + delta.y)
        );
    }
    assert_eq!(app.tab.camera.zoom, camera.zoom);
    assert_eq!(app.tab.camera.center, camera.center);
}

#[tokio::test]
#[ignore = "Opt-in real renderer label menu dismissal and foreign-release check"]
async fn label_menu_dismissal_and_foreign_canvas_release_keep_pointer_ownership() {
    use crate::canvas::{Edit, Tool};
    use iced::{Event, keyboard, mouse, touch};
    let mut app = ethyl_acetate();
    let mut ui = NmrUi::new(Size::new(1280., 820.)).await;
    ui.redraw(&mut app);
    let _ = app.nmr_action(Action::Predict(Nucleus::C13));
    finish_prediction(&mut app);
    ui.redraw(&mut app);
    let _ = app.nmr_action(Action::Labels);
    let outside = iced::Point::new(160., 200.);
    for touch_input in [false, true] {
        let _ = app.nmr_action(Action::LabelMenu(true));
        let snapshot = ui.snapshot(&app);
        let inside = snapshot
            .nodes
            .iter()
            .find(|n| n.id == "nmr.labels.both")
            .unwrap()
            .bounds
            .center();
        let events = if touch_input {
            vec![Event::Touch(touch::Event::FingerPressed {
                id: touch::Finger(55),
                position: outside,
            })]
        } else {
            // The press is outside, even though the batch ends inside a choice.
            vec![
                Event::Mouse(mouse::Event::CursorMoved { position: outside }),
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                Event::Mouse(mouse::Event::CursorMoved { position: inside }),
            ]
        };
        let (_, messages) = ui.events(&app, &events, mouse::Cursor::Available(inside));
        assert_eq!(
            messages
                .iter()
                .filter(|message| matches!(message, Message::Nmr(Action::LabelMenu(false))))
                .count(),
            1
        );
        assert!(!messages.iter().any(
            |message| matches!(message, Message::Canvas(edit) if !matches!(edit, Edit::Hover(_)))
        ));
        for message in messages {
            let _ = app.update(message);
        }
        let release = if touch_input {
            Event::Touch(touch::Event::FingerLifted {
                id: touch::Finger(55),
                position: outside,
            })
        } else {
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left))
        };
        let (_, messages) = ui.event(&app, release, mouse::Cursor::Available(outside));
        assert!(!messages.iter().any(
            |message| matches!(message, Message::Canvas(edit) if !matches!(edit, Edit::Hover(_)))
        ));
        assert!(!app.tab.nmr.label_menu);
    }
    let _ = app.nmr_action(Action::LabelMenu(true));
    let (_, messages) = ui.event(
        &app,
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: keyboard::Key::Named(keyboard::key::Named::Escape),
            modified_key: keyboard::Key::Named(keyboard::key::Named::Escape),
            physical_key: keyboard::key::Physical::Code(keyboard::key::Code::Escape),
            location: keyboard::Location::Standard,
            modifiers: keyboard::Modifiers::empty(),
            text: None,
            repeat: false,
        }),
        mouse::Cursor::Unavailable,
    );
    assert!(matches!(
        messages.as_slice(),
        [Message::Nmr(Action::LabelMenu(false))]
    ));
    for message in messages {
        let _ = app.update(message);
    }

    app.tool = Tool::Erase;
    let (_, messages) = ui.events(
        &app,
        &[
            Event::Mouse(mouse::Event::CursorMoved { position: outside }),
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
        ],
        mouse::Cursor::Available(outside),
    );
    for message in messages {
        let _ = app.update(message);
    }
    assert!(app.tab.erase_stroke);
    // Native accessibility can open a menu while a foreign pointer is held.
    let _ = app.nmr_action(Action::LabelMenu(true));
    let snapshot = ui.snapshot(&app);
    let inside = snapshot
        .nodes
        .iter()
        .find(|n| n.id == "nmr.labels.both")
        .unwrap()
        .bounds
        .center();
    let (_, messages) = ui.events(
        &app,
        &[
            Event::Mouse(mouse::Event::CursorMoved { position: inside }),
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
        ],
        mouse::Cursor::Available(inside),
    );
    assert_eq!(
        messages
            .iter()
            .filter(|message| matches!(message, Message::Canvas(Edit::EraseEnd)))
            .count(),
        1
    );
    assert!(
        !messages
            .iter()
            .any(|message| matches!(message, Message::Nmr(Action::LabelMode(_))))
    );
    for message in messages {
        let _ = app.update(message);
    }
    assert!(!app.tab.erase_stroke);
}
