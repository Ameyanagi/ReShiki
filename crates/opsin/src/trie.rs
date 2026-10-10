//! Prefix matching for every dictionary token, including empty markers.
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct Trie {
    root: Node,
}

#[derive(Debug, Clone, Default)]
struct Node {
    terminal: bool,
    children: BTreeMap<u8, Node>,
}

impl Trie {
    pub fn insert(&mut self, token: &str) {
        let mut node = &mut self.root;
        for byte in token.bytes() {
            node = node.children.entry(byte).or_default();
        }
        node.terminal = true;
    }
    pub fn matches(&self, input: &[u8], offset: usize) -> Vec<usize> {
        let mut node = &self.root;
        let mut matches = Vec::new();
        if node.terminal {
            matches.push(offset);
        }
        for (i, byte) in input[offset..].iter().enumerate() {
            let Some(child) = node.children.get(byte) else {
                break;
            };
            node = child;
            if node.terminal {
                matches.push(offset + i + 1);
            }
        }
        matches
    }
}
