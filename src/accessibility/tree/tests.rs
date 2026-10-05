use super::super::Node;
use super::*;
fn node(id: &str) -> Node {
    Node {
        id: id.into(),
        name: "Apply".into(),
        role: Role::Button,
        enabled: true,
        focused: true,
        checked: None,
        expanded: None,
        value: None,
        bounds: Rectangle {
            x: 10.,
            y: 20.,
            width: 80.,
            height: 30.,
        },
        visible_bounds: Some(Rectangle {
            x: 10.,
            y: 20.,
            width: 80.,
            height: 30.,
        }),
    }
}
fn snapshot(nodes: Vec<Node>) -> Snapshot {
    Snapshot {
        nodes,
        duplicate_ids: Vec::new(),
    }
}
fn request(id: NodeId) -> ActionRequest {
    ActionRequest {
        action: Action::Click,
        target_tree: TreeId::ROOT,
        target_node: id,
        data: None,
    }
}
#[test]
fn native_consumer_accepts_scaled_tree_and_removed_targets_never_rebind() {
    let viewport = Rectangle::with_size(iced::Size::new(320., 220.));
    let mut bridge = NativeTree::default();
    let first = bridge
        .update(&snapshot(vec![node("apply")]), "ReShiki", viewport, 2.)
        .unwrap();
    let id = first.focus;
    let consumer = accesskit_consumer::Tree::new(first.clone(), true);
    let state = consumer.state();
    let child = state.root().children().next().unwrap();
    assert_eq!(child.bounding_box(), Some(Rect::new(20., 40., 180., 100.)));
    assert_eq!(
        bridge.resolve(&request(id)),
        Some(Request::Activate("apply".into()))
    );
    assert_eq!(
        bridge
            .update(&snapshot(vec![node("apply")]), "ReShiki", viewport, 2.)
            .unwrap()
            .focus,
        id
    );
    bridge
        .update(&snapshot(vec![]), "ReShiki", viewport, 2.)
        .unwrap();
    assert_eq!(bridge.resolve(&request(id)), None);
    let reopened = bridge
        .update(&snapshot(vec![node("apply")]), "ReShiki", viewport, 2.)
        .unwrap();
    assert_ne!(reopened.focus, id);
    assert_eq!(bridge.resolve(&request(id)), None);
    assert_eq!(
        bridge.resolve(&request(reopened.focus)),
        Some(Request::Activate("apply".into()))
    );
}
#[test]
fn invalid_or_disabled_controls_revoke_native_actions() {
    let viewport = Rectangle::with_size(iced::Size::new(320., 220.));
    let mut bridge = NativeTree::default();
    let id = bridge
        .update(&snapshot(vec![node("apply")]), "ReShiki", viewport, 1.)
        .unwrap()
        .focus;
    assert!(
        bridge
            .update(
                &snapshot(vec![node("apply"), node("apply")]),
                "ReShiki",
                viewport,
                1.
            )
            .is_err()
    );
    assert_eq!(bridge.resolve(&request(id)), None);
    let mut disabled = node("apply");
    disabled.enabled = false;
    disabled.focused = false;
    bridge
        .update(&snapshot(vec![disabled]), "ReShiki", viewport, 1.)
        .unwrap();
    assert_eq!(bridge.resolve(&request(id)), None);
}

#[test]
fn disabling_controls_revokes_actions_without_changing_native_ids() {
    let viewport = Rectangle::with_size(iced::Size::new(320., 220.));
    let mut bridge = NativeTree::default();
    let id = bridge
        .update(&snapshot(vec![node("apply")]), "ReShiki", viewport, 1.)
        .unwrap()
        .focus;
    for enabled in [false, true] {
        let mut control = node("apply");
        control.enabled = enabled;
        control.focused = enabled;
        let tree = bridge
            .update(&snapshot(vec![control]), "ReShiki", viewport, 1.)
            .unwrap();
        assert_eq!(bridge.ids.get("apply"), Some(&id));
        assert!(tree.nodes.iter().any(|(current, _)| *current == id));
        for (action, expected) in [
            (Action::Click, Request::Activate("apply".into())),
            (Action::Focus, Request::Focus("apply".into())),
            (Action::ScrollIntoView, Request::Reveal("apply".into())),
        ] {
            assert_eq!(
                bridge.resolve(&ActionRequest {
                    action,
                    ..request(id)
                }),
                enabled.then_some(expected)
            );
        }
    }
}

#[test]
fn native_actions_follow_the_current_role_and_value_limits() {
    let viewport = Rectangle::with_size(iced::Size::new(320., 220.));
    let mut bridge = NativeTree::default();
    let mut previous = None;
    for role in [
        Role::Button,
        Role::TextInput,
        Role::TextArea,
        Role::ToggleButton,
    ] {
        let mut control = node("control");
        control.role = role;
        let id = bridge
            .update(&snapshot(vec![control]), "ReShiki", viewport, 1.)
            .unwrap()
            .focus;
        assert_eq!(*previous.get_or_insert(id), id);
        let editable = matches!(role, Role::TextInput | Role::TextArea);
        assert_eq!(
            bridge.resolve(&request(id)),
            (!editable).then(|| Request::Activate("control".into()))
        );
        for length in [16384, 16385] {
            let value = "x".repeat(length);
            let action = ActionRequest {
                action: Action::SetValue,
                data: Some(accesskit::ActionData::Value(value.clone().into())),
                ..request(id)
            };
            assert_eq!(
                bridge.resolve(&action),
                (editable && length <= 16384).then_some(Request::SetValue("control".into(), value))
            );
        }
        for action in [Action::Click, Action::Focus, Action::ScrollIntoView] {
            assert_eq!(
                bridge.resolve(&ActionRequest {
                    action,
                    data: Some(accesskit::ActionData::Value("unexpected".into())),
                    ..request(id)
                }),
                None
            );
        }
    }
}

#[test]
fn invalid_unretained_semantics_still_revoke_native_targets() {
    let viewport = Rectangle::with_size(iced::Size::new(320., 220.));
    let mut invalid_name = node("apply");
    invalid_name.name.clear();
    let mut invalid_value = node("apply");
    invalid_value.value = Some("x".repeat(16385));
    let mut invalid_bounds = node("apply");
    invalid_bounds.bounds.x = f32::NAN;
    for invalid in [invalid_name, invalid_value, invalid_bounds] {
        let mut bridge = NativeTree::default();
        let id = bridge
            .update(&snapshot(vec![node("apply")]), "ReShiki", viewport, 1.)
            .unwrap()
            .focus;
        assert!(
            bridge
                .update(&snapshot(vec![invalid]), "ReShiki", viewport, 1.)
                .is_err()
        );
        assert_eq!(bridge.resolve(&request(id)), None);
    }
}
