use super::*;
use reshiki::document::Document;

fn placed(figure: &IrFigure) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = Document::default();
    let built = spectrum(&app.tab.doc, figure, Point::new(145., 62.));
    let members = built.group.members.clone();
    built.add_to(&mut app.tab.doc);
    app.tab.saved = app.tab.doc.clone();
    app.set_selection(members);
    app
}

fn sample() -> IrFigure {
    IrFigure::with_defaults(
        "acetic acid".into(),
        "GFN2-xTB".into(),
        vec![[3448., 0.3], [1803., 1.], [1243., 0.36], [657., 0.4]],
        Journal::Acs,
    )
}

fn target(app: &App) -> Target {
    app.tab.ir_figure.target.unwrap()
}

fn send(app: &mut App, action: Action) {
    let _ = app.update(Message::IrFigure(action));
}

fn draft(app: &mut App, field: Field, value: &str) {
    let key = target(app);
    send(app, Action::Draft(key, field, value.into()));
}

fn verify_rebuild(app: &mut App, before: &Document, expected: &IrFigure) {
    assert!(!app.error, "{}", app.status);
    let (actual, group, _) = app.selected_ir_figure().unwrap();
    assert_eq!(actual, expected);
    assert_eq!(app.tab.selected, group.members);
    assert!(!group.integral && group.layout.is_none());
    let old = &before.groups[0];
    assert_eq!(group.id, old.id);
    assert!(
        app.tab
            .doc
            .object_ids()
            .all(|id| !old.members.contains(&id))
    );
    assert!(group.members.iter().all(|&id| id >= before.next_id()));
    let old_top_left = selection_bounds(before, &old.members).unwrap().0;
    let new_top_left = selection_bounds(&app.tab.doc, &group.members).unwrap().0;
    assert!(old_top_left.distance(new_top_left) < 1e-3);
    assert_eq!(app.tab.history.frames(), 1);
    let _ = app.update(Message::Undo);
    assert_eq!(&app.tab.doc, before);
}

#[test]
fn card_syncs_numeric_drafts_and_shows_hidden_peaks_in_order() {
    let figure = IrFigure {
        from: 3900.,
        to: 600.,
        fwhm: 30.5,
        scale: 0.96,
        hidden_labels: vec![1000, 1900],
        ..sample()
    };
    let mut app = placed(&figure);
    assert!(app.ir_figure_card().is_some());
    assert_eq!(app.tab.ir_figure.fields, ["3900", "600", "30.5", "0.960"]);
    assert_eq!(app.tab.ir_figure.peaks, [3310, 1900, 1731, 1193, 1000, 631]);
    let cached_peaks = app.tab.ir_figure.peaks.as_ptr();
    app.set_selection(vec![target(&app).graphic]);
    app.sync_ir_figure();
    assert!(app.ir_figure_card().is_some());
    assert_eq!(app.tab.ir_figure.peaks.as_ptr(), cached_peaks);
}

#[tokio::test]
#[ignore = "Opt-in real widget tree check with a headless GPU renderer"]
async fn ir_figure_accessibility_exposes_controls_and_activates_peak_switches() {
    use iced::advanced::{renderer::Headless, widget::operation::black_box};
    use iced_runtime::{UserInterface, user_interface::Cache};
    use reshiki::accessibility::{Activate, Collect, Role};

    let mut app = placed(&sample());
    let size = iced::Size::new(300., 1000.);
    let mut renderer =
        <iced::Renderer as Headless>::new(iced::Font::default(), iced::Pixels(16.), None)
            .await
            .unwrap();
    let message = {
        let mut ui =
            UserInterface::build(app.properties_panel(), size, Cache::new(), &mut renderer);
        let mut collect = Collect::new(iced::Rectangle::with_size(size));
        ui.operate(&renderer, &mut black_box(&mut collect));
        let snapshot = collect.snapshot();
        assert!(snapshot.duplicate_ids.is_empty());
        let node = |id| snapshot.nodes.iter().find(|node| node.id == id).unwrap();
        assert_eq!(node("ir-figure-from").role, Role::TextInput);
        assert!(!node("ir-figure-from").focused);
        assert_eq!(node("ir-figure-peak-1803").checked, Some(true));
        let mut activate = Activate::<Message>::new("ir-figure-peak-1803");
        ui.operate(&renderer, &mut black_box(&mut activate));
        activate.message().unwrap().clone()
    };
    let _ = app.update(message);
    assert_eq!(app.selected_ir_figure().unwrap().0.hidden_labels, [1803]);
}

