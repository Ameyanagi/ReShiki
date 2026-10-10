//! Ordered parse-tree elements ported from OPSIN `Element`, `GroupingEl`,
//! `TokenEl`, `Attribute`, `OpsinTools` and `SortParses`.
//! Source: OPSIN 2.9.0, b91b610af5ab07560fedb20730d7aef46bb2bca0.
//! Copyright Daniel Lowe and OPSIN contributors; MIT (see LICENSE).

use crate::graph::FragmentId;
use std::cmp::Ordering;
use std::ops::{Index, IndexMut};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Attribute {
    pub name: String,
    pub value: String,
}

/// A grouping element has `value == None`; a token has `Some(value)`.
/// Attribute order and duplicate attributes follow upstream's list semantics.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Element {
    pub name: String,
    pub parent: Option<NodeId>,
    pub attributes: Vec<Attribute>,
    pub children: Vec<NodeId>,
    pub value: Option<String>,
    pub fragment: Option<FragmentId>,
}

impl Element {
    pub fn attribute(&self, name: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|a| a.name == name)
            .map(|a| a.value.as_str())
    }

    pub fn add_attribute(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.attributes.push(Attribute {
            name: name.into(),
            value: value.into(),
        });
    }

    /// Mutates the first matching attribute, preserving its position.
    pub fn set_attribute(&mut self, name: &str, value: impl Into<String>) {
        let value = value.into();
        if let Some(attribute) = self.attributes.iter_mut().find(|a| a.name == name) {
            attribute.value = value;
        } else {
            self.add_attribute(name, value);
        }
    }

    pub fn remove_attribute(&mut self, name: &str) -> Option<Attribute> {
        let index = self.attributes.iter().position(|a| a.name == name)?;
        Some(self.attributes.remove(index))
    }

    pub fn is_token(&self) -> bool {
        self.value.is_some()
    }
    pub fn get_fragment(&self) -> Option<FragmentId> {
        assert!(self.is_token(), "Only tokens can have associated fragments");
        self.fragment
    }
    pub fn set_fragment(&mut self, fragment: Option<FragmentId>) {
        assert!(self.is_token(), "Only tokens can have associated fragments");
        self.fragment = fragment;
    }
    pub fn set_value(&mut self, value: impl Into<String>) {
        assert!(self.is_token(), "Token groups do not have a value");
        self.value = Some(value.into());
    }
}

/// IDs never change; detaching a node leaves it available for later insertion.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Arena {
    nodes: Vec<Element>,
}

impl Index<NodeId> for Arena {
    type Output = Element;
    fn index(&self, id: NodeId) -> &Self::Output {
        &self.nodes[id.0]
    }
}
impl IndexMut<NodeId> for Arena {
    fn index_mut(&mut self, id: NodeId) -> &mut Self::Output {
        &mut self.nodes[id.0]
    }
}

