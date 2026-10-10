//! Tree-only OPSIN utilities, transcribed from `OpsinTools.java`.
//! Source: OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).

use crate::parse_tree::{Arena, NodeId};

impl Arena {
    pub fn next_sibling_named(&self, id: NodeId, name: &str) -> Option<NodeId> {
        let mut current = self.next_sibling(id);
        while let Some(id) = current {
            if self[id].name == name {
                return Some(id);
            }
            current = self.next_sibling(id);
        }
        None
    }
    pub fn next_sibling_ignoring(&self, id: NodeId, ignored: &[&str]) -> Option<NodeId> {
        let mut current = self.next_sibling(id);
        while let Some(id) = current {
            if !ignored.contains(&self[id].name.as_str()) {
                return Some(id);
            }
            current = self.next_sibling(id);
        }
        None
    }
    pub fn previous_sibling_ignoring(&self, id: NodeId, ignored: &[&str]) -> Option<NodeId> {
        let mut current = self.previous_sibling(id);
        while let Some(id) = current {
            if !ignored.contains(&self[id].name.as_str()) {
                return Some(id);
            }
            current = self.previous_sibling(id);
        }
        None
    }
    /// The next leaf across grouping scopes, restricted to the current molecule
    /// branch if `within_connected_component` is true.
    pub fn next_element(&self, id: NodeId, within_connected_component: bool) -> Option<NodeId> {
        let parent = self[id].parent?;
        if within_connected_component && self[parent].name == "molecule" {
            return None;
        }
        if let Some(mut next) = self.next_sibling(id) {
            while let Some(&child) = self[next].children.first() {
                next = child;
            }
            Some(next)
        } else {
            self.next_element(parent, within_connected_component)
        }
    }
    pub fn previous_element(&self, id: NodeId, within_connected_component: bool) -> Option<NodeId> {
        let parent = self[id].parent?;
        if within_connected_component && self[parent].name == "molecule" {
            return None;
        }
        if let Some(mut previous) = self.previous_sibling(id) {
            while let Some(&child) = self[previous].children.last() {
                previous = child;
            }
            Some(previous)
        } else {
            self.previous_element(parent, within_connected_component)
        }
    }
    pub fn next_siblings_named(&self, id: NodeId, name: &str) -> Vec<NodeId> {
        let mut result = Vec::new();
        let mut current = self.next_sibling(id);
        while let Some(id) = current {
            if self[id].name == name {
                result.push(id);
            }
            current = self.next_sibling(id);
        }
        result
    }
    pub fn descendants_named_any(&self, root: NodeId, names: &[&str]) -> Vec<NodeId> {
        self.descendants(root)
            .into_iter()
            .filter(|&id| names.contains(&self[id].name.as_str()))
            .collect()
    }
    pub fn children_with_attribute(
        &self,
        root: NodeId,
        name: &str,
        attribute: &str,
        value: &str,
    ) -> Vec<NodeId> {
        self[root]
            .children
            .iter()
            .copied()
            .filter(|&id| self[id].name == name && self[id].attribute(attribute) == Some(value))
            .collect()
    }
    pub fn parent_word_rule(&self, id: NodeId) -> Option<NodeId> {
        let mut current = self[id].parent;
        while let Some(id) = current {
            if self[id].name == "wordRule" {
                return Some(id);
            }
            current = self[id].parent;
        }
        None
    }
}

pub fn fix_locant_capitalisation(locant: &str) -> String {
    let bytes = locant.as_bytes();
    if bytes.len() >= 2
        && matches!(bytes[bytes.len() - 1], b'A'..=b'G')
        && bytes[..bytes.len() - 1].iter().all(u8::is_ascii_digit)
    {
        locant.to_ascii_lowercase()
    } else {
        locant.to_string()
    }
}

pub fn remove_dash(value: &str) -> &str {
    value.strip_suffix('-').unwrap_or(value)
}
