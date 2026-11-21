//! Names and catalogue metadata in the detached equipment draft.
use super::*;
use iced::{Length, widget::column};
use reshiki::equipment::format::{Category, Credits, Definition};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy)]
pub enum Field {
    Author,
    License,
    Keyword(u64),
    VariantName(u64),
    VariantValue(u64),
}

#[derive(Debug, Clone)]
pub enum Action {
    Text(Field, String),
    Category(Category),
    AddKeyword,
    RemoveKeyword(u64),
    AddVariant,
    RemoveVariant(u64),
}

#[derive(Clone, PartialEq)]
struct Entry {
    id: u64,
    name: String,
    value: String,
}

impl Entry {
    fn new(name: String, value: String) -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT: AtomicU64 = AtomicU64::new(1);
        Self {
            id: NEXT.fetch_add(1, Ordering::Relaxed),
            name,
            value,
        }
    }
}

#[derive(Clone, PartialEq)]
pub(super) struct Metadata {
    category: Category,
    keywords: Vec<Entry>,
    variants: Vec<Entry>,
    author: String,
    license: String,
}

impl Metadata {
    pub(super) fn from_definition(d: &Definition) -> Self {
        Self {
            category: d.category,
            keywords: d
                .keywords
                .iter()
                .map(|s| Entry::new(String::new(), s.clone()))
                .collect(),
            variants: d
                .variant
                .iter()
                .map(|(k, v)| Entry::new(k.clone(), v.clone()))
                .collect(),
            author: d.credits.author.clone().unwrap_or_default(),
            license: d.credits.license.clone().unwrap_or_default(),
        }
    }

    pub(super) fn apply(&self, d: &mut Definition) -> Result<(), String> {
        // Preserve entered text; the definition validator checks its limits
        // and control characters. Empty editor rows carry no saved metadata.
        let optional = |s: &str| (!s.is_empty()).then(|| s.to_owned());
        // Localized names stay in the existing definition, outside this form.
        d.category = self.category;
        d.keywords = self
            .keywords
            .iter()
            .filter_map(|e| optional(&e.value))
            .collect();
        let mut variants = BTreeMap::new();
        for entry in &self.variants {
            let name = entry.name.as_str();
            let value = entry.value.as_str();
            if name.is_empty() {
                if value.is_empty() {
                    continue;
                }
                return Err("Enter a name for the variant field.".into());
            }
            if variants.insert(name.to_owned(), value.to_owned()).is_some() {
                return Err(format!("The variant field \"{name}\" is listed twice."));
            }
        }
        d.variant = variants;
        d.credits = Credits {
            author: optional(&self.author),
            license: optional(&self.license),
        };
        reshiki::equipment::validate::validate(d)
    }

    fn edit(&mut self, action: Action) {
        match action {
            Action::Category(category) => self.category = category,
            Action::AddKeyword => self.keywords.push(Entry::new(String::new(), String::new())),
            Action::AddVariant => self.variants.push(Entry::new(String::new(), String::new())),
            Action::RemoveKeyword(id) => self.keywords.retain(|e| e.id != id),
            Action::RemoveVariant(id) => self.variants.retain(|e| e.id != id),
            Action::Text(field, value) => {
                let input = match field {
                    Field::Author => Some(&mut self.author),
                    Field::License => Some(&mut self.license),
                    Field::Keyword(id) => self
                        .keywords
                        .iter_mut()
                        .find(|e| e.id == id)
                        .map(|e| &mut e.value),
                    Field::VariantName(id) => self
                        .variants
                        .iter_mut()
                        .find(|e| e.id == id)
                        .map(|e| &mut e.name),
                    Field::VariantValue(id) => self
                        .variants
                        .iter_mut()
                        .find(|e| e.id == id)
                        .map(|e| &mut e.value),
                };
                if let Some(input) = input {
                    *input = value;
                }
            }
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct Choice(Category, &'static str);
impl std::fmt::Display for Choice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.1)
    }
}
const CATEGORIES: [Choice; 13] = [
    Choice(Category::Flask, "Flask"),
    Choice(Category::Schlenk, "Schlenk ware"),
    Choice(Category::Condenser, "Condenser"),
    Choice(Category::Distillation, "Distillation"),
    Choice(Category::Adapter, "Adapter"),
    Choice(Category::Addition, "Stopper or addition"),
    Choice(Category::Filtration, "Filtration"),
    Choice(Category::Nmr, "NMR tube"),
    Choice(Category::Chromatography, "Chromatography"),
    Choice(Category::Vessel, "Vessel"),
    Choice(Category::Heating, "Heating"),
    Choice(Category::Holding, "Clamp or stand"),
    Choice(Category::Other, "Other"),
];

// Scope widget lookup as well as emitted messages: native requests may wait
// either before or after their live-widget operation.
fn control_id(draft: &str, id: &str) -> String {
    format!("{id}-{draft}")
}

fn control_message(draft: &str, action: super::Action) -> Message {
    Message::EquipmentAuthoring(super::Action::MetadataControl {
        draft: draft.into(),
        action: Box::new(action),
    })
}

fn message(draft: &str, action: Action) -> Message {
    control_message(draft, super::Action::Metadata(action))
}

fn input<'a>(
    draft: &str,
    id: String,
    label: String,
    value: &str,
    field: Field,
) -> Element<'a, Message> {
    let id = control_id(draft, &id);
    let draft = draft.to_owned();
    column![
        text(label.clone()).size(12),
        reshiki::accessibility::text_input(id, label, "", value)
            .style(crate::appearance::input_style)
            .on_input(move |value| message(&draft, Action::Text(field, value)))
            .on_submit(Message::ContextKey("Enter".into()))
            .padding(7)
            .size(12)
            .width(Length::Fill),
    ]
    .spacing(4)
    .width(Length::Fill)
    .into()
}

