use super::*;
use iced::advanced::{renderer::Headless, widget::operation};
use reshiki::document::Document;

fn carbon() -> Report {
    let document = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/nmr/ethyl-acetate.rsk"
    ))
    .unwrap();
    let request = reshiki_io::nmr::prepare(&document, &[]).unwrap();
    reshiki_io::nmr::predict(&request, Nucleus::C13).unwrap()
}

fn app_with_report(report: &Report) -> crate::app::App {
    let mut app = crate::app::App::new().0;
    app.tab.doc = Document::from_json(include_bytes!(
        "../../../../tests/fixtures/nmr/ethyl-acetate.rsk"
    ))
    .unwrap();
    app.tab.saved = app.tab.doc.clone();
    app.tab.selected.clear();
    // Accept the report through the same epoch/fingerprint gate as a completed
    // prediction. Directly assigning result would leave its epoch unset, so
    // the next App::update correctly invalidates that stale fixture.
    let _ = app.update(Message::Nmr(Action::Predict(report.nucleus)));
    let key = app.tab.nmr.pending.clone().expect("Prediction owns a key");
    let report = std::sync::Arc::new(report.clone());
    let _ = app.update(Message::Nmr(Action::Finished(key, Ok(report.clone()))));
    assert!(std::sync::Arc::ptr_eq(
        app.tab.nmr.result.as_ref().expect("Accepted report"),
        &report,
    ));
    assert_eq!(app.tab.nmr.result_epoch, Some(app.tab.file_epoch));
    app
}

fn label_bounds(report: &Report, axis: Axis, group: &Group, width: f32) -> Rectangle {
    let measured =
        (crate::app::text_width(&group_label(report, &group.indices), LABEL_SIZE) + 6.).min(width);
    Rectangle {
        x: (axis.x(group.center, width) - measured / 2.).clamp(0., (width - measured).max(0.)),
        y: group.lane as f32 * LABEL_HEIGHT,
        width: measured,
        height: LABEL_HEIGHT - 1.,
    }
}

fn assert_coverage_and_clear_labels(report: &Report, groups: &[Group], width: f32) {
    let axis = Axis::new(report);
    let mut actual: Vec<_> = groups.iter().flat_map(|g| &g.indices).copied().collect();
    actual.sort();
    let expected: Vec<_> = report
        .rows
        .iter()
        .enumerate()
        .filter_map(|(i, r)| {
            r.statistics
                .as_ref()
                .filter(|s| s.median.is_finite())
                .map(|_| i)
        })
        .collect();
    assert_eq!(
        actual, expected,
        "Every real prediction retains exactly one label owner"
    );
    for (i, group) in groups.iter().enumerate() {
        let bounds = label_bounds(report, axis, group, width);
        assert!(bounds.x >= 0. && bounds.x + bounds.width <= width + 0.01);
        assert!(bounds.y + bounds.height < STICK_TOP);
        for other in groups.iter().skip(i + 1) {
            assert!(
                !bounds.intersects(&label_bounds(report, axis, other, width)),
                "Packed labels overlap at allocated width {width}"
            );
        }
    }
}

#[test]
fn carbon_labels_stay_individual_and_stagger_close_real_shifts() {
    let report = carbon();
    let original = reshiki_io::nmr::to_tsv(&report);
    let axis = Axis::new(&report);
    assert!(axis.x(170.7, 384.) < axis.x(61.05, 384.));
    assert!(axis.x(61.05, 384.) < axis.x(20.9, 384.));
    assert!(axis.x(20.9, 384.) < axis.x(14.2, 384.));
    for width in [240., 284., 304., 384., 460., 584.] {
        let labels = groups(&report, axis, width);
        assert_coverage_and_clear_labels(&report, &labels, width);
        assert_eq!(labels.len(), 4);
        assert!(labels.iter().all(|g| g.indices.len() == 1));
        let label = |id| {
            labels
                .iter()
                .find(|g| report.rows[g.indices[0]].atom_id == id)
                .unwrap()
        };
        let fifth = label(5);
        let first = label(1);
        assert_eq!(fifth.center, 20.9);
        assert_eq!(first.center, 14.2);
        if label_bounds(&report, axis, fifth, width).x
            + label_bounds(&report, axis, fifth, width).width
            > label_bounds(&report, axis, first, width).x
        {
            assert_ne!(fifth.lane, first.lane);
        }
    }
    assert_eq!(reshiki_io::nmr::to_tsv(&report), original);
}

