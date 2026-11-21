use super::*;
use iced::advanced::{
    mouse,
    renderer::Headless,
    widget::{Operation, operation},
};
use iced::{
    Event, Size,
    keyboard::{
        self, Key, Modifiers,
        key::{Code, Named, Physical},
    },
};
use iced_runtime::{UserInterface, user_interface::Cache};
use reshiki::accessibility::{Collect, FocusControl, Role as AccessibleRole, SetValue};

struct Ui {
    renderer: iced::Renderer,
    cache: Cache,
    size: Size,
}

impl Ui {
    async fn new() -> Self {
        Self {
            renderer: <iced::Renderer as Headless>::new(
                iced::Font::default(),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap(),
            cache: Cache::new(),
            size: Size::new(340., 600.),
        }
    }

    fn operate<T>(&mut self, app: &App, op: &mut dyn Operation<T>) {
        let mut ui = UserInterface::build(
            app.tab.equipment_authoring.as_ref().unwrap().shapes_panel(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        ui.operate(&self.renderer, &mut operation::black_box(op));
        self.cache = ui.into_cache();
    }

    fn snapshot(&mut self, app: &App) -> reshiki::accessibility::Snapshot {
        let mut ui = UserInterface::build(
            app.tab.equipment_authoring.as_ref().unwrap().shapes_panel(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        ui.draw(
            &mut self.renderer,
            &iced::Theme::Light,
            &iced::advanced::renderer::Style::default(),
            mouse::Cursor::Unavailable,
        );
        let mut collect = Collect::new(iced::Rectangle::with_size(self.size));
        ui.operate(&self.renderer, &mut operation::black_box(&mut collect));
        self.cache = ui.into_cache();
        collect.snapshot().clone()
    }

    fn select(&mut self, app: &App, id: &str, value: &str) -> Option<Message> {
        let mut set = SetValue::<Message>::new(id, value.into());
        self.operate(app, &mut set);
        match set.finish() {
            operation::Outcome::Some(message) => Some(message),
            operation::Outcome::None => None,
            operation::Outcome::Chain(_) => panic!("A value action is immediate"),
        }
    }

    fn key(&mut self, app: &App, id: &str, named: Named) -> Vec<Message> {
        let mut focus = FocusControl::new(id);
        self.operate(app, &mut focus);
        if let operation::Outcome::Chain(mut next) = focus.finish() {
            self.operate(app, next.as_mut());
        }
        assert!(
            self.snapshot(app)
                .nodes
                .iter()
                .any(|n| n.id == id && n.focused)
        );
        let mut ui = UserInterface::build(
            app.tab.equipment_authoring.as_ref().unwrap().shapes_panel(),
            self.size,
            std::mem::take(&mut self.cache),
            &mut self.renderer,
        );
        let mut messages = vec![];
        let (_, statuses) = ui.update(
            &[Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Named(named),
                modified_key: Key::Named(named),
                physical_key: Physical::Code(Code::ArrowDown),
                location: keyboard::Location::Standard,
                modifiers: Modifiers::empty(),
                text: None,
                repeat: false,
            })],
            mouse::Cursor::Unavailable,
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        assert_eq!(statuses, [iced::event::Status::Captured]);
        self.cache = ui.into_cache();
        messages
    }
}

fn id(app: &App, field: &str) -> String {
    app.tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .shape_control_id(field)
}

#[tokio::test]
#[ignore = "Opt-in actual renderer equipment picker semantics and live native actions"]
async fn shape_pickers_expose_values_keyboard_and_native_selection_without_stale_targets() {
    let mut app = drawing();
    let original = app.tab.doc.clone();
    send(&mut app, Action::Begin);
    calibrate(&mut app);
    let mut ui = Ui::new().await;
    let snapshot = ui.snapshot(&app);
    assert!(snapshot.duplicate_ids.is_empty());
    assert_eq!(snapshot.nodes.len(), 4);
    for (field, name, value) in [
        ("select", "Shape", "Shape 1"),
        ("layer", "Layer", "Body"),
        ("material", "Material", "Ink"),
        ("role", "Draw as", "Outline"),
    ] {
        let control = snapshot
            .nodes
            .iter()
            .find(|n| n.id == id(&app, field))
            .unwrap();
        assert_eq!(control.name, name);
        assert_eq!(control.role, AccessibleRole::ComboBox);
        assert_eq!(control.value.as_deref(), Some(value));
        assert!(control.enabled && control.visible_bounds.is_some());
    }

    let messages = ui.key(&app, &id(&app, "layer"), Named::ArrowDown);
    assert_eq!(messages.len(), 1);
    for message in messages {
        let _ = app.update(message);
    }
    assert_eq!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .current
            .draft
            .definition
            .shapes[0]
            .layer,
        Layer::Inner
    );
    let material_id = id(&app, "material");
    let message = ui.select(&app, &material_id, "Metal").unwrap();
    let _ = app.update(message);
    assert_eq!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .current
            .draft
            .definition
            .shapes[0]
            .material,
        Material::Metal
    );
    assert!(
        ui.select(&app, &id(&app, "role"), "Joined glass").is_none(),
        "Native choices must use the same filtered roles as the menu"
    );
    let message = ui.select(&app, &id(&app, "role"), "Solid").unwrap();
    let _ = app.update(message);
    assert_eq!(
        app.tab
            .equipment_authoring
            .as_ref()
            .unwrap()
            .current
            .draft
            .definition
            .shapes[0]
            .role,
        Role::Solid
    );
    let snapshot = ui.snapshot(&app);
    for (name, value) in [
        ("Layer", "Inner"),
        ("Material", "Metal"),
        ("Draw as", "Solid"),
    ] {
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .find(|n| n.name == name)
                .unwrap()
                .value
                .as_deref(),
            Some(value)
        );
    }

    let stale = ui.select(&app, &material_id, "Rubber").unwrap();
    let message = ui.select(&app, &id(&app, "select"), "Shape 2").unwrap();
    let _ = app.update(message);
    assert_eq!(
        app.tab.equipment_authoring.as_ref().unwrap().selected_shape,
        1
    );
    assert!(ui.select(&app, &material_id, "Rubber").is_none());
    let before = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .current
        .draft
        .clone();
    let history = app.tab.equipment_authoring.as_ref().unwrap().past.len();
    let _ = app.update(stale.clone());
    assert_eq!(
        app.tab.equipment_authoring.as_ref().unwrap().current.draft,
        before
    );
    assert_eq!(
        app.tab.equipment_authoring.as_ref().unwrap().past.len(),
        history
    );
    let snapshot = ui.snapshot(&app);
    for (name, value) in [
        ("Shape", "Shape 2"),
        ("Layer", "Body"),
        ("Material", "Ink"),
        ("Draw as", "Outline"),
    ] {
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .find(|n| n.name == name)
                .unwrap()
                .value
                .as_deref(),
            Some(value)
        );
    }
    let message = ui.select(&app, &id(&app, "material"), "Rubber").unwrap();
    let _ = app.update(message);
    let state = app.tab.equipment_authoring.as_ref().unwrap();
    assert_eq!(
        state.current.draft.definition.shapes[0].material,
        Material::Metal
    );
    assert_eq!(
        state.current.draft.definition.shapes[1].material,
        Material::Rubber
    );
    assert_eq!(app.tab.doc, original);

