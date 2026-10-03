//! Convert live widget snapshots into the native tree and validate requests.
use super::{Node, Role, Snapshot};
use accesskit::{Action, ActionRequest, Affine, NodeId, Rect, Tree, TreeId, TreeUpdate};
use iced::Rectangle;
use std::collections::{BTreeMap, BTreeSet};

const ROOT: NodeId = NodeId(1);
const MAX_NODES: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Activate(String),
    Focus(String),
    Reveal(String),
    SetValue(String, String),
}

/// A window owns this identity registry. Removed IDs are never reused, so an
/// old native request cannot hit a newly opened menu with the same semantic ID.
pub struct NativeTree {
    next: u64,
    ids: BTreeMap<String, NodeId>,
    live: BTreeMap<NodeId, Node>,
}

impl Default for NativeTree {
    fn default() -> Self {
        Self {
            next: 2,
            ids: BTreeMap::new(),
            live: BTreeMap::new(),
        }
    }
}

impl NativeTree {
    /// Every result is a complete tree, usable for synchronous native
    /// activation as well as updates. The caller can skip unchanged results.
    pub fn update(
        &mut self,
        snapshot: &Snapshot,
        title: &str,
        viewport: Rectangle,
        scale: f32,
    ) -> Result<TreeUpdate, String> {
        // On invalid input, revoke actions immediately even if the caller has
        // not yet replaced a previously published native tree.
        self.live.clear();
        let previous_ids = std::mem::take(&mut self.ids);
        if !snapshot.duplicate_ids.is_empty() || snapshot.nodes.len() > MAX_NODES {
            return Err(
                "Accessibility controls have duplicate IDs or exceed the tree limit".into(),
            );
        }
        if !valid_rectangle(viewport) || !scale.is_finite() || scale <= 0. || scale > 32. {
            return Err("Accessibility window geometry is invalid".into());
        }
        let mut seen = BTreeSet::new();
        let mut focused = None;
        for node in &snapshot.nodes {
            if node.id.is_empty()
                || node.id.len() > 256
                || node.name.trim().is_empty()
                || node.name.len() > 4096
                || node.value.as_ref().is_some_and(|value| value.len() > 16384)
                || !seen.insert(node.id.as_str())
                || !valid_rectangle(node.bounds)
                || node
                    .visible_bounds
                    .is_some_and(|bounds| !valid_rectangle(bounds))
            {
                return Err("Accessibility control identity, label or geometry is invalid".into());
            }
            if node.focused {
                if !node.enabled || focused.replace(node.id.as_str()).is_some() {
                    return Err("Accessibility focus does not identify one enabled control".into());
                }
            }
        }
        let mut ids = BTreeMap::new();
        let mut nodes = Vec::with_capacity(snapshot.nodes.len() + 1);
        let mut children = Vec::with_capacity(snapshot.nodes.len());
        let mut focus = ROOT;
        for node in &snapshot.nodes {
            let id = if let Some(id) = previous_ids.get(&node.id) {
                *id
            } else {
                let id = NodeId(self.next);
                self.next = self
                    .next
                    .checked_add(1)
                    .ok_or("Accessibility identifiers exhausted")?;
                id
            };
            let mut native = accesskit::Node::new(match node.role {
                Role::Button | Role::ToggleButton => accesskit::Role::Button,
                Role::TextInput => accesskit::Role::TextInput,
            });
            native.set_label(node.name.clone());
            native.set_author_id(node.id.clone());
            // Iced has already applied every scroll translation and viewport
            // clip. A fully offscreen control has no clickable rectangle but
            // remains navigable through Focus/ScrollIntoView.
            native.set_bounds(node.visible_bounds.map(rect).unwrap_or(Rect::ZERO));
            if !node.enabled {
                native.set_disabled();
            }
            if let Some(checked) = node.checked {
                native.set_toggled(checked.into());
            }
            if let Some(value) = &node.value {
                native.set_value(value.clone());
            }
            if node.enabled {
                native.add_action(if node.role == Role::TextInput {
                    Action::SetValue
                } else {
                    Action::Click
                });
                native.add_action(Action::Focus);
                native.add_action(Action::ScrollIntoView);
            }
            if node.focused {
                focus = id;
            }
            ids.insert(node.id.clone(), id);
            self.live.insert(id, node.clone());
            children.push(id);
            nodes.push((id, native));
        }
        self.ids = ids;
        let mut root = accesskit::Node::new(accesskit::Role::Window);
        root.set_label(title.to_owned());
        root.set_bounds(rect(viewport));
        root.set_clips_children();
        root.set_transform(Affine::scale(f64::from(scale)));
        root.set_children(children);
        nodes.push((ROOT, root));
        let mut tree = Tree::new(ROOT);
        tree.toolkit_name = Some("ReShiki / Iced".into());
        tree.toolkit_version = Some("0.14".into());
        Ok(TreeUpdate {
            nodes,
            tree: Some(tree),
            tree_id: TreeId::ROOT,
            focus,
        })
    }

    /// The application additionally checks the queued window generation, and
    /// resolves this semantic target once more against its current widget tree.
    pub fn resolve(&self, request: &ActionRequest) -> Option<Request> {
        if request.target_tree != TreeId::ROOT {
            return None;
        }
        let node = self.live.get(&request.target_node)?;
        if !node.enabled {
            return None;
        }
        if request.action == Action::SetValue && node.role == Role::TextInput {
            return match &request.data {
                Some(accesskit::ActionData::Value(value)) if value.len() <= 16384 => {
                    Some(Request::SetValue(node.id.clone(), value.to_string()))
                }
                _ => None,
            };
        }
        if request.data.is_some() {
            return None;
        }
        match request.action {
            Action::Click if node.role != Role::TextInput => {
                Some(Request::Activate(node.id.clone()))
            }
            Action::Focus => Some(Request::Focus(node.id.clone())),
            Action::ScrollIntoView => Some(Request::Reveal(node.id.clone())),
            _ => None,
        }
    }
}

fn valid_rectangle(rect: Rectangle) -> bool {
    [rect.x, rect.y, rect.width, rect.height]
        .into_iter()
        .all(f32::is_finite)
        && (rect.x + rect.width).is_finite()
        && (rect.y + rect.height).is_finite()
        && rect.width >= 0.
        && rect.height >= 0.
}
fn rect(bounds: Rectangle) -> Rect {
    Rect::new(
        f64::from(bounds.x),
        f64::from(bounds.y),
        f64::from(bounds.x + bounds.width),
        f64::from(bounds.y + bounds.height),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn node(id: &str) -> Node {
        Node {
            id: id.into(),
            name: "Apply".into(),
            role: Role::Button,
            enabled: true,
            focused: true,
            checked: None,
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
}