#[test]
fn dense_coincident_and_long_id_reports_keep_every_owner_in_cycleable_groups() {
    let mut report = carbon();
    let prototype = report.rows[0].clone();
    report.rows = (0..128)
        .map(|i| {
            let mut row = prototype.clone();
            row.atom_id = u64::MAX - i;
            row.statistics.as_mut().unwrap().median = 20.9;
            row
        })
        .collect();
    for width in [240., 284., 584.] {
        let labels = groups(&report, Axis::new(&report), width);
        assert_coverage_and_clear_labels(&report, &labels, width);
        assert!(labels.iter().any(|g| g.indices.len() > 1));
        for group in labels.iter().filter(|g| g.indices.len() > 1) {
            assert_eq!(
                group_label(&report, &group.indices),
                format!("{} sites", group.indices.len())
            );
            let description = marker_name(&report, &group.indices);
            assert!(description.len() < 4096);
            assert!(description.contains("additional sites"));
            assert!(description.contains("atom-linked result rows"));
        }
    }
    // A succession of nearby shifts across the full plot should not become
    // one giant single-link cluster merely because neighbors are <46px apart.
    for (i, row) in report.rows.iter_mut().take(12).enumerate() {
        row.atom_id = i as u64 + 1;
        row.statistics.as_mut().unwrap().median = 190. - i as f64 * 16.;
    }
    report.rows.truncate(12);
    let labels = groups(&report, Axis::new(&report), 384.);
    assert_coverage_and_clear_labels(&report, &labels, 384.);
    assert_eq!(labels.len(), 12);
}

#[test]
fn missing_data_and_nonfinite_values_never_become_zero_shift_markers() {
    let mut report = carbon();
    report.rows[0].statistics = None;
    report.rows[1].statistics.as_mut().unwrap().median = f64::NAN;
    let axis = Axis::new(&report);
    let labels = groups(&report, axis, 384.);
    assert_coverage_and_clear_labels(&report, &labels, 384.);
    let plot = Plot {
        report: &report,
        selected: &[],
        axis,
        children: vec![],
        positions: vec![],
    };
    assert!(
        plot.hit(
            Point::new(axis.x(0., 384.), 60.),
            Rectangle::with_size(Size::new(384., HEIGHT))
        )
        .is_empty()
    );
}

#[test]
fn extreme_finite_shifts_keep_axis_and_packed_positions_finite() {
    let mut report = carbon();
    for (row, shift) in report
        .rows
        .iter_mut()
        .zip([f64::MAX, f64::MAX, f64::MAX, -f64::MAX])
    {
        row.statistics.as_mut().unwrap().median = shift;
    }
    let axis = Axis::new(&report);
    assert!(axis.high.is_finite() && axis.low.is_finite());
    for row in &report.rows {
        let x = axis.x(row.statistics.as_ref().unwrap().median, 284.);
        assert!(x.is_finite() && (MARGIN..=284. - MARGIN).contains(&x));
    }
    let labels = groups(&report, axis, 284.);
    assert_coverage_and_clear_labels(&report, &labels, 284.);
    assert!(labels.iter().all(|group| group.center.is_finite()));
}

