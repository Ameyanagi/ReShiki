//! Real Export popup layout and focus at the default and compact window sizes.
use super::{GAP, MARGIN};
use crate::app::{
    App, InspectorTab, Message,
    inspector::{Action, ChemicalFormat, Section},
};
use iced::advanced::{
    renderer::Headless,
    widget::{Operation, operation},
};
use iced::keyboard::{
    self, Key, Modifiers,
    key::{Code, Named, Physical},
};
use iced::{Event, Rectangle, Size, mouse};
use iced_runtime::{UserInterface, user_interface::Cache};
use reshiki::accessibility::{Collect, FocusControl, Node, Snapshot};
use reshiki::document::Point;

const IDS: [&str; 4] = [
    "chemical-format-mol",
    "chemical-format-smiles",
    "chemical-format-inchi",
    "chemical-format-cdxml",
];

struct Ui {
    renderer: iced::Renderer,
    cache: Cache,
    size: Size,
}

impl Ui {
    async fn new(size: Size) -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .expect("headless renderer"),
            cache: Cache::new(),
            size,
        }
    }

    fn snapshot(&mut self, app: &App) -> Snapshot {
        let mut ui = UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut collect = Collect::new(Rectangle::with_size(self.size));
        ui.operate(&self.renderer, &mut operation::black_box(&mut collect));
        self.cache = ui.into_cache();
        let snapshot = collect.snapshot().clone();
        assert!(snapshot.duplicate_ids.is_empty());
        snapshot
    }

    fn focus(&mut self, app: &App, id: &str) {
        let mut ui = UserInterface::build(
            app.view(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut focus: Box<dyn Operation> = Box::new(FocusControl::new(id));
        loop {
            ui.operate(&self.renderer, focus.as_mut());
            match focus.finish() {
                operation::Outcome::Chain(next) => focus = next,
                _ => break,
            }
        }
        self.cache = ui.into_cache();
        let snapshot = self.snapshot(app);
        assert!(find(&snapshot, id).focused);
    }

    fn event(&mut self, app: &App, event: Event) -> (iced::event::Status, Vec<Message>) {
        self.event_at(app, event, mouse::Cursor::Unavailable)
    }

    fn event_at(
        &mut self,
        app: &App,
        event: Event,
        cursor: mouse::Cursor,
    ) -> (iced::event::Status, Vec<Message>) {
        let mut ui = UserInterface::build(
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

    fn key(&mut self, app: &App, named: Named) -> Vec<Message> {
        let code = match named {
            Named::Tab => Code::Tab,
            Named::Enter => Code::Enter,
            Named::Escape => Code::Escape,
            _ => unreachable!(),
        };
        let (status, messages) = self.event(
            app,
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(named),
                modified_key: Key::Named(named),
                physical_key: Physical::Code(code),
                location: keyboard::Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            }),
        );
        assert_eq!(status, iced::event::Status::Captured);
        assert!(
            self.event(
                app,
                Event::Keyboard(keyboard::Event::KeyReleased {
                    key: Key::Named(named),
                    modified_key: Key::Named(named),
                    physical_key: Physical::Code(code),
                    location: keyboard::Location::Standard,
                    modifiers: Modifiers::empty(),
                }),
            )
            .1
            .is_empty()
        );
        messages
    }
}

fn find<'a>(snapshot: &'a Snapshot, id: &str) -> &'a Node {
    snapshot.nodes.iter().find(|node| node.id == id).unwrap()
}

fn close(a: f32, b: f32) {
    assert!((a - b).abs() < 0.02, "{a} differs from {b}");
}

fn fully_visible(node: &Node, size: Size) {
    let bounds = node.bounds;
    assert!(
        bounds.width > 100. && bounds.height >= 24.,
        "{}: {bounds:?}",
        node.id
    );
    assert!(
        bounds.x >= MARGIN && bounds.y >= MARGIN,
        "{}: {bounds:?}",
        node.id
    );
    assert!(bounds.x + bounds.width <= size.width - MARGIN + 0.02);
    assert!(bounds.y + bounds.height <= size.height - MARGIN + 0.02);
    let visible = node.visible_bounds.expect("every menu row is visible");
    close(bounds.x, visible.x);
    close(bounds.y, visible.y);
    close(bounds.width, visible.width);
    close(bounds.height, visible.height);
}

fn apply(app: &mut App, messages: Vec<Message>) {
    assert_eq!(messages.len(), 1);
    for message in messages {
        let _ = app.update(message);
    }
}

