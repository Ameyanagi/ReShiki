//! Disable unused use references before usvg eagerly expands definitions.
use std::collections::{HashMap, HashSet};

type Node<'a, 'input> = roxmltree::Node<'a, 'input>;

pub(super) fn ids<'a, 'input>(
    xml: &'a roxmltree::Document<'input>,
) -> HashMap<&'a str, Node<'a, 'input>> {
    let mut ids = HashMap::new();
    for node in xml.descendants().filter(|n| n.is_element()) {
        if let Some(id) = node.attribute("id") {
            ids.entry(id).or_insert(node); // Match usvg's first-ID-wins lookup.
        }
    }
    ids
}

pub(super) fn href<'a, 'input>(node: Node<'a, 'input>) -> Option<&'a str> {
    node.attribute(("http://www.w3.org/1999/xlink", "href"))
        .or_else(|| node.attribute("href"))
        .and_then(|value| svgtypes::IRI::from_str(value).ok().map(|iri| iri.0))
}

fn paint_reference(value: &str) -> Option<&str> {
    match svgtypes::Paint::from_str(value) {
        Ok(svgtypes::Paint::FuncIRI(id, _)) => Some(id),
        _ => svgtypes::FuncIRI::from_str(value).ok().map(|iri| iri.0),
    }
}

pub(super) fn remove_unused_use_links(source: &str) -> Result<String, String> {
    let xml = roxmltree::Document::parse(source).map_err(|e| format!("Could not read SVG: {e}"))?;
    let ids = ids(&xml);
    let mut resources: HashMap<&str, Vec<Node<'_, '_>>> = HashMap::new();
    for node in xml.descendants().filter(|n| n.is_element()) {
        if let Some(id) = node.attribute("id") {
            resources.entry(id).or_default().push(node);
        }
    }
    let mut inspected_attributes = HashSet::new();
    let mut pending = vec![(xml.root_element(), false)];
    // Stylesheets apply globally, including when stored inside defs. Retain
    // their resources conservatively, using the same CSS/IRI parsers as usvg.
    for node in xml.descendants().filter(|n| n.has_tag_name("style")) {
        if let Some(text) = node.text() {
            for declaration in simplecss::StyleSheet::parse(text)
                .rules
                .into_iter()
                .flat_map(|r| r.declarations)
            {
                if let Some(id) = paint_reference(declaration.value)
                    && let Some(targets) = resources.get(id)
                {
                    pending.extend(targets.iter().map(|n| (*n, true)));
                }
            }
        }
    }
    let mut seen = HashSet::new();
    while let Some((node, referenced)) = pending.pop() {
        if !referenced && matches!(node.tag_name().name(), "defs" | "symbol") {
            continue;
        }
        if !seen.insert(node.id()) {
            continue;
        }
        if !node.has_tag_name("use") {
            pending.extend(
                node.children()
                    .filter(|n| n.is_element())
                    .map(|n| (n, false)),
            );
        }
        if let Some(id) = href(node) {
            if node.has_tag_name("use") {
                if let Some(target) = ids.get(id) {
                    pending.push((*target, true));
                }
            } else if let Some(targets) = resources.get(id) {
                pending.extend(targets.iter().map(|n| (*n, true)));
            }
        }
        // Marker and paint conversion inherits source-ancestor properties.
        // Inspect those URLs without making ancestor siblings reachable.
        for ancestor in node.ancestors().filter(|n| n.is_element()) {
            if !inspected_attributes.insert(ancestor.id()) {
                continue;
            }
            for attribute in ancestor.attributes() {
                let values: Vec<_> = if attribute.name() == "style" {
                    simplecss::DeclarationTokenizer::from(attribute.value())
                        .map(|d| d.value)
                        .collect()
                } else {
                    vec![attribute.value()]
                };
                for value in values {
                    if let Some(id) = paint_reference(value)
                        && let Some(targets) = resources.get(id)
                    {
                        pending.extend(targets.iter().map(|n| (*n, true)));
                    }
                }
            }
        }
    }
    // Keep the definition elements and paint/style resources intact. Only
    // unused href edges can cause work beyond the bounded source tree.
    let edits = xml
        .descendants()
        .filter(|node| node.has_tag_name("use") && !seen.contains(&node.id()))
        .flat_map(|node| {
            node.attributes()
                .filter(|a| a.name() == "href")
                .map(|a| (a.range(), String::new()))
        })
        .collect();
    Ok(super::css::replace(source, edits))
}