    send(&mut app, Action::Discard);
    send(&mut app, Action::Begin);
    let before = app
        .tab
        .equipment_authoring
        .as_ref()
        .unwrap()
        .current
        .draft
        .clone();
    let _ = app.update(stale);
    assert_eq!(
        app.tab.equipment_authoring.as_ref().unwrap().current.draft,
        before
    );
}

#[tokio::test]
#[ignore = "Opt-in actual renderer filtered equipment roles and pending joint controls"]
async fn native_role_options_and_pending_joint_match_the_visible_pickers() {
    let mut ui = Ui::new().await;
    let mut app = drawing();
    app.tab.doc.graphics[0].kind = GraphicKind::Line;
    send(&mut app, Action::Begin);
    assert!(ui.select(&app, &id(&app, "role"), "Solid").is_none());
    assert!(ui.select(&app, &id(&app, "role"), "Joined glass").is_none());
    assert!(ui.select(&app, &id(&app, "role"), "Detail").is_some());
    assert!(
        ui.select(&app, &id(&app, "material"), "Unknown material")
            .is_none()
    );

    let (mut app, edges) = joint_drawing();
    send(&mut app, Action::Begin);
    let material = id(&app, "material");
    mark_joint(&mut app, edges);
    let snapshot = ui.snapshot(&app);
    assert_eq!(snapshot.nodes.len(), 1);
    assert_eq!(snapshot.nodes[0].name, "Shape");
    assert!(ui.select(&app, &material, "Metal").is_none());
}