struct Ui {
    renderer: Renderer,
    tree: Tree,
    bounds: Rectangle,
}
impl Ui {
    async fn new() -> Self {
        Self {
            renderer: <Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .expect("Actual renderer is required"),
            tree: Tree::empty(),
            bounds: Rectangle::with_size(Size::new(384., HEIGHT)),
        }
    }
    fn layout(&mut self, view: &mut Element<'_, Message>) -> layout::Node {
        self.tree.diff(view.as_widget());
        view.as_widget_mut().layout(
            &mut self.tree,
            &self.renderer,
            &layout::Limits::new(self.bounds.size(), self.bounds.size()),
        )
    }
    fn snapshot(&mut self, view: &mut Element<'_, Message>) -> reshiki::accessibility::Snapshot {
        let node = self.layout(view);
        let mut collect = reshiki::accessibility::Collect::new(self.bounds);
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &mut operation::black_box(&mut collect),
        );
        collect.snapshot().clone()
    }
    fn activate(&mut self, view: &mut Element<'_, Message>, id: &str) -> Message {
        use iced::advanced::widget::Operation as _;
        let node = self.layout(view);
        let mut activate = reshiki::accessibility::Activate::<Message>::new(id);
        view.as_widget_mut().operate(
            &mut self.tree,
            Layout::new(&node),
            &self.renderer,
            &mut operation::black_box(&mut activate),
        );
        let operation::Outcome::Some(message) = activate.finish() else {
            panic!("Live accessible marker {id}");
        };
        message
    }
    fn event(
        &mut self,
        view: &mut Element<'_, Message>,
        event: Event,
        point: Point,
    ) -> Vec<Message> {
        let node = self.layout(view);
        let mut messages = Vec::new();
        let mut shell = Shell::new(&mut messages);
        view.as_widget_mut().update(
            &mut self.tree,
            &event,
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut shell,
            &self.bounds,
        );
        assert!(shell.is_event_captured(), "Marker owns its actual event");
        messages
    }
    fn pixels(&mut self, view: &mut Element<'_, Message>, theme: &Theme) -> Vec<u8> {
        let node = self.layout(view);
        self.renderer.reset(self.bounds);
        view.as_widget().draw(
            &self.tree,
            &mut self.renderer,
            theme,
            &renderer::Style::default(),
            Layout::new(&node),
            mouse::Cursor::Unavailable,
            &self.bounds,
        );
        Headless::screenshot(
            &mut self.renderer,
            Size::new(self.bounds.width as u32, HEIGHT as u32),
            1.,
            theme.palette().background,
        )
    }
    fn reference_carbon_sticks(
        &mut self,
        report: &Report,
        selected: &[u64],
        theme: &Theme,
    ) -> Vec<u8> {
        self.renderer.reset(self.bounds);
        // Independent reference: the fixture's real shifts use the approved
        // 200→0 ppm axis, 16px margins and equal-height teal sticks. Rendering
        // known ink with this backend includes subpixel coverage and its
        // linear-light compositing instead of assuming an opaque center pixel.
        let color = match theme {
            Theme::Light => Color::from_rgb8(19, 135, 116),
            Theme::Dark => Color::from_rgb8(120, 236, 217),
            _ => panic!("Reference covers the two accepted panel themes"),
        };
        for row in &report.rows {
            let shift = row.statistics.as_ref().unwrap().median;
            assert!((0. ..=200.).contains(&shift));
            let center = 16. + ((200. - shift) / 200.) as f32 * (self.bounds.width - 32.);
            let width = if selected.contains(&row.atom_id) {
                2.5
            } else {
                1.5
            };
            self.renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: center - width / 2.,
                        y: 38.,
                        width,
                        height: 42.,
                    },
                    ..Default::default()
                },
                color,
            );
        }
        Headless::screenshot(
            &mut self.renderer,
            Size::new(self.bounds.width as u32, HEIGHT as u32),
            1.,
            theme.palette().background,
        )
    }
}

#[tokio::test]
#[ignore = "Opt-in actual allocated-width plot, pixels and accessible interaction"]
async fn actual_plot_has_four_teal_sticks_separate_labels_and_preserves_history() {
    let report = carbon();
    let mut app = app_with_report(&report);
    let accepted = app.tab.nmr.result.as_ref().unwrap().clone();
    app.tab.selected = vec![2];
    let original = app.tab.doc.file_json().unwrap();
    let svg = reshiki::scene::svg(&app.tab.doc);
    let revision = app.tab.revision;
    let tsv = reshiki_io::nmr::to_tsv(&report);
    let mut ui = Ui::new().await;
    for width in [284., 460., 304., 584.] {
        ui.bounds = Rectangle::with_size(Size::new(width, HEIGHT));
        let mut plot = view(&report, &app.tab.selected);
        let snapshot = ui.snapshot(&mut plot);
        assert!(snapshot.duplicate_ids.is_empty());
        assert_eq!(snapshot.nodes.len(), 4);
        for (i, row) in report.rows.iter().enumerate() {
            let id = format!("nmr.marker.13C.{}", row.atom_id);
            let node = snapshot.nodes.iter().find(|n| n.id == id).unwrap();
            let shift = row.statistics.as_ref().unwrap().median;
            assert!(node.name.contains(&format!("{shift:.3} ppm")));
            assert_eq!(node.checked, Some(row.atom_id == 2));
            assert!((node.bounds.center_x() - Axis::new(&report).x(shift, width)).abs() < 0.1);
            let Message::Nmr(Action::SelectMarker(indices)) = ui.activate(&mut plot, &id) else {
                panic!("Marker route");
            };
            assert_eq!(indices, [i]);
        }
        for (i, node) in snapshot.nodes.iter().enumerate() {
            for other in snapshot.nodes.iter().skip(i + 1) {
                assert!(!node.bounds.intersects(&other.bounds));
            }
        }
        for theme in [Theme::Light, Theme::Dark] {
            let pixels = ui.pixels(&mut plot, &theme);
            let reference = ui.reference_carbon_sticks(&report, &app.tab.selected, &theme);
            for row in &report.rows {
                let shift = row.statistics.as_ref().unwrap().median;
                let x = 16. + ((200. - shift) / 200.) as f32 * (width - 32.);
                for y in 40..78 {
                    for column in (x.floor() as usize - 2)..=(x.ceil() as usize + 2) {
                        let offset = (y * width as usize + column) * 4;
                        let rgb = &pixels[offset..offset + 3];
                        let expected = &reference[offset..offset + 3];
                        assert!(
                            rgb.iter().zip(expected).all(|(&a, &b)| a.abs_diff(b) <= 1),
                            "Real stick #{} at {shift} ppm in {theme:?}, width {width}, pixel ({column},{y}) must match known teal ink: {rgb:?} vs {expected:?}",
                            row.atom_id
                        );
                    }
                }
            }
        }
        let fifth = snapshot
            .nodes
            .iter()
            .find(|n| n.id == "nmr.marker.13C.5")
            .unwrap();
        let point = fifth.bounds.center();
        assert!(
            ui.event(
                &mut plot,
                Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                point
            )
            .is_empty()
        );
        let messages = ui.event(
            &mut plot,
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
            point,
        );
        drop(plot);
        assert_eq!(messages.len(), 1);
        for message in messages {
            let _ = app.update(message);
        }
        assert_eq!(app.tab.selected, [5]);
        assert!(std::sync::Arc::ptr_eq(
            app.tab
                .nmr
                .result
                .as_ref()
                .expect("Marker keeps prediction"),
            &accepted,
        ));
        app.tab.selected = vec![2];
    }
    assert_eq!(app.tab.doc.file_json().unwrap(), original);
    assert_eq!(reshiki::scene::svg(&app.tab.doc), svg);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo() && !app.tab.history.can_redo());
    assert_eq!(
        reshiki_io::nmr::to_tsv(app.tab.nmr.result.as_ref().unwrap()),
        tsv
    );
}

