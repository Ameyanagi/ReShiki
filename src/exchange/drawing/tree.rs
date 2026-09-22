use super::{Error, Result, invalid};
pub(super) type Key = usize;
pub(super) struct Node {
    pub tag: &'static str,
    pub attrs: Vec<(&'static str, String)>,
    pub children: Vec<Key>,
    pub parent: Option<Key>,
    pub text: Option<String>,
}
pub(super) struct Tree {
    nodes: Vec<Node>,
}
impl Tree {
    pub fn new() -> Self {
        Self {
            nodes: vec![Node {
                tag: "CDXML",
                attrs: vec![],
                children: vec![],
                parent: None,
                text: None,
            }],
        }
    }
    pub fn node(&self, k: Key) -> Result<&Node> {
        self.nodes.get(k).ok_or_else(|| invalid("Missing XML node"))
    }
    pub fn node_mut(&mut self, k: Key) -> Result<&mut Node> {
        self.nodes
            .get_mut(k)
            .ok_or_else(|| invalid("Missing XML node"))
    }
    pub fn set(&mut self, k: Key, name: &'static str, value: impl Into<String>) -> Result<()> {
        let attrs = &mut self.node_mut(k)?.attrs;
        if let Some((_, v)) = attrs.iter_mut().find(|(n, _)| *n == name) {
            *v = value.into();
        } else {
            attrs.push((name, value.into()));
        }
        Ok(())
    }
    pub fn get(&self, k: Key, name: &str) -> Result<Option<&str>> {
        Ok(self
            .node(k)?
            .attrs
            .iter()
            .find(|(n, _)| *n == name)
            .map(|(_, v)| v.as_str()))
    }
    pub fn value(&self, k: Key, name: &str) -> Result<String> {
        self.get(k, name)?
            .map(str::to_owned)
            .ok_or_else(|| invalid(format!("Missing XML {name}")))
    }
    pub fn add(
        &mut self,
        parent: Option<Key>,
        tag: &'static str,
        attrs: impl IntoIterator<Item = (&'static str, String)>,
    ) -> Result<Key> {
        if self.nodes.len() >= super::super::MAX_OBJECTS {
            return Err(Error::Limit);
        }
        let k = self.nodes.len();
        self.nodes.push(Node {
            tag,
            attrs: attrs.into_iter().collect(),
            children: vec![],
            parent: None,
            text: None,
        });
        if let Some(p) = parent {
            self.attach(p, k)?;
        }
        Ok(k)
    }
    pub fn attach(&mut self, parent: Key, child: Key) -> Result<()> {
        if parent == child {
            return Err(invalid("XML cycle"));
        }
        if let Some(old) = self.node(child)?.parent {
            self.node_mut(old)?.children.retain(|k| *k != child);
        }
        self.node_mut(parent)?.children.push(child);
        self.node_mut(child)?.parent = Some(parent);
        Ok(())
    }
    pub fn descendants(&self, k: Key) -> Result<Vec<Key>> {
        let mut stack = vec![k];
        let mut result = Vec::new();
        let mut seen = std::collections::HashSet::new();
        while let Some(k) = stack.pop() {
            if !seen.insert(k) {
                return Err(invalid("XML cycle or duplicate child"));
            }
            result.push(k);
            stack.extend(self.node(k)?.children.iter().rev());
        }
        Ok(result)
    }
    pub fn serialize(&self) -> Result<String> {
        let mut output = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
        let mut stack = vec![(0, false, 0usize)];
        let mut seen = std::collections::HashSet::new();
        while let Some((k, close, depth)) = stack.pop() {
            if depth > 64 {
                return Err(Error::Limit);
            }
            let n = self.node(k)?;
            if close {
                push(&mut output, &format!("</{}>", n.tag))?;
                continue;
            }
            if !seen.insert(k) {
                return Err(invalid("XML cycle or duplicate child"));
            }
            push(&mut output, &format!("<{}", n.tag))?;
            for (key, value) in &n.attrs {
                push(&mut output, &format!(" {key}=\""))?;
                escaped(&mut output, value, true)?;
                push(&mut output, "\"")?;
            }
            if n.children.is_empty() && n.text.as_deref().unwrap_or("").is_empty() {
                push(&mut output, " />")?;
                continue;
            }
            push(&mut output, ">")?;
            if let Some(text) = &n.text {
                escaped(&mut output, text, false)?;
            }
            stack.push((k, true, depth));
            stack.extend(n.children.iter().rev().map(|&k| (k, false, depth + 1)));
        }
        Ok(output)
    }
}
fn push(out: &mut String, value: &str) -> Result<()> {
    if value.len() > super::super::LIMIT.saturating_sub(out.len()) {
        return Err(Error::Limit);
    }
    out.push_str(value);
    Ok(())
}
fn escaped(out: &mut String, value: &str, attribute: bool) -> Result<()> {
    for c in value.chars() {
        match c {
            '&' => push(out, "&amp;")?,
            '<' => push(out, "&lt;")?,
            '>' => push(out, "&gt;")?,
            '"' if attribute => push(out, "&quot;")?,
            '\n' if attribute => push(out, "&#10;")?,
            '\r' if attribute => push(out, "&#13;")?,
            '\t' if attribute => push(out, "&#09;")?,
            _ => {
                if c.is_control() && !matches!(c, '\n' | '\r' | '\t') {
                    return Err(invalid("Invalid XML text character"));
                }
                let mut bytes = [0; 4];
                push(out, c.encode_utf8(&mut bytes))?;
            }
        }
    }
    Ok(())
}