#[test]
fn discrete_controls_rebuild_in_place_in_one_undo_step() {
    let figure = sample();
    type UpdateExpected = fn(&mut IrFigure);
    let edits: [(Edit, UpdateExpected); 9] = [
        (Edit::Y(YAxis::Absorbance), |f| f.y = YAxis::Absorbance),
        (Edit::MinorTicks(false), |f| f.minor_ticks = false),
        (Edit::Frame(Frame::Baseline), |f| f.frame = Frame::Baseline),
        (Edit::Shape(BandShape::Gaussian), |f| {
            f.shape = BandShape::Gaussian
        }),
        (Edit::Labels(7), |f| f.labels = 7),
        (Edit::VerticalLabels(true), |f| f.vertical_labels = true),
        (Edit::MethodLine(true), |f| f.method_line = true),
        (Edit::Journal(Journal::Wiley), |f| {
            f.journal = Journal::Wiley
        }),
        (Edit::Columns(Columns::Two), |f| f.columns = Columns::Two),
    ];
    for (edit, expected) in edits {
        let mut app = placed(&figure);
        let before = app.tab.doc.clone();
        let mut next = figure.clone();
        expected(&mut next);
        let key = target(&app);
        send(&mut app, Action::Edit(key, edit));
        verify_rebuild(&mut app, &before, &next);
    }
}

#[test]
fn numeric_apply_validates_each_field_and_commits_the_drafts_together() {
    let figure = sample();
    let mut app = placed(&figure);
    let before = app.tab.doc.clone();
    let key = target(&app);
    for (field, invalid, valid) in [
        (Field::From, "500", "3900"),
        (Field::To, "nan", "480"),
        (Field::Fwhm, "201", "30"),
        (Field::Scale, "inf", "0.965"),
    ] {
        draft(&mut app, field, invalid);
        send(&mut app, Action::Apply(key));
        assert!(app.tab.ir_figure.errors[field.index()].is_some());
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.history.frames(), 0);
        draft(&mut app, field, valid);
    }
    send(&mut app, Action::Apply(key));
    verify_rebuild(
        &mut app,
        &before,
        &IrFigure {
            from: 3900.,
            to: 480.,
            fwhm: 30.,
            scale: 0.965,
            ..figure
        },
    );
}

#[test]
fn hiding_a_peak_removes_only_its_label_and_keeps_its_switch() {
    let figure = IrFigure {
        labels: 3,
        ..sample()
    };
    let mut app = placed(&figure);
    let before = app.tab.doc.clone();
    let key = target(&app);
    send(&mut app, Action::Edit(key, Edit::Peak(1803, false)));
    let texts = |doc: &Document| {
        doc.annotations
            .iter()
            .map(|a| a.text.clone())
            .collect::<Vec<_>>()
    };
    let mut previous = texts(&before);
    previous.retain(|text| text != "1803");
    assert_eq!(texts(&app.tab.doc), previous);
    assert!(app.tab.ir_figure.peaks.contains(&1803));
    let hidden = app.tab.doc.clone();
    verify_rebuild(
        &mut app,
        &before,
        &IrFigure {
            hidden_labels: vec![1803],
            ..figure
        },
    );

    let _ = app.update(Message::Redo);
    app.set_selection(hidden.groups[0].members.clone());
    let key = target(&app);
    send(&mut app, Action::Edit(key, Edit::Peak(1803, true)));
    assert_eq!(texts(&app.tab.doc), texts(&before));
    assert!(app.selected_ir_figure().unwrap().0.hidden_labels.is_empty());
}