#[tokio::test]
#[ignore = "Opt-in actual 128-site plot snapshot accepted by native accessibility"]
async fn maximum_dense_report_preserves_native_accessibility_and_all_marker_actions() {
    let mut report = carbon();
    let prototype = report.rows[0].clone();
    report.rows = (1..=128)
        .map(|id| {
            let mut row = prototype.clone();
            row.atom_id = id;
            row.statistics.as_mut().unwrap().median = 20.9;
            row
        })
        .collect();
    let mut ui = Ui::new().await;
    for width in [284., 584.] {
        ui.bounds = Rectangle::with_size(Size::new(width, HEIGHT));
        let mut plot = view(&report, &[]);
        let snapshot = ui.snapshot(&mut plot);
        assert!(snapshot.duplicate_ids.is_empty());
        reshiki::accessibility::tree::NativeTree::default()
            .update(&snapshot, "NMR prediction", ui.bounds, 1.)
            .expect("A dense cluster must not revoke the complete native accessibility tree");
        let mut owners = Vec::new();
        for node in &snapshot.nodes {
            let Message::Nmr(Action::SelectMarker(indices)) = ui.activate(&mut plot, &node.id)
            else {
                panic!("Every marker retains its live cycle action");
            };
            owners.extend(indices);
        }
        owners.sort();
        assert_eq!(owners, (0..128).collect::<Vec<_>>());
    }
}

#[tokio::test]
#[ignore = "Opt-in actual dense accessible marker cycling"]
async fn dense_marker_keeps_all_distinct_original_atoms_accessible_and_cycleable() {
    let mut report = carbon();
    for row in &mut report.rows {
        row.statistics.as_mut().unwrap().median = 20.9;
    }
    let mut ui = Ui::new().await;
    let mut app = app_with_report(&report);
    let accepted = app.tab.nmr.result.as_ref().unwrap().clone();
    let original = app.tab.doc.file_json().unwrap();
    let svg = reshiki::scene::svg(&app.tab.doc);
    let tsv = reshiki_io::nmr::to_tsv(&report);
    let revision = app.tab.revision;
    let group = groups(&report, Axis::new(&report), ui.bounds.width)
        .into_iter()
        .find(|g| g.indices.len() > 1)
        .unwrap();
    let id = format!("nmr.marker.13C.{}", report.rows[group.indices[0]].atom_id);
    for index in group.indices.iter().chain(group.indices.iter().take(1)) {
        let mut plot = view(&report, &app.tab.selected);
        let snapshot = ui.snapshot(&mut plot);
        let node = snapshot.nodes.iter().find(|n| n.id == id).unwrap();
        for &owner in &group.indices {
            assert!(
                node.name
                    .contains(&format!("atom #{}", report.rows[owner].atom_id))
            );
        }
        let message = ui.activate(&mut plot, &id);
        drop(plot);
        let _ = app.update(message);
        assert_eq!(app.tab.selected, [report.rows[*index].atom_id]);
        assert!(std::sync::Arc::ptr_eq(
            app.tab
                .nmr
                .result
                .as_ref()
                .expect("Cycling keeps prediction"),
            &accepted,
        ));
    }
    assert_eq!(app.tab.doc.file_json().unwrap(), original);
    assert_eq!(reshiki::scene::svg(&app.tab.doc), svg);
    assert_eq!(reshiki_io::nmr::to_tsv(&accepted), tsv);
    assert_eq!(app.tab.revision, revision);
    assert!(!app.tab.history.can_undo() && !app.tab.history.can_redo());
}
