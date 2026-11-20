use crate::exchange::{is_symbol_font, symbol_char};
use std::{borrow::Cow, collections::HashSet};

/// Decode legacy Symbol runs before drawing readers interpret their text and styles.
pub(crate) fn symbol_text_to_unicode(xml: &str) -> Cow<'_, str> {
    // Most drawings have no Symbol font; skip parsing them.
    if !xml
        .as_bytes()
        .windows(6)
        .any(|w| w.eq_ignore_ascii_case(b"symbol"))
    {
        return Cow::Borrowed(xml);
    }
    let Ok(document) = super::presentation::parse(xml) else {
        return Cow::Borrowed(xml);
    };
    let mut fonts = HashSet::new();
    for font in document
        .root_element()
        .children()
        .filter(|node| node.has_tag_name("fonttable"))
        .flat_map(|table| table.children().filter(|node| node.has_tag_name("font")))
    {
        let Some(id) = font.attribute("id") else {
            continue;
        };
        if font.attribute("name").is_some_and(is_symbol_font)
            && !font
                .attribute("charset")
                .is_some_and(|name| name.eq_ignore_ascii_case("utf-8"))
        {
            fonts.insert(id);
        } else {
            fonts.remove(id);
        }
    }
    if fonts.is_empty() {
        return Cow::Borrowed(xml);
    }

    let mut edits = Vec::new();
    for run in document.descendants().filter(|node| node.has_tag_name("s")) {
        let Some(font) = run
            .attribute_node("font")
            .filter(|a| fonts.contains(a.value()))
        else {
            continue;
        };
        edits.push((font.range(), String::new()));
        for node in run.children().filter(|node| node.is_text()) {
            let text = node.text().unwrap_or_default();
            if !text.chars().any(|ch| symbol_char(ch) != ch) {
                continue;
            }
            let range = node.range();
            // roxmltree joins adjacent text and CDATA but keeps the first
            // token's range. The next sibling or closing tag ends the text.
            let end = node.next_sibling().map_or_else(
                || xml[..run.range().end].rfind('<').unwrap_or(range.end),
                |next| next.range().start,
            );
            let mut escaped = String::with_capacity(text.len());
            for ch in text.chars().map(symbol_char) {
                match ch {
                    '&' => escaped.push_str("&amp;"),
                    '<' => escaped.push_str("&lt;"),
                    '>' => escaped.push_str("&gt;"),
                    '\r' => escaped.push_str("&#13;"),
                    _ => escaped.push(ch),
                }
            }
            edits.push((range.start..end, escaped));
        }
    }
    if edits.is_empty() {
        return Cow::Borrowed(xml);
    }
    edits.sort_unstable_by_key(|(range, _)| range.start);
    let mut output = String::with_capacity(xml.len());
    let mut copied = 0;
    for (range, replacement) in edits {
        output.push_str(&xml[copied..range.start]);
        output.push_str(&replacement);
        copied = range.end;
    }
    output.push_str(&xml[copied..]);
    Cow::Owned(output)
}