#[tokio::test]
#[ignore = "Opt-in real Export popup placement and keyboard regression"]
async fn chemical_export_choices_fit_without_shrinking_and_keep_keyboard_actions() {
    let mut regular_rows = Vec::new();
    for size in [Size::new(1280., 820.), Size::new(1040., 680.)] {
        let mut ui = Ui::new(size).await;
        let (mut app, _) = App::new();
        app.viewport = size;
        app.tab.busy = false;
        let a = app.tab.doc.add_atom("C", Point::new(0., 0.));
        let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
        let c = app.tab.doc.add_atom("O", Point::new(84., 0.));
        app.tab.doc.add_bond(a, b, 1, "plain");
        app.tab.doc.add_bond(b, c, 1, "plain");
        app.tab.saved = app.tab.doc.clone();
        let drawing = app.tab.doc.clone();
        let revision = app.tab.revision;
        let _ = app.update(Message::Inspector(InspectorTab::Export));
        for section in [Section::ExportFigure, Section::ExportChemical] {
            let _ = app.update(Message::InspectorAction(Action::Section(section, true)));
        }
        ui.focus(&app, "export-chemical-format");
        let anchor = find(&ui.snapshot(&app), "export-chemical-format").bounds;
        let messages = ui.key(&app, Named::Enter);
        assert!(matches!(
            messages.as_slice(),
            [Message::InspectorAction(Action::ChemicalMenu(true))]
        ));
        apply(&mut app, messages);
        let popup = ui.snapshot(&app);
        assert_eq!(
            popup
                .nodes
                .iter()
                .map(|node| node.id.as_str())
                .collect::<Vec<_>>(),
            IDS
        );
        for (index, id) in IDS.iter().enumerate() {
            let node = find(&popup, id);
            assert!(node.enabled);
            fully_visible(node, size);
            if size.height == 820. {
                regular_rows.push(node.bounds.size());
            } else {
                close(node.bounds.width, regular_rows[index].width);
                close(node.bounds.height, regular_rows[index].height);
            }
        }
        let first = find(&popup, IDS[0]).bounds;
        let last = find(&popup, IDS[3]).bounds;
        if size.height == 820. {
            // Keep the existing below-anchor gap and five-pixel surface inset
            // when the complete popup already fits there.
            close(first.y, anchor.y + anchor.height + GAP + 5.);
        } else {
            assert!(last.y + last.height <= anchor.y - GAP);
        }
        for id in IDS {
            assert!(ui.key(&app, Named::Tab).is_empty());
            let snapshot = ui.snapshot(&app);
            let focused: Vec<_> = snapshot.nodes.iter().filter(|node| node.focused).collect();
            assert_eq!(focused.len(), 1);
            assert_eq!(focused[0].id, id);
            fully_visible(focused[0], size);
        }
        let messages = ui.key(&app, Named::Enter);
        assert!(matches!(
            messages.as_slice(),
            [Message::InspectorAction(Action::Chemical(
                ChemicalFormat::Cdxml
            ))]
        ));
        apply(&mut app, messages);
        assert!(!app.tab.inspector_ui.menu_open());
        let closed = ui.snapshot(&app);
        let restored = find(&closed, "export-chemical-format");
        assert!(restored.name.contains("CDXML"));
        assert!(restored.focused, "the original opener keeps its focus");
        close(restored.bounds.y, anchor.y);
        assert!(
            !closed
                .nodes
                .iter()
                .any(|node| node.id.starts_with("chemical-format-"))
        );

        let messages = ui.key(&app, Named::Enter);
        apply(&mut app, messages);
        assert!(app.tab.inspector_ui.menu_open());
        let messages = ui.key(&app, Named::Escape);
        assert!(matches!(
            messages.as_slice(),
            [Message::InspectorAction(Action::ChemicalMenu(false))]
        ));
        apply(&mut app, messages);
        assert!(!app.tab.inspector_ui.menu_open());
        assert!(find(&ui.snapshot(&app), "export-chemical-format").focused);
        let messages = ui.key(&app, Named::Enter);
        apply(&mut app, messages);
        let (status, messages) = ui.event_at(
            &app,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
            mouse::Cursor::Available(iced::Point::new(1., 1.)),
        );
        assert_eq!(status, iced::event::Status::Captured);
        assert!(matches!(
            messages.as_slice(),
            [Message::InspectorAction(Action::ChemicalMenu(false))]
        ));
        apply(&mut app, messages);
        assert!(!app.tab.inspector_ui.menu_open());
        assert_eq!(app.tab.doc, drawing);
        assert_eq!(app.tab.revision, revision);
        assert!(!app.tab.history.can_undo());
    }
}
