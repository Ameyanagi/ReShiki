//! Remove unsupported CSS effects without changing selectors or the cascade.
use super::Notices;
use std::ops::Range;

fn effects<'a>(
    css: &'a str,
    declarations: impl IntoIterator<Item = simplecss::Declaration<'a>>,
    notices: &mut Notices,
) -> Option<String> {
    let mut values = Vec::new();
    for declaration in declarations {
        let kind = match declaration.name {
            "filter" => "filter styles",
            "mask" => "mask styles",
            "clip-path" => "clip-path styles",
            _ => continue,
        };
        if declaration.value == "none" {
            continue;
        }
        // simplecss returns slices borrowed from this CSS. Keep the original
        // rule text and replace only the parsed value, including quoted URLs.
        let Some(start) = (declaration.value.as_ptr() as usize).checked_sub(css.as_ptr() as usize)
        else {
            continue;
        };
        let end = start + declaration.value.len();
        if css.get(start..end) == Some(declaration.value) {
            values.push((start, end, kind));
        }
    }
    values.sort_unstable();
    values.dedup(); // A comma-separated selector shares its declarations.
    if values.is_empty() {
        return None;
    }
    let mut clean = css.to_owned();
    for (start, end, kind) in values.into_iter().rev() {
        clean.replace_range(start..end, "none");
        notices.add(kind);
    }
    Some(clean)
}

pub(super) fn replace(source: &str, mut edits: Vec<(Range<usize>, String)>) -> String {
    edits.sort_unstable_by_key(|(range, _)| (range.start, std::cmp::Reverse(range.end)));
    // Whole stylesheet replacements also remove descendant attributes.
    let mut outermost: Vec<(Range<usize>, String)> = Vec::new();
    for edit in edits {
        if outermost
            .last()
            .is_some_and(|(range, _)| edit.0.end <= range.end)
        {
            continue;
        }
        outermost.push(edit);
    }
    let mut clean = source.to_owned();
    for (range, value) in outermost.into_iter().rev() {
        clean.replace_range(range, &value);
    }
    clean
}

pub(super) fn sanitise(source: &str, notices: &mut Notices) -> Result<String, String> {
    let xml = roxmltree::Document::parse(source).map_err(|e| format!("Could not read SVG: {e}"))?;
    let mut edits = Vec::new();
    for node in xml.descendants().filter(|n| n.is_element()) {
        if let Some(attr) = node
            .attributes()
            .find(|a| a.namespace().is_none() && a.name() == "style")
            && let Some(clean) = effects(
                attr.value(),
                simplecss::DeclarationTokenizer::from(attr.value()),
                notices,
            )
        {
            edits.push((
                attr.range(),
                format!("style=\"{}\"", quick_xml::escape::escape(&clean)),
            ));
        }
        if node.has_tag_name("style")
            && node.attribute("type").is_none_or(|kind| kind == "text/css")
            && let Some(text) = node.first_child().filter(|n| n.is_text())
            && let Some(css) = text.text()
        {
            let declarations = simplecss::StyleSheet::parse(css)
                .rules
                .into_iter()
                .flat_map(|rule| rule.declarations);
            if let Some(clean) = effects(css, declarations, notices) {
                // Adjacent text and CDATA can share one logical text value
                // but only the first lexical range. Replace the full content.
                let range = node.range();
                let raw = source.get(range.clone()).ok_or("Invalid SVG style range")?;
                let mut reader = quick_xml::Reader::from_str(raw);
                reader.read_event().map_err(|e| e.to_string())?;
                let start = usize::try_from(reader.buffer_position()).map_err(|e| e.to_string())?;
                let end = raw.rfind("</").ok_or("Invalid SVG style end")?;
                edits.push((
                    range.start + start..range.start + end,
                    quick_xml::escape::escape(&clean).into_owned(),
                ));
            }
        }
    }
    Ok(replace(source, edits))
}
