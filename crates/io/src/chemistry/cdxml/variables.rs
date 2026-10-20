//! Drawing-only labels on unspecified atoms; query chemistry remains unsupported.
use super::{
    PreparedCdxml, Result,
    presentation::{self, Attributes, NativeSpan, NativeText, TextReader},
    scene::{MANUAL_LABEL_SCRIPTS, imported_label_scripts},
    tree::Tree,
};
use crate::typography::ScriptRun;
use std::collections::HashMap;

pub(crate) struct Variable {
    text: String,
    scripts: Vec<ScriptRun>,
    attachment: crate::atom_labels::LabelAttachment,
}
pub(crate) struct Variables(pub HashMap<u32, Variable>);

fn scripts(node: roxmltree::Node<'_, '_>, reader: &TextReader<'_, '_>) -> Result<Vec<ScriptRun>> {
    let defaults: Attributes = node
        .attributes()
        .map(|attribute| (attribute.name().into(), attribute.value().into()))
        .collect();
    let mut label: Option<NativeText> = None;
    let mut manual = false;
    for node in node.children().filter(|node| node.has_tag_name("t")) {
        manual |= node.attribute("Name") == Some(MANUAL_LABEL_SCRIPTS);
        let text = reader
            .read(node, Some(&defaults), true)
            .map_err(|error| super::Error::Invalid(error.to_string()))?;
        let Some(label) = &mut label else {
            label = Some(text);
            continue;
        };
        // Unspecified labels may contain several text objects. Carry their
        // scripts into the same concatenated source bytes as the legacy reader.
        let offset = label.text.len();
        let mut end = 0;
        for span in &text.format.spans {
            if end < span.start {
                label.format.spans.push(NativeSpan {
                    start: offset + end,
                    end: offset + span.start,
                    style: text.format.style.clone(),
                });
            }
            label.format.spans.push(NativeSpan {
                start: offset + span.start,
                end: offset + span.end,
                style: span.style.clone(),
            });
            end = span.end;
        }
        if end < text.text.len() {
            label.format.spans.push(NativeSpan {
                start: offset + end,
                end: offset + text.text.len(),
                style: text.format.style,
            });
        }
        label.text.push_str(&text.text);
    }
    label.map_or_else(
        || Ok(Vec::new()),
        |label| {
            imported_label_scripts(&label, manual)
                .map_err(|error| super::Error::Invalid(error.to_string()))
        },
    )
}

pub(crate) fn drawing_variables(xml: &str) -> Result<(String, Variables)> {
    let mut tree = Tree::parse_import(xml)?;
    let mut labels = HashMap::new();
    let mut unspecified = false;
    for index in tree.descendants(0)? {
        let node = tree.node(index)?;
        if node.tag != "n" {
            continue;
        }
        let missing_element =
            node.attr("Element").is_none() && tree.children(index, "fragment")?.is_empty();
        if node.attr("NodeType") == Some("Unspecified")
            && (node.attr("Element") == Some("0") || missing_element)
        {
            let mut label = String::new();
            // The text's justification, else one inherited from its node,
            // fragment, page or document.
            let mut owner = tree.children(index, "t")?.first().copied().or(Some(index));
            let mut justification = None;
            while let Some(current) = owner.filter(|_| justification.is_none()) {
                let element = tree.node(current)?;
                justification = element.attr("LabelJustification").map(str::to_owned);
                owner = element.parent;
            }
            let attachment = justification
                .as_deref()
                .map(crate::atom_labels::LabelAttachment::from_cdxml)
                .unwrap_or_default();
            for text in tree.children(index, "t")? {
                for run in tree.children(text, "s")? {
                    label.push_str(&tree.node(run)?.text);
                }
            }
            if !label.trim().is_empty() && label != "*" {
                if label.chars().count() > 32 || label.chars().any(char::is_control) {
                    return Err(super::Error::Invalid(
                        "Unsupported unspecified atom label".into(),
                    ));
                }
                if let Some(id) = node.attr("id").and_then(|s| s.parse::<u32>().ok()) {
                    unspecified = true;
                    labels.insert(
                        id,
                        Variable {
                            text: label,
                            scripts: Vec::new(),
                            attachment,
                        },
                    );
                    if missing_element {
                        // ChemDraw can omit zero for an unspecified label.
                        // A nested fragment is a molecular definition instead;
                        // do not reinterpret those wrappers as free text.
                        tree.node_mut(index)?.set("Element", "0".into());
                    }
                }
            }
            continue;
        }
        if node.attr("NodeType") != Some("GenericNickname") {
            continue;
        }
        let label = node.attr("GenericNickname").unwrap_or("");
        if !matches!(label, "R" | "X")
            || ["Element", "Charge", "Isotope", "NumHydrogens"]
                .iter()
                .any(|key| node.attr(key).is_some_and(|v| v != "0"))
        {
            continue;
        }
        let Some(id) = node.attr("id").and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        let label = label.to_owned();
        let node = tree.node_mut(index)?;
        node.set("NodeType", "Unspecified".into());
        node.set("Element", "0".into());
        node.set("NumHydrogens", "0".into());
        node.attributes.retain(|(key, _)| key != "GenericNickname");
        labels.insert(
            id,
            Variable {
                text: label,
                scripts: Vec::new(),
                attachment: Default::default(),
            },
        );
    }
    if unspecified {
        let native =
            presentation::parse(xml).map_err(|error| super::Error::Invalid(error.to_string()))?;
        let reader = TextReader::new(native.root_element())
            .map_err(|error| super::Error::Invalid(error.to_string()))?;
        for node in native.descendants().filter(|node| {
            node.has_tag_name("n")
                && node.attribute("NodeType") == Some("Unspecified")
                && (node.attribute("Element") == Some("0")
                    || node.attribute("Element").is_none()
                        && !node.children().any(|child| child.has_tag_name("fragment")))
        }) {
            if let Some(label) = node
                .attribute("id")
                .and_then(|id| id.parse::<u32>().ok())
                .and_then(|id| labels.get_mut(&id))
            {
                label.scripts = scripts(node, &reader)?;
            }
        }
    }
    Ok((
        if labels.is_empty() {
            xml.into()
        } else {
            tree.serialize()?
        },
        Variables(labels),
    ))
}

impl Variables {
    pub(crate) fn restore(
        &self,
        prepared: &PreparedCdxml,
        document: &mut crate::document::Document,
    ) -> Result<()> {
        let sources = prepared.fragments.iter().flat_map(|f| &f.atom_ids);
        for (&source, &id) in sources.zip(&prepared.molecule.ids) {
            if let Some(label) = self.0.get(&source) {
                let atom = document.atom_mut(id).ok_or(super::Error::Limit)?;
                atom.display.set_variable(Some(label.text.clone()));
                atom.display.label_scripts = label.scripts.clone();
                atom.display.label_attachment = label.attachment;
                // ChemDraw's LabelDisplay on free text is its flush choice,
                // not a side for hydrogens this label does not have.
                atom.display.hydrogen_position = crate::atom_labels::HydrogenPosition::Auto;
                atom.element = "*".into();
                atom.no_implicit = true;
                atom.label_h = 0;
            }
        }
        Ok(())
    }
}
