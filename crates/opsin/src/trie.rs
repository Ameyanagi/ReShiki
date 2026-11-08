// SPDX-License-Identifier: MIT
// Origin: adapted
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/OpsinRadixTrie.java | 42e87ac6efb1bd9f578986f5ea7ee1faa8e93c91
// Source: OPSIN | opsin-core/src/main/java/uk/ac/cam/ch/wwmm/opsin/OpsinRadixTrie.java | b91b610af5ab07560fedb20730d7aef46bb2bca0
// Copyright 2017 Daniel Lowe
// Changes: Rust ownership, arena/container and error-handling adaptation; integrated native changes are documented in PORTING.md.

//! Prefix matching for every dictionary token, including empty markers.

#[derive(Debug, Clone, Default)]
pub(crate) struct Trie {
    root: Node,
}

/// Children sorted by byte. Most nodes have one child, which a sorted vector
/// keeps in one small allocation where a B-tree map allocates a whole leaf.
#[derive(Debug, Clone, Default)]
struct Node {
    terminal: bool,
    children: Vec<(u8, Node)>,
}

impl Node {
    fn child(&self, byte: u8) -> Option<&Node> {
        let index = self
            .children
            .binary_search_by_key(&byte, |(b, _)| *b)
            .ok()?;
        Some(&self.children[index].1)
    }
}

impl Trie {
    pub fn insert(&mut self, token: &str) {
        let mut node = &mut self.root;
        for byte in token.bytes() {
            let index = match node.children.binary_search_by_key(&byte, |(b, _)| *b) {
                Ok(index) => index,
                Err(index) => {
                    // Exact growth: a vector's default first capacity of four
                    // would quadruple the usual single child.
                    node.children.reserve_exact(1);
                    node.children.insert(index, (byte, Node::default()));
                    index
                }
            };
            node = &mut node.children[index].1;
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
            let Some(child) = node.child(*byte) else {
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