#[test]
fn stale_targets_and_ungrouped_figures_cannot_be_edited() {
    let mut app = placed(&sample());
    let key = target(&app);
    let before = app.tab.doc.clone();
    for stale in [
        Target {
            tab: TabId(key.tab.0 + 1),
            ..key
        },
        Target {
            epoch: key.epoch + 1,
            ..key
        },
        Target {
            revision: key.revision + 1,
            ..key
        },
        Target {
            graphic: key.graphic + 1,
            ..key
        },
    ] {
        send(&mut app, Action::Edit(stale, Edit::MethodLine(true)));
    }
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.frames(), 0);
    let _ = app.update(Message::Ungroup);
    let ungrouped = app.tab.doc.clone();
    assert!(app.ir_figure_card().is_none());
    assert!(ungrouped.graphics.iter().any(|g| g.ir.is_some()));
    send(&mut app, Action::Edit(key, Edit::MethodLine(true)));
    assert_eq!(app.tab.doc, ungrouped);
    assert_eq!(app.tab.history.frames(), 1);
}

fn wrap_figure(app: &mut App) -> u64 {
    use super::super::figure_panels;
    let _ = app.update(Message::Figure(figure_panels::Action::Wrap));
    assert!(!app.error, "{}", app.status);
    app.tab
        .doc
        .graphics
        .iter()
        .find(|g| g.panel.is_some())
        .unwrap()
        .id
}

fn verify_hidden_and_inert(app: &mut App, old_target: Target) {
    assert!(app.ir_figure_card().is_none());
    let before = app.tab.doc.clone();
    let frames = app.tab.history.frames();
    let current = app.ir_figure_target(old_target.graphic, old_target.group);
    for key in [old_target, current] {
        send(app, Action::Edit(key, Edit::MinorTicks(false)));
    }
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.frames(), frames);
}

#[test]
fn ungrouping_the_figure_inside_a_panel_leaves_the_panel_untouched() {
    let mut app = placed(&sample());
    let members = app.tab.selected.clone();
    let panel = wrap_figure(&mut app);
    let frame = app
        .tab
        .doc
        .graphics
        .iter()
        .find(|g| g.id == panel)
        .unwrap()
        .clone();
    app.set_selection(members);
    let key = target(&app);
    let _ = app.update(Message::Ungroup);
    assert_eq!(app.tab.doc.groups.len(), 1);
    assert!(app.tab.doc.groups[0].members.contains(&key.graphic));
    verify_hidden_and_inert(&mut app, key);
    assert_eq!(
        app.tab.doc.graphics.iter().find(|g| g.id == panel),
        Some(&frame)
    );
}

#[test]
fn deleting_a_figure_member_hides_the_card() {
    let mut app = placed(&sample());
    let key = target(&app);
    app.set_selection(vec![app.tab.doc.annotations[0].id]);
    let _ = app.update(Message::Delete);
    app.set_selection(vec![key.graphic]);
    assert!(app.tab.doc.groups[0].members.contains(&key.graphic));
    verify_hidden_and_inert(&mut app, key);
}

#[test]
fn dragging_the_curve_out_of_its_group_but_inside_the_panel_hides_the_card() {
    let mut app = placed(&sample());
    let (lo, hi) = selection_bounds(&app.tab.doc, &app.tab.selected).unwrap();
    let dy = hi.y - lo.y + 50.;
    let panel = wrap_figure(&mut app);
    let frame = app
        .tab
        .doc
        .graphics
        .iter_mut()
        .find(|g| g.id == panel)
        .unwrap();
    frame.axis_y.y += dy;
    let key = target(&app);
    app.set_selection(vec![key.graphic]);
    let _ = app.update(Message::Canvas(crate::canvas::Edit::Move(
        vec![key.graphic],
        0.,
        dy,
    )));
    let doc = &app.tab.doc;
    assert!(
        !doc.groups
            .iter()
            .find(|g| g.id == key.group)
            .unwrap()
            .members
            .contains(&key.graphic)
    );
    assert!(
        doc.groups
            .iter()
            .any(|g| g.members.contains(&panel) && g.members.contains(&key.graphic))
    );
    verify_hidden_and_inert(&mut app, key);
}

