//! Parent-locant insertion and transient name-to-atom feedback.
use super::*;
use iced::widget::{mouse_area, rich_text, text::Span};
use reshiki::atom_labels::{self as labels, Number};
use std::{borrow::Cow, collections::HashMap, ops::Range};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocantTarget {
    Preview,
    Structure,
}

#[derive(Debug, Clone)]
pub(super) struct Hover {
    target: LocantTarget,
    /// The hovered token's byte offset; one locant can appear several times.
    token: usize,
    locant: String,
    atoms: Vec<u64>,
}

#[derive(Debug, PartialEq, Eq)]
struct Token {
    range: Range<usize>,
    locant: String,
}

fn superscript(c: char) -> bool {
    "⁰¹²³⁴⁵⁶⁷⁸⁹".contains(c)
}

fn prime(c: char) -> bool {
    matches!(c, '\'' | '’' | '′' | '″' | '‴')
}

/// Conservative parent locants: nested substituent/fusion descriptions and
/// primed numbering are deliberately outside this first hover implementation.
fn tokens(name: &str) -> Vec<Token> {
    let mut chars = name.char_indices().peekable();
    let mut tokens = vec![];
    let mut depth = 0_usize;
    let mut previous: Option<char> = None;
    while let Some((begin, c)) = chars.next() {
        let preceding = previous;
        previous = Some(c);
        if matches!(c, '(' | '[' | '{') {
            depth += 1;
        } else if matches!(c, ')' | ']' | '}') {
            depth = depth.saturating_sub(1);
        }
        if depth != 0
            || !c.is_ascii_digit()
            || preceding.is_some_and(|c| c.is_alphanumeric() || prime(c))
        {
            continue;
        }
        while let Some((_, c)) = chars.next_if(|(_, c)| c.is_ascii_digit()) {
            previous = Some(c);
        }
        while let Some((_, c)) = chars.next_if(|(_, c)| c.is_ascii_lowercase()) {
            previous = Some(c);
        }
        while let Some((_, c)) = chars.next_if(|(_, c)| superscript(*c)) {
            previous = Some(c);
        }
        let core_end = chars.peek().map_or(name.len(), |(at, _)| *at);
        if chars.next_if(|(_, c)| *c == 'H').is_some() {
            previous = Some('H');
        } else if chars.next_if(|(_, c)| *c == 'λ').is_some() {
            previous = Some('λ');
            while let Some((_, c)) = chars.next_if(|(_, c)| superscript(*c) || c.is_ascii_digit()) {
                previous = Some(c);
            }
        }
        if chars
            .peek()
            .is_some_and(|(_, c)| c.is_alphanumeric() || prime(*c))
        {
            continue;
        }
        let end = chars.peek().map_or(name.len(), |(at, _)| *at);
        if let Some(locant) = name.get(begin..core_end) {
            tokens.push(Token {
                range: begin..end,
                locant: locant.into(),
            });
        }
    }
    tokens
}

pub(super) fn insertion_document(preview: &Preview, numbered: bool) -> Document {
    let mut document = preview.document.clone();
    if numbered {
        for (&id, locant) in &preview.locants {
            if let Some(atom) = document.atom_mut(id) {
                atom.display.number = Some(Number {
                    text: locant.clone(),
                    offset: None,
                    style: labels::number_style(),
                });
            }
        }
    }
    document
}

/// Each atom's displayed parent locant, when `document` is the named graph.
pub(super) fn display_locants(record: &Record, document: &Document) -> HashMap<u64, String> {
    record
        .locants
        .as_ref()
        .map(|locants| {
            locants
                .for_document(document)
                .into_iter()
                .map(|(id, locant)| (id, naming::locant_text(&locant)))
                .collect()
        })
        .unwrap_or_default()
}