fn button(
    draft: &str,
    id: String,
    label: String,
    caption: &'static str,
    action: Action,
) -> Element<'static, Message> {
    reshiki::accessibility::button(control_id(draft, &id), label, text(caption).size(12))
        .style(super::super::workspace::control(false))
        .padding([5, 7])
        .on_press(message(draft, action))
        .into()
}

impl State {
    pub(super) fn metadata_control_id(&self, id: &str) -> String {
        control_id(&self.identity, id)
    }

    pub(super) fn metadata_control_message(&self, action: super::Action) -> Message {
        control_message(&self.identity, action)
    }

    pub(super) fn edit_metadata(&mut self, action: Action) {
        let mut next = self.current.metadata.clone();
        next.edit(action);
        if next != self.current.metadata {
            self.checkpoint();
            self.current.metadata = next;
        }
    }

    pub(super) fn metadata_panel(&self) -> Element<'_, Message> {
        let m = &self.current.metadata;
        let category = CATEGORIES.iter().find(|v| v.0 == m.category).copied();
        let choose = |c: Choice| message(&self.identity, Action::Category(c.0));
        let mut body = column![
            section("Piece details"),
            text("Category").size(12),
            reshiki::accessibility::select(
                self.metadata_control_id("equipment-metadata-category"),
                "Equipment category",
                CATEGORIES.to_vec(),
                category.map_or("", |c| c.1),
                choose,
                crate::appearance::pick_list(CATEGORIES.as_slice(), category, choose).text_size(12),
            ),
            text("Keywords").size(12),
        ]
        .spacing(7);
        for (i, entry) in m.keywords.iter().enumerate() {
            body = body.push(
                row![
                    input(
                        &self.identity,
                        format!("equipment-keyword-{}", entry.id),
                        format!("Keyword {}", i + 1),
                        &entry.value,
                        Field::Keyword(entry.id)
                    ),
                    button(
                        &self.identity,
                        format!("equipment-remove-keyword-{}", entry.id),
                        format!("Remove keyword {}", i + 1),
                        "Remove",
                        Action::RemoveKeyword(entry.id)
                    ),
                ]
                .spacing(5)
                .align_y(iced::Alignment::End),
            );
        }
        body = body
            .push(button(
                &self.identity,
                "equipment-add-keyword".into(),
                "Add keyword".into(),
                "Add keyword",
                Action::AddKeyword,
            ))
            .push(text("Variant fields").size(12));
        for (i, entry) in m.variants.iter().enumerate() {
            body = body.push(
                column![
                    input(
                        &self.identity,
                        format!("equipment-variant-name-{}", entry.id),
                        format!("Variant {} name", i + 1),
                        &entry.name,
                        Field::VariantName(entry.id)
                    ),
                    row![
                        input(
                            &self.identity,
                            format!("equipment-variant-value-{}", entry.id),
                            format!("Variant {} value", i + 1),
                            &entry.value,
                            Field::VariantValue(entry.id)
                        ),
                        button(
                            &self.identity,
                            format!("equipment-remove-variant-{}", entry.id),
                            format!("Remove variant {}", i + 1),
                            "Remove",
                            Action::RemoveVariant(entry.id)
                        ),
                    ]
                    .spacing(5)
                    .align_y(iced::Alignment::End),
                ]
                .spacing(4),
            );
        }
        body.push(button(
            &self.identity,
            "equipment-add-variant".into(),
            "Add variant field".into(),
            "Add variant field",
            Action::AddVariant,
        ))
        .push(input(
            &self.identity,
            "equipment-metadata-author".into(),
            "Author (optional)".into(),
            &m.author,
            Field::Author,
        ))
        .push(input(
            &self.identity,
            "equipment-metadata-license".into(),
            "Licence (optional)".into(),
            &m.license,
            Field::License,
        ))
        .into()
    }
}

#[cfg(test)]
#[path = "metadata_tests.rs"]
mod tests;