#[test]
fn rebuilding_the_first_figure_in_a_lasting_row_keeps_its_slot() {
    use super::super::figure_panels;
    use reshiki::{document::Annotation, layouts::Flow};

    let mut app = placed(&sample());
    let neighbor = app.tab.doc.next_id();
    let hi = selection_bounds(&app.tab.doc, &app.tab.selected).unwrap().1;
    app.tab.doc.annotations.insert(
        0,
        Annotation {
            id: neighbor,
            position: hi.offset(100., 0.),
            text: "Neighbor".into(),
            format: Default::default(),
            angle: 0.,
        },
    );
    let members = app.tab.selected.clone();
    app.set_selection([&[neighbor], members.as_slice()].concat());
    let _ = app.update(Message::Figure(figure_panels::Action::Arrange(Flow::Row)));
    let key = target(&app);
    let row = app
        .tab
        .doc
        .groups
        .iter()
        .find(|g| g.layout.is_some())
        .unwrap();
    let row_id = row.id;
    assert_eq!(row.members.first(), Some(&neighbor));
    assert_eq!(row.layout.as_ref().unwrap().order.last(), Some(&neighbor));
    let before = selection_bounds(&app.tab.doc, &members).unwrap().0;

    send(&mut app, Action::Edit(key, Edit::MinorTicks(false)));
    assert!(!app.error, "{}", app.status);
    let (figure, group, _) = app.selected_ir_figure().unwrap();
    assert_eq!(group.id, key.group);
    assert!(!figure.minor_ticks);
    let row = app.tab.doc.groups.iter().find(|g| g.id == row_id).unwrap();
    assert_eq!(
        row.members,
        [&[neighbor], group.members.as_slice()].concat()
    );
    assert_eq!(row.layout.as_ref().unwrap().order.last(), Some(&neighbor));
    let (lo, hi) = selection_bounds(&app.tab.doc, &group.members).unwrap();
    let next = selection_bounds(&app.tab.doc, &[neighbor]).unwrap().0;
    assert!(lo.distance(before) < 1e-3);
    assert!(hi.x < next.x);
}

#[test]
fn stale_ir_edit_leaves_an_inline_text_draft_open() {
    use super::super::inline_text::Action as Inline;
    let mut app = placed(&sample());
    let key = target(&app);
    let _ = app.update(Message::InlineText(Inline::Begin(None, Point::default())));
    let _ = app.update(Message::InlineText(Inline::ReplaceText("Draft".into())));
    let before = app.tab.doc.clone();
    let stale = Target {
        revision: key.revision + 1,
        ..key
    };
    send(&mut app, Action::Edit(stale, Edit::MethodLine(true)));
    assert!(app.tab.inline_changed());
    assert_eq!(app.tab.doc, before);
    assert_eq!(app.tab.history.frames(), 0);
}

#[test]
fn editing_a_duplicate_leaves_the_original_group_and_objects_intact() {
    let mut app = placed(&sample());
    let original = app.tab.doc.clone();
    app.duplicate_selection();
    let before = app.tab.doc.clone();
    let (_, copy, _) = app.selected_ir_figure().unwrap();
    assert_ne!(copy.id, original.groups[0].id);
    let key = target(&app);
    send(&mut app, Action::Edit(key, Edit::Journal(Journal::Rsc)));
    assert_eq!(app.selected_ir_figure().unwrap().0.journal, Journal::Rsc);
    assert_eq!(app.tab.doc.groups[0], original.groups[0]);
    assert!(app.tab.doc.graphics.starts_with(&original.graphics));
    assert!(app.tab.doc.annotations.starts_with(&original.annotations));
    assert_eq!(app.tab.history.frames(), 2);
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
}

#[tokio::test]
async fn caption_copy_reports_the_clipboard_result() {
    use iced::futures::StreamExt;
    let figure = sample();
    let mut app = placed(&figure);
    let caption = figure.caption();
    let task = app.copy_ir_caption_with(|contents| async move {
        assert_eq!(contents, caption);
        Ok(())
    });
    let mut stream = iced_runtime::task::into_stream(task).unwrap();
    let Some(iced_runtime::Action::Output(message)) = stream.next().await else {
        panic!("expected a clipboard receipt");
    };
    let _ = app.update(message);
    assert_eq!(app.status, "Copied the IR caption");
    assert!(!app.error && !app.copy_as_busy);
    assert_eq!(app.tab.history.frames(), 0);
}
