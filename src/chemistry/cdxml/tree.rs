//! Private detached XML tree. Element text and tails follow ElementTree's model;
//! comments, processing instructions, and the external DTD are not retained.
use super::{Error, Result};
use std::cell::Cell;

const INPUT_BYTES: usize = 16 * 1024 * 1024;
const OUTPUT_BYTES: usize = 32 * 1024 * 1024;
const ARENA_NODES: usize = 200_000;

#[derive(Clone, Debug)]
pub(super) struct Element {
    pub tag: String,
    pub attributes: Vec<(String, String)>,
    pub text: String,
    pub tail: String,
    pub children: Vec<usize>,
    pub parent: Option<usize>,
}
impl Element {
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attributes
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }
    pub fn set(&mut self, key: &str, value: String) {
        if let Some((_, v)) = self.attributes.iter_mut().find(|(k, _)| k == key) {
            *v = value;
        } else {
            self.attributes.push((key.into(), value));
        }
    }
}

pub(super) struct Tree {
    elements: Vec<Element>,
    work: Cell<usize>,
}
impl Tree {
    pub fn parse(text: &str) -> Result<Self> {
        let tree = Self::parse_import(text)?;
        if tree.node(0)?.tag != "CDXML" {
            return Err(Error::Invalid("Expected CDXML root".into()));
        }
        Ok(tree)
    }
    /// Original import allows any permitted object as the XML root. Keep its
    /// identity and ordinals; callers still validate allowed objects/pages.
    pub fn parse_import(text: &str) -> Result<Self> {
        if text.len() > INPUT_BYTES {
            return Err(Error::Limit);
        }
        reject_dtd_declarations(text)?;
        let doc = roxmltree::Document::parse_with_options(
            text,
            roxmltree::ParsingOptions {
                nodes_limit: 400_000,
                allow_dtd: true,
            },
        )
        .map_err(|e| Error::Invalid(e.to_string()))?;
        let root = doc.root_element();
        let mut tree = Self {
            elements: Vec::new(),
            work: Cell::new(20_000_000),
        };
        let root_index = tree.add(root, None)?;
        let mut stack = vec![(root, root_index, 0usize)];
        let mut attributes = 0usize;
        while let Some((xml, index, depth)) = stack.pop() {
            if depth > 64 {
                return Err(Error::Limit);
            }
            attributes += xml.attributes().len();
            if attributes > 1_000_000 {
                return Err(Error::Limit);
            }
            let mut previous = None;
            for child in xml.children() {
                if child.is_element() {
                    let next = tree.add(child, Some(index))?;
                    tree.node_mut(index)?.children.push(next);
                    previous = Some(next);
                    stack.push((child, next, depth + 1));
                } else if let Some(value) = child.text().filter(|_| child.is_text()) {
                    if let Some(previous) = previous {
                        tree.node_mut(previous)?.tail.push_str(value);
                    } else {
                        tree.node_mut(index)?.text.push_str(value);
                    }
                }
            }
        }
        Ok(tree)
    }
    fn add(&mut self, xml: roxmltree::Node<'_, '_>, parent: Option<usize>) -> Result<usize> {
        if self.elements.len() >= 100_000 {
            return Err(Error::Limit);
        }
        if xml.tag_name().namespace().is_some() || xml.attributes().any(|a| a.namespace().is_some())
        {
            return Err(Error::Invalid("Unsupported namespace".into()));
        }
        let index = self.elements.len();
        self.elements.push(Element {
            tag: xml.tag_name().name().into(),
            attributes: xml
                .attributes()
                .map(|a| (a.name().into(), a.value().into()))
                .collect(),
            text: String::new(),
            tail: String::new(),
            children: Vec::new(),
            parent,
        });
        Ok(index)
    }
    pub fn spend(&self, amount: usize) -> Result<()> {
        self.work
            .set(self.work.get().checked_sub(amount).ok_or(Error::Limit)?);
        Ok(())
    }
    pub fn node(&self, index: usize) -> Result<&Element> {
        self.spend(1)?;
        self.elements
            .get(index)
            .ok_or_else(|| Error::Invalid("Missing XML node".into()))
    }
    pub fn node_mut(&mut self, index: usize) -> Result<&mut Element> {
        self.spend(1)?;
        self.elements
            .get_mut(index)
            .ok_or_else(|| Error::Invalid("Missing XML node".into()))
    }
    pub fn children(&self, index: usize, tag: &str) -> Result<Vec<usize>> {
        let node = self.node(index)?;
        self.spend(node.children.len())?;
        node.children
            .iter()
            .copied()
            .filter_map(|child| match self.node(child) {
                Ok(node) if node.tag == tag => Some(Ok(child)),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }
    pub fn descendants(&self, index: usize) -> Result<Vec<usize>> {
        let mut found = Vec::new();
        let mut stack = vec![index];
        while let Some(index) = stack.pop() {
            let node = self.node(index)?;
            found.push(index);
            stack.extend(node.children.iter().rev().copied());
        }
        Ok(found)
    }
    pub fn detach(&mut self, child: usize) -> Result<()> {
        if let Some(parent) = self.node(child)?.parent {
            self.spend(self.node(parent)?.children.len())?;
            self.node_mut(parent)?.children.retain(|&i| i != child);
            self.node_mut(child)?.parent = None;
        }
        Ok(())
    }
    pub fn append(&mut self, parent: usize, child: usize) -> Result<()> {
        self.detach(child)?;
        self.node_mut(parent)?.children.push(child);
        self.node_mut(child)?.parent = Some(parent);
        Ok(())
    }
    pub fn clone_subtree(&mut self, source: usize) -> Result<usize> {
        let root = self.clone_element(source)?;
        let mut stack = vec![(source, root)];
        while let Some((old, new)) = stack.pop() {
            let children = self.node(old)?.children.clone();
            for old_child in children {
                let new_child = self.clone_element(old_child)?;
                self.append(new, new_child)?;
                stack.push((old_child, new_child));
            }
        }
        Ok(root)
    }
    fn clone_element(&mut self, source: usize) -> Result<usize> {
        if self.elements.len() >= ARENA_NODES {
            return Err(Error::Limit);
        }
        let mut node = self.node(source)?.clone();
        node.children.clear();
        node.parent = None;
        let index = self.elements.len();
        self.elements.push(node);
        Ok(index)
    }
    pub fn serialize(&self) -> Result<String> {
        enum Event {
            Open(usize),
            Close(usize),
        }
        let mut output = String::new();
        let mut stack = vec![Event::Open(0)];
        while let Some(event) = stack.pop() {
            match event {
                Event::Open(index) => {
                    let node = self.node(index)?;
                    push(&mut output, "<")?;
                    push(&mut output, &node.tag)?;
                    for (name, value) in &node.attributes {
                        push(&mut output, " ")?;
                        push(&mut output, name)?;
                        push(&mut output, "=\"")?;
                        escaped(&mut output, value, true)?;
                        push(&mut output, "\"")?;
                    }
                    push(&mut output, ">")?;
                    escaped(&mut output, &node.text, false)?;
                    stack.push(Event::Close(index));
                    stack.extend(node.children.iter().rev().copied().map(Event::Open));
                }
                Event::Close(index) => {
                    let node = self.node(index)?;
                    push(&mut output, "</")?;
                    push(&mut output, &node.tag)?;
                    push(&mut output, ">")?;
                    escaped(&mut output, &node.tail, false)?;
                }
            }
        }
        Ok(output)
    }
}

/// Inspect the XML preamble before parsing so entity expansion cannot consume
/// resources and DTD attribute defaults cannot disappear. Quoted external IDs,
/// comments, and processing instructions are inert and remain allowed.
fn reject_dtd_declarations(mut text: &str) -> Result<()> {
    while let Some(start) = text.find('<') {
        text = text.get(start..).ok_or(Error::Limit)?;
        if text.starts_with("<!--") || text.starts_with("<?") {
            let end = if text.starts_with("<!--") {
                "-->"
            } else {
                "?>"
            };
            let Some(index) = text.find(end) else {
                break;
            };
            text = text.get(index + end.len()..).ok_or(Error::Limit)?;
            continue;
        }
        if !text.starts_with("<!DOCTYPE") {
            break;
        }
        let mut cursor = "<!DOCTYPE".len();
        let mut quote = None;
        let mut brackets = 0usize;
        while let Some(&byte) = text.as_bytes().get(cursor) {
            if let Some(delimiter) = quote {
                if byte == delimiter {
                    quote = None;
                }
            } else if byte == b'\'' || byte == b'"' {
                quote = Some(byte);
            } else if byte == b'<' {
                let rest = text.get(cursor..).ok_or(Error::Limit)?;
                if rest.starts_with("<!--") || rest.starts_with("<?") {
                    let end = if rest.starts_with("<!--") {
                        "-->"
                    } else {
                        "?>"
                    };
                    let Some(index) = rest.find(end) else {
                        break;
                    };
                    cursor += index + end.len();
                    continue;
                }
                if ["<!ENTITY", "<!ATTLIST", "<!ELEMENT", "<!NOTATION"]
                    .iter()
                    .any(|p| rest.starts_with(p))
                {
                    return Err(Error::Unsupported("internal DTD declarations"));
                }
            } else if byte == b'[' {
                brackets += 1;
            } else if byte == b']' {
                brackets = brackets.saturating_sub(1);
            } else if byte == b'>' && brackets == 0 {
                cursor += 1;
                break;
            }
            cursor += 1;
        }
        text = text.get(cursor..).ok_or(Error::Limit)?;
    }
    Ok(())
}

fn push(output: &mut String, text: &str) -> Result<()> {
    if output.len().saturating_add(text.len()) > OUTPUT_BYTES {
        return Err(Error::Limit);
    }
    output.push_str(text);
    Ok(())
}
fn escaped(output: &mut String, value: &str, attribute: bool) -> Result<()> {
    for c in value.chars() {
        let mut buffer = [0; 4];
        push(
            output,
            match c {
                '&' => "&amp;",
                '<' => "&lt;",
                '>' => "&gt;",
                '"' if attribute => "&quot;",
                '\n' if attribute => "&#10;",
                '\t' if attribute => "&#9;",
                '\r' => "&#13;",
                _ => c.encode_utf8(&mut buffer),
            },
        )?;
    }
    Ok(())
}