/// Slice joiner-free spans by display offsets, restoring joiners within the result.
fn range_spans(spans: &[Span<'static>], range: Range<usize>) -> Vec<Span<'static>> {
    let mut offset = 0;
    let mut parts = spans
        .iter()
        .filter_map(|span| {
            let start = offset;
            offset += span.text.len();
            let (begin, finish) = (start.max(range.start), offset.min(range.end));
            (begin < finish).then(|| {
                let mut part = span.clone();
                part.text = Cow::Owned(span.text[begin - start..finish - start].into());
                part
            })
        })
        .peekable();
    let mut joined = vec![];
    while let Some(mut part) = parts.next() {
        if !part.text.ends_with(char::is_whitespace)
            && parts
                .peek()
                .is_some_and(|next| next.text.starts_with(|c: char| !c.is_whitespace()))
        {
            part.text.to_mut().push('\u{2060}');
        }
        joined.push(part);
    }
    joined
}

fn hover_segment<'a, R: iced::advanced::Renderer + 'a>(
    content: impl Into<Element<'a, Message, iced::Theme, R>>,
    target: LocantTarget,
    token: usize,
    locant: &str,
) -> Element<'a, Message, iced::Theme, R> {
    let action = |hovered| {
        Message::Naming(Action::HoverLocant {
            target,
            token,
            locant: locant.into(),
            hovered,
        })
    };
    mouse_area(content)
        .on_enter(action(true))
        .on_exit(action(false))
        .into()
}

impl App {
    /// A hidden name never sends its exit, so its highlight ends with the panel.
    pub(in crate::app) fn invalidate_locant_hover(&mut self) {
        let Some(hover) = &self.tab.naming.locant_hover else {
            return;
        };
        let visible = self.inspector_open
            && match (self.inspector_tab, hover.target) {
                (InspectorTab::Names, _) => true,
                (InspectorTab::Import, LocantTarget::Preview) => self.tab.naming.preview_open,
                (InspectorTab::Import, LocantTarget::Structure) => self.tab.naming.details_open,
                _ => false,
            };
        if !visible {
            self.tab.naming.locant_hover = None;
        }
    }

    pub(super) fn name_locant_checkbox(
        &self,
        id: &'static str,
        enabled: bool,
    ) -> Element<'_, Message> {
        accessible_checkbox(
            id,
            "Number atoms by name locants",
            self.tab.naming.number_by_locants,
            enabled.then_some(Message::Naming(Action::NumberByLocants(
                !self.tab.naming.number_by_locants,
            ))),
        )
        .into()
    }

    /// The displayed name's atom locants while that name still describes them.
    fn locant_map(&self, target: LocantTarget) -> Option<&HashMap<u64, String>> {
        let state = &self.tab.naming;
        match target {
            LocantTarget::Preview => state.preview.as_ref().map(|preview| &preview.locants),
            LocantTarget::Structure => state
                .structure
                .as_ref()
                .filter(|(ticket, _)| ticket.is_current(self.tab.revision, self.tab.file_epoch))
                .map(|_| &state.structure_locants),
        }
    }

    pub(super) fn hover_name_locant(
        &mut self,
        target: LocantTarget,
        token: usize,
        locant: String,
        hovered: bool,
    ) {
        let state = &self.tab.naming;
        if !hovered {
            // Moving between tokens can deliver the old token's exit last.
            if state
                .locant_hover
                .as_ref()
                .is_some_and(|hover| hover.target == target && hover.token == token)
            {
                self.tab.naming.locant_hover = None;
            }
            return;
        }
        let mut atoms: Vec<_> = self
            .locant_map(target)
            .into_iter()
            .flatten()
            .filter_map(|(&id, name)| (*name == locant).then_some(id))
            .collect();
        atoms.sort_unstable();
        self.tab.naming.locant_hover = (!atoms.is_empty()).then_some(Hover {
            target,
            token,
            locant,
            atoms,
        });
    }

    fn current_locant_hover(&self, target: LocantTarget) -> Option<&Hover> {
        self.tab
            .naming
            .locant_hover
            .as_ref()
            .filter(|hover| hover.target == target && self.locant_map(target).is_some())
    }

    pub(in crate::app) fn naming_canvas_highlight(&self) -> &[u64] {
        self.current_locant_hover(LocantTarget::Structure)
            .map_or(&[], |hover| &hover.atoms)
    }

    pub(super) fn name_preview_highlight(&self) -> &[u64] {
        self.current_locant_hover(LocantTarget::Preview)
            .map_or(&[], |hover| &hover.atoms)
    }

    pub(super) fn locant_name<'a>(
        &'a self,
        name: &str,
        target: LocantTarget,
        size: f32,
    ) -> Element<'a, Message> {
        let family = reshiki::style::font_name(&self.tab.doc.drawing_style.font_family);
        let mut spans = super::aliases::chemical_name_spans(name, size, family);
        let displayed = reshiki::typography::name_markup(name).unicode().text;
        let available = self.locant_map(target);
        let tokens: Vec<_> = tokens(&displayed)
            .into_iter()
            .filter(|token| {
                available.is_some_and(|map| map.values().any(|locant| *locant == token.locant))
            })
            .collect();
        if tokens.is_empty() {
            return rich_text(spans).into();
        }
        for span in &mut spans {
            span.text = Cow::Owned(span.text.replace('\u{2060}', ""));
        }
        let hovered = self.current_locant_hover(target);
        let mut segments: Vec<Element<'a, Message>> = vec![];
        let mut end = 0;
        // Break plain text after separators so long names still wrap.
        let plain = |segments: &mut Vec<Element<'a, Message>>, range: Range<usize>| {
            let mut start = range.start;
            for (at, c) in displayed[range.clone()].char_indices() {
                if matches!(c, '-' | ',' | ' ') {
                    let end = range.start + at + c.len_utf8();
                    segments.push(rich_text(range_spans(&spans, start..end)).into());
                    start = end;
                }
            }
            if start < range.end {
                segments.push(rich_text(range_spans(&spans, start..range.end)).into());
            }
        };
        for token in tokens {
            plain(&mut segments, end..token.range.start);
            let mut segment = rich_text(range_spans(&spans, token.range.clone()));
            if hovered.is_some_and(|hover| hover.locant == token.locant) {
                segment = segment.color(crate::canvas::overlay::ACCENT_STRONG);
            }
            segments.push(hover_segment(
                segment,
                target,
                token.range.start,
                &token.locant,
            ));
            end = token.range.end;
        }
        plain(&mut segments, end..displayed.len());
        row(segments).spacing(0).wrap().vertical_spacing(0).into()
    }
}

#[cfg(test)]
mod tests;
