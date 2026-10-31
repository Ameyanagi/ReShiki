use super::*;
use iced::advanced::{
    Layout, layout,
    renderer::Headless,
    widget::{Id, Operation, Tree, operation},
};
use iced::{Rectangle, Size};
use reshiki::document::Document;

#[test]
fn keyboard_drawing_context_hides_the_unpointed_initial_target() {
    use reshiki::document::Point;
    use reshiki::keyboard_drawing::Target;
    let (mut app, _) = App::new();
    app.tab.doc = Document::default();
    let point = Point::new(0., 0.);
    let id = app.tab.doc.add_atom("C", point);
    app.tab.selected.clear();
    app.tab.file_epoch += 1;
    app.sync_keyboard_drawing();
    assert_eq!(app.keyboard_context_summary(), None);
    app.tab
        .keyboard_drawing
        .pointer_target(point, Target::Atom(id), &app.tab.doc, false);
    assert_eq!(app.keyboard_context_summary(), Some(format!("C #{id}")));
}

#[derive(Default)]
struct StatusBounds {
    message: Option<Rectangle>,
    details: Option<Rectangle>,
}
impl Operation for StatusBounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if id == Some(&Id::from("status-message")) {
            self.message = Some(bounds);
        } else if id == Some(&Id::from("status-details")) {
            self.details = Some(bounds);
        }
    }
}

#[tokio::test]
#[ignore = "Opt-in native application renderer for measured status-bar layout"]
async fn long_status_keeps_its_full_message_cue_outside_the_clipped_text() {
    let mut renderer = <iced::Renderer as Headless>::new(
        iced::Font::with_name(reshiki::style::ui_font_family()),
        iced::Pixels(16.),
        None,
    )
    .await
    .unwrap();
    let (mut app, _) = App::new();
    app.recovered.clear();
    let shortcut = super::super::shortcuts::keys(Modifiers::COMMAND, "C");
    let refusal = format!(
        "Could not copy CDXML: CDXML/CDX cannot keep text label “MeO” yet · Try {shortcut} for an editable copy · Save as .rsk keeps everything · Clipboard unchanged"
    );
    for width in [1040., 1279., 1280., 1600.] {
        let mut footer_height = None;
        let mut fit_bounds = None;
        for recovery in [false, true] {
            app.recovered = if recovery {
                vec![reshiki::recovery::Candidate {
                    path: "4294967294-0.json".into(),
                    snapshot: reshiki::recovery::Snapshot {
                        document: Document::default(),
                        source: Some("A long recovered document name.rsk".into()),
                        saved_at: 1792845600,
                    },
                }]
            } else {
                vec![]
            };
            for update in [false, true] {
                // Use a fresh update state for each case without executing a request.
                app.updates = super::super::updates::State::new();
                if update {
                    let _ = app.update_action(super::super::updates::Action::Check(true));
                    let _ = app.update_action(super::super::updates::Action::Checked(
                        1,
                        Ok(reshiki::updates::Release {
                            version: "99.0.0".into(),
                        }),
                    ));
                    assert!(app.updates.available());
                }
                for status in [
                    "Ready",
                    refusal.as_str(),
                    "Copied · Review details\nFull notice",
                ] {
                    app.status = status.into();
                    let mut view = app.status_bar();
                    let mut tree = Tree::new(view.as_widget());
                    let node = view.as_widget_mut().layout(
                        &mut tree,
                        &renderer,
                        &layout::Limits::new(Size::ZERO, Size::new(width, 100.)),
                    );
                    let mut bounds = StatusBounds::default();
                    view.as_widget_mut().operate(
                        &mut tree,
                        Layout::new(&node),
                        &renderer,
                        &mut bounds,
                    );
                    let message = bounds.message.unwrap();
                    if status == "Ready" {
                        assert!(bounds.details.is_none());
                    } else {
                        let details = bounds.details.expect("visible full-message cue");
                        assert!(details.x >= message.x + message.width);
                        assert!(details.width > 0. && details.x + details.width <= width);
                        assert!(
                            details.y >= 0. && details.y + details.height <= node.size().height
                        );
                    }
                    assert_eq!(
                        *footer_height.get_or_insert(node.size().height),
                        node.size().height,
                        "Notifications changed footer height at {width}"
                    );
                    let size = Size::new(width, 100.);
                    let mut ui = iced_runtime::UserInterface::build(
                        app.status_bar(),
                        size,
                        iced_runtime::user_interface::Cache::new(),
                        &mut renderer,
                    );
                    let mut collect =
                        reshiki::accessibility::Collect::new(Rectangle::with_size(size));
                    ui.operate(&renderer, &mut operation::black_box(&mut collect));
                    let nodes = &collect.snapshot().nodes;
                    let mut ids = vec![
                        "status-view",
                        "status-zoom-out",
                        "status-zoom-in",
                        "status-fit",
                    ];
                    if update {
                        ids.push("status-update");
                    }
                    for id in ids {
                        let control = nodes.iter().find(|node| node.id == id).unwrap();
                        let visible = control.visible_bounds.expect("visible footer control");
                        assert!(
                            (visible.width - control.bounds.width).abs() < 0.1
                                && (visible.height - control.bounds.height).abs() < 0.1,
                            "Clipped {id} at {width}: {visible:?} vs {:?}",
                            control.bounds
                        );
                        assert!(visible.x >= 0. && visible.x + visible.width <= width);
                    }
                    let fit = nodes.iter().find(|node| node.id == "status-fit").unwrap();
                    assert_eq!(
                        *fit_bounds.get_or_insert(fit.bounds),
                        fit.bounds,
                        "Notifications moved Fit at {width}"
                    );
                }
            }
        }
    }
}
