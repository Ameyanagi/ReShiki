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