impl Arena {
    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    pub fn nodes(&self) -> impl Iterator<Item = (NodeId, &Element)> {
        self.nodes
            .iter()
            .enumerate()
            .map(|(i, element)| (NodeId(i), element))
    }
    fn allocate(&mut self, name: impl Into<String>, value: Option<String>) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Element {
            name: name.into(),
            parent: None,
            attributes: Vec::new(),
            children: Vec::new(),
            value,
            fragment: None,
        });
        id
    }
    pub fn grouping(&mut self, name: impl Into<String>) -> NodeId {
        self.allocate(name, None)
    }
    pub fn token(&mut self, name: impl Into<String>, value: impl Into<String>) -> NodeId {
        self.allocate(name, Some(value.into()))
    }
    pub fn add_child(&mut self, parent: NodeId, child: NodeId) {
        assert!(!self[parent].is_token(), "Tokens do not have children");
        self[child].parent = Some(parent);
        self[parent].children.push(child);
    }
    pub fn insert_child(&mut self, parent: NodeId, child: NodeId, index: usize) {
        assert!(!self[parent].is_token(), "Tokens do not have children");
        self[child].parent = Some(parent);
        self[parent].children.insert(index, child);
    }
    pub fn index_of(&self, parent: NodeId, child: NodeId) -> Option<usize> {
        self[parent].children.iter().position(|&id| id == child)
    }
    pub fn remove_child(&mut self, parent: NodeId, child: NodeId) -> bool {
        assert!(!self[parent].is_token(), "Tokens do not have children");
        self[child].parent = None;
        if let Some(index) = self.index_of(parent, child) {
            self[parent].children.remove(index);
            true
        } else {
            false
        }
    }
    pub fn remove_child_at(&mut self, parent: NodeId, index: usize) -> NodeId {
        assert!(!self[parent].is_token(), "Tokens do not have children");
        let removed = self[parent].children.remove(index);
        self[removed].parent = None;
        removed
    }
    pub fn detach(&mut self, child: NodeId) {
        if let Some(parent) = self[child].parent {
            self.remove_child(parent, child);
        }
    }
    pub fn replace_child(&mut self, parent: NodeId, old: NodeId, new: NodeId) {
        let index = self
            .index_of(parent, old)
            .expect("oldChild is not a child of this element.");
        self.remove_child_at(parent, index);
        self.insert_child(parent, new, index);
    }
    pub fn insert_after(&mut self, reference: NodeId, inserted: NodeId) {
        let parent = self[reference]
            .parent
            .expect("Reference element has no parent");
        let index = self
            .index_of(parent, reference)
            .expect("Reference is not a child of its parent");
        self.insert_child(parent, inserted, index + 1);
    }
    pub fn insert_before(&mut self, reference: NodeId, inserted: NodeId) {
        let parent = self[reference]
            .parent
            .expect("Reference element has no parent");
        let index = self
            .index_of(parent, reference)
            .expect("Reference is not a child of its parent");
        self.insert_child(parent, inserted, index);
    }
    pub fn children_named(&self, parent: NodeId, name: &str) -> Vec<NodeId> {
        self[parent]
            .children
            .iter()
            .copied()
            .filter(|&id| self[id].name == name)
            .collect()
    }
    pub fn first_child_named(&self, parent: NodeId, name: &str) -> Option<NodeId> {
        self[parent]
            .children
            .iter()
            .copied()
            .find(|&id| self[id].name == name)
    }
    pub fn last_child(&self, parent: NodeId) -> Option<NodeId> {
        self[parent].children.last().copied()
    }
    pub fn previous_sibling(&self, id: NodeId) -> Option<NodeId> {
        let parent = self[id].parent?;
        let index = self.index_of(parent, id)?;
        index.checked_sub(1).map(|i| self[parent].children[i])
    }
    pub fn next_sibling(&self, id: NodeId) -> Option<NodeId> {
        let parent = self[id].parent?;
        let index = self.index_of(parent, id)?;
        self[parent].children.get(index + 1).copied()
    }
    pub fn previous_sibling_named(&self, id: NodeId, name: &str) -> Option<NodeId> {
        let mut current = self.previous_sibling(id);
        while let Some(id) = current {
            if self[id].name == name {
                return Some(id);
            }
            current = self.previous_sibling(id);
        }
        None
    }
    /// Descendants, excluding `root`, in upstream's left-to-right depth-first order.
    pub fn descendants(&self, root: NodeId) -> Vec<NodeId> {
        let mut stack: Vec<_> = self[root].children.iter().rev().copied().collect();
        let mut descendants = Vec::new();
        while let Some(id) = stack.pop() {
            descendants.push(id);
            stack.extend(self[id].children.iter().rev().copied());
        }
        descendants
    }
    pub fn descendants_named(&self, root: NodeId, name: &str) -> Vec<NodeId> {
        self.descendants(root)
            .into_iter()
            .filter(|&id| self[id].name == name)
            .collect()
    }
    pub fn last_leaf(&self, root: NodeId) -> Option<NodeId> {
        let mut id = self.last_child(root)?;
        while let Some(child) = self.last_child(id) {
            id = child;
        }
        Some(id)
    }
    pub fn value(&self, id: NodeId) -> String {
        if let Some(value) = &self[id].value {
            return value.clone();
        }
        self[id]
            .children
            .iter()
            .map(|&child| self.value(child))
            .collect()
    }
    /// `Element.copy()` deliberately does not copy token fragment references.
    pub fn copy(&mut self, root: NodeId) -> NodeId {
        let source = self[root].clone();
        let copy = self.allocate(source.name, source.value);
        self[copy].attributes = source.attributes;
        for child in source.children {
            let child_copy = self.copy(child);
            self.add_child(copy, child_copy);
        }
        copy
    }
    pub fn copy_token_with_value(&mut self, token: NodeId, value: impl Into<String>) -> NodeId {
        assert!(self[token].is_token(), "Only tokens have a value");
        let copy = self.token(self[token].name.clone(), value);
        self[copy].attributes = self[token].attributes.clone();
        copy
    }
    pub fn counts(&self, root: NodeId) -> (usize, usize) {
        let descendants = self.descendants(root);
        let leaves = descendants
            .iter()
            .filter(|&&id| self[id].children.is_empty())
            .count()
            + usize::from(self[root].children.is_empty());
        (descendants.len(), leaves)
    }
    pub fn to_xml(&self, root: NodeId) -> String {
        fn write(arena: &Arena, id: NodeId, indent: usize, output: &mut String) {
            output.push_str(&"  ".repeat(indent));
            output.push('<');
            output.push_str(&arena[id].name);
            for attribute in &arena[id].attributes {
                output.push(' ');
                output.push_str(&attribute.name);
                output.push_str("=\"");
                output.push_str(&xml_encode(&attribute.value));
                output.push('"');
            }
            output.push('>');
            if arena[id].children.is_empty() {
                output.push_str(&xml_encode(&arena.value(id)));
            } else {
                for &child in &arena[id].children {
                    output.push('\n');
                    write(arena, child, indent + 1, output);
                }
                output.push('\n');
                output.push_str(&"  ".repeat(indent));
            }
            output.push_str("</");
            output.push_str(&arena[id].name);
            output.push('>');
        }
        let mut output = String::new();
        write(self, root, 0, &mut output);
        output
    }
}

pub fn xml_encode(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for ch in value.chars() {
        let encoded = match ch {
            '\t' => "&#x09;",
            '\n' => "&#x0A;",
            '\r' => "&#x0D;",
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&quot;",
            _ => {
                output.push(ch);
                continue;
            }
        };
        output.push_str(encoded);
    }
    output
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseTree {
    pub arena: Arena,
    pub root: NodeId,
}

impl ParseTree {
    pub fn to_xml(&self) -> String {
        self.arena.to_xml(self.root)
    }
    pub fn compare(&self, other: &Self) -> Ordering {
        let substituent = |tree: &Self| {
            tree.arena
                .first_child_named(tree.root, "wordRule")
                .is_some_and(|id| tree.arena[id].attribute("wordRule") == Some("substituent"))
        };
        let (elements, leaves) = self.arena.counts(self.root);
        let (other_elements, other_leaves) = other.arena.counts(other.root);
        (substituent(self), leaves, elements).cmp(&(
            substituent(other),
            other_leaves,
            other_elements,
        ))
    }
}

/// Rust's stable sort preserves the upstream order when the comparator ties.
pub fn sort_parses(parses: &mut [ParseTree]) {
    parses.sort_by(ParseTree::compare);
}
