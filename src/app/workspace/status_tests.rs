use super::*;
use iced::advanced::{
    Layout, layout,
    renderer::Headless,
    widget::{Id, Operation, Tree},
};
use iced::{Rectangle, Size};

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
    let renderer = <iced::Renderer as Headless>::new(
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
    for width in [1040., 1280., 1600.] {
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
            view.as_widget_mut()
                .operate(&mut tree, Layout::new(&node), &renderer, &mut bounds);
            let message = bounds.message.unwrap();
            if status == "Ready" {
                assert!(bounds.details.is_none());
            } else {
                let details = bounds.details.expect("visible full-message cue");
                assert!(details.x >= message.x + message.width);
                assert!(details.width > 0. && details.x + details.width <= width);
                assert!(details.y >= 0. && details.y + details.height <= node.size().height);
            }
        }
    }
}
