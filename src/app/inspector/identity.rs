//! One-click molecular identity, using the Properties analysis and naming caches.
use super::super::icons::{Glyph, Icon};
use super::*;
use iced::widget::{Space, canvas, column, hover, responsive};
use reshiki::linked_properties::Property;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdentityField {
    Name,
    SystematicName,
    Formula,
    Weight,
    Smiles,
    Inchi,
    InchiKey,
    Analysis,
}
impl IdentityField {
    fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::SystematicName => "Systematic name",
            Self::Formula => "Formula",
            Self::Weight => "Mol. Wt.",
            Self::Smiles => "SMILES",
            Self::Inchi => "InChI",
            Self::InchiKey => "InChIKey",
            Self::Analysis => "Elemental analysis",
        }
    }
    fn id(self) -> &'static str {
        match self {
            Self::Name => "identity-copy-name",
            Self::SystematicName => "identity-copy-systematic-name",
            Self::Formula => "identity-copy-formula",
            Self::Weight => "identity-copy-weight",
            Self::Smiles => "identity-copy-smiles",
            Self::Inchi => "identity-copy-inchi",
            Self::InchiKey => "identity-copy-inchikey",
            Self::Analysis => "identity-copy-analysis",
        }
    }
}

#[derive(Debug, Clone)]
pub struct IdentityCopyKey {
    epoch: u64,
    revision: u64,
    atoms: Vec<u64>,
    field: IdentityField,
}

async fn write_identity(field: IdentityField, text: String) -> Result<(), String> {
    #[cfg(target_arch = "wasm32")]
    {
        let _ = field;
        crate::web::write_text(text).await
    }
    #[cfg(not(target_arch = "wasm32"))]
    if matches!(field, IdentityField::Name | IdentityField::SystematicName) {
        reshiki::clipboard::write_prepared(reshiki::clipboard::prepare_name(text))
            .await
            .map(|_| ())
    } else {
        reshiki::clipboard::write_text(text).await
    }
}

#[derive(Default)]
pub(super) struct ScopeCache {
    key: Option<PropertyKey>,
    target: Option<Vec<u64>>,
    identity: Option<reshiki::naming::Identity>,
    notice: Option<String>,
    #[cfg(test)]
    calculations: usize,
}

struct Names {
    primary: String,
    systematic: Option<String>,
}

fn one_line(value: &str, width: f32, size: f32) -> String {
    let line = value.replace(['\n', '\r'], " ");
    let value = line.as_str();
    if super::super::workspace::text_width(value, size) <= width {
        return value.into();
    }
    let chars = iced::widget::text_input::Value::new(value);
    let mut lo: usize = 0;
    let mut hi = chars.len();
    while lo < hi {
        let n = (lo + hi).div_ceil(2);
        let candidate = chars.until(n).to_string() + "…";
        if super::super::workspace::text_width(&candidate, size) <= width {
            lo = n;
        } else {
            hi = n - 1;
        }
    }
    chars.until(lo).to_string() + "…"
}

impl App {
    pub(super) fn special_molecular_scope(&self) -> bool {
        reshiki::attachments::present(&self.tab.doc)
            || reshiki::models3d::crystal::touches(
                &self.tab.doc,
                &self.property_request_key().atoms,
            )
    }

    fn identity_target(&self) -> Option<Vec<u64>> {
        let key = self.property_request_key();
        let mut cache = self.tab.inspector_ui.identity_scope.borrow_mut();
        if cache.key.as_ref() != Some(&key) {
            let result = if self.special_molecular_scope() {
                Err("Names are unavailable for this drawing.".into())
            } else {
                reshiki::naming::selected_identity(&self.tab.doc, &key.atoms)
            };
            cache.identity = result.as_ref().ok().cloned();
            cache.target = result.is_ok().then(|| key.atoms.clone());
            cache.notice = result.err();
            cache.key = Some(key);
            #[cfg(test)]
            {
                cache.calculations += 1;
            }
        }
        cache.target.clone()
    }

    fn identity_record(&self, atoms: &[u64]) -> Option<&reshiki::naming::Record> {
        let mut identity = self
            .tab
            .inspector_ui
            .identity_scope
            .borrow()
            .identity
            .clone()?;
        identity.write_unknown_stereo = self.appearance.write_unknown_stereo;
        self.named_identity(atoms, &identity)
    }

    fn identity_name_stale(&self) -> bool {
        let Some(atoms) = self.identity_target() else {
            return false;
        };
        reshiki::linked_properties::caption(&self.tab.doc, &atoms)
            .is_some_and(|link| link.renamed_structure_changed())
    }

    fn identity_names(&self) -> Option<Names> {
        let atoms = self.identity_target()?;
        let record = self.identity_record(&atoms);
        let link = reshiki::linked_properties::caption(&self.tab.doc, &atoms);
        let automatic = link
            .filter(|link| !link.needs_name())
            .map(|link| link.name.as_str())
            .filter(|name| !name.is_empty());
        let systematic = record
            .and_then(|record| record.systematic_name.as_deref())
            .or(automatic);
        let placed =
            reshiki::molecule_names::label(&self.tab.doc, &atoms).map(|name| name.text.as_str());
        let custom = link
            .map(|link| link.custom_name.as_str())
            .filter(|name| !name.is_empty());
        let primary = custom
            .or(placed)
            .or_else(|| {
                record.and_then(|record| record.synonyms.first().map(|alias| alias.name.as_str()))
            })
            .or(systematic)?;
        Some(Names {
            primary: primary.into(),
            systematic: systematic
                .filter(|name| *name != primary)
                .map(str::to_owned),
        })
    }

    fn identity_text(&self, field: IdentityField) -> Option<String> {
        if self.special_molecular_scope() {
            return None;
        }
        if field == IdentityField::Name {
            return self.identity_names().map(|names| names.primary);
        }
        if field == IdentityField::SystematicName {
            return self.identity_names()?.systematic;
        }
        let analysis = self.property_analysis()?;
        let text = match field {
            IdentityField::Formula => self.shown_formula(&analysis.display_formula),
            IdentityField::Weight => format!(
                "{} g/mol",
                molecular_weight(analysis.mass, self.appearance.two_decimal_weight)
            ),
            IdentityField::Smiles => analysis.smiles.clone(),
            IdentityField::Inchi => analysis.inchi.clone(),
            IdentityField::InchiKey => analysis.inchikey.clone(),
            IdentityField::Analysis => self
                .elemental_analysis(analysis)?
                .chars()
                .map(|c| match c {
                    '₀'..='₉' => char::from_digit(c as u32 - '₀' as u32, 10).unwrap_or(c),
                    _ => c,
                })
                .collect(),
            IdentityField::Name | IdentityField::SystematicName => return None,
        };
        (!text.is_empty()).then_some(text)
    }

    pub(super) fn copy_identity(&mut self, field: IdentityField) -> Task<Message> {
        self.copy_identity_with(field, move |text| write_identity(field, text))
    }

    pub(super) fn copy_identity_with<Write, Pending>(
        &mut self,
        field: IdentityField,
        write: Write,
    ) -> Task<Message>
    where
        Write: FnOnce(String) -> Pending + iced_runtime::futures::MaybeSend + 'static,
        Pending: std::future::Future<Output = Result<(), String>>
            + iced_runtime::futures::MaybeSend
            + 'static,
    {
        if self.clipboard_working() {
            self.status = "A clipboard operation is already in progress".into();
            return Task::none();
        }
        let Some(value) = self.identity_text(field) else {
            return Task::none();
        };
        let key = IdentityCopyKey {
            epoch: self.tab.file_epoch,
            revision: self.tab.revision,
            atoms: self.property_request_key().atoms,
            field,
        };
        self.copy_as_busy = true;
        self.status = format!("Copying {}…", field.label());
        self.error = false;
        Task::perform(write(value), move |result| {
            Message::InspectorAction(Action::IdentityCopied(key.clone(), result))
        })
    }

    pub(super) fn identity_copied(&mut self, key: IdentityCopyKey, result: Result<(), String>) {
        self.copy_as_busy = false;
        if key.epoch != self.tab.file_epoch {
            return;
        }
        match result {
            Ok(()) => {
                self.status = if key.revision == self.tab.revision
                    && key.atoms == self.property_request_key().atoms
                {
                    format!("{} copied", key.field.label())
                } else {
                    format!("Previous {} copied", key.field.label())
                };
                self.error = false;
            }
            Err(error) => {
                self.status = format!("Could not copy {}: {error}", key.field.label());
                self.error = true;
            }
        }
    }

    pub(super) fn name_identity(&mut self) -> Task<Message> {
        if self.tab.busy || !self.identity_naming_idle() {
            return Task::none();
        }
        let Some(atoms) = self.identity_target() else {
            return Task::none();
        };
        self.generate_structure_name(atoms)
    }

    fn identity_caption_action(&self) -> Option<Message> {
        if self.tab.busy || !self.identity_naming_idle() {
            return None;
        }
        let atoms = self.identity_target()?;
        if self.identity_names().is_none()
            || reshiki::molecule_names::label(&self.tab.doc, &atoms).is_some()
        {
            return None;
        }
        if let Some(link) = reshiki::linked_properties::caption(&self.tab.doc, &atoms)
            && !link.name_line().is_empty()
        {
            return (!reshiki::linked_properties::shown(&self.tab.doc, &atoms, Property::Name))
                .then_some(Message::ToggleProperty(atoms, Property::Name));
        }
        self.identity_record(&atoms)?.systematic_name.as_ref()?;
        Some(Message::InspectorAction(Action::AddIdentityName))
    }

    pub(super) fn add_identity_name(&mut self) -> Task<Message> {
        let Some(action) = self.identity_caption_action() else {
            return Task::none();
        };
        if let Message::ToggleProperty(atoms, property) = action {
            let Some(names) = self.identity_names() else {
                return Task::none();
            };
            if reshiki::linked_properties::caption(&self.tab.doc, &atoms)
                .is_some_and(|saved| names.primary == saved.name_line())
            {
                self.toggle_property(&atoms, property);
            } else {
                self.rename_molecule(&atoms, &names.primary);
            }
        } else if let Some(atoms) = self.identity_target()
            && let Some(record) = self.identity_record(&atoms)
        {
            let Some(systematic) = record.systematic_name.clone() else {
                return Task::none();
            };
            let canonical = record.canonical_smiles.clone();
            let primary = record
                .synonyms
                .first()
                .map_or_else(|| systematic.clone(), |alias| alias.name.clone());
            if self.property_target(&[]).as_ref() == Some(&atoms) {
                self.show_identity_name(&atoms, &primary, &systematic, &canonical);
            } else {
                // The established standalone caption also supports salts;
                // property pins intentionally target connected molecules.
                self.place_molecule_caption(&atoms, primary, canonical);
            }
        }
        Task::none()
    }

    fn identity_copy_value(
        &self,
        field: IdentityField,
        label: bool,
        size: f32,
    ) -> Element<'_, Message> {
        let value = self.identity_text(field);
        let enabled = value.is_some();
        let full = value.unwrap_or_else(|| "Unavailable".into());
        let shown = full.clone();
        // Responsive fills by default. Give the row its own text/glyph height
        // so a shrinking card does not depend on a neighboring property pin.
        let height = text::LineHeight::default()
            .to_absolute(size.into())
            .0
            .max(20.);
        let line = row![
            label.then(|| text(field.label()).size(11).style(muted_text).width(
                if field == IdentityField::Analysis {
                    110
                } else {
                    75
                }
            )),
            responsive(move |bounds| text(one_line(&shown, bounds.width, size))
                .size(size)
                .height(Length::Fill)
                .align_y(Alignment::Center)
                .wrapping(text::Wrapping::None)
                .into())
            .width(Length::Fill),
            Space::new().width(20),
        ]
        .spacing(6)
        .height(height)
        .align_y(Alignment::Center);
        let content = hover(
            line,
            container(canvas(Glyph(Icon::Copy, enabled)).width(20).height(20))
                .align_right(Length::Fill)
                .center_y(Length::Fill),
        );
        hover_hint(
            reshiki::accessibility::button(
                field.id(),
                if field == IdentityField::Name && self.identity_name_stale() {
                    "Copy Name · Structure changed since you named it.".into()
                } else {
                    format!("Copy {}", field.label())
                },
                content,
            )
            .value(full.clone())
            .width(Length::Fill)
            .padding([5, 0])
            .style(move |theme, status| {
                let mut style = button::text(theme, status);
                style.text_color = if field == IdentityField::SystematicName || !enabled {
                    crate::appearance::muted(theme)
                } else {
                    theme.palette().text
                };
                style
            })
            .on_press_maybe(
                (enabled && !self.clipboard_working()).then_some(match field {
                    IdentityField::Smiles => Message::CopySmiles,
                    IdentityField::Analysis => Message::CopyAnalysis,
                    _ => Message::InspectorAction(Action::CopyIdentity(field)),
                }),
            ),
            full,
            tooltip::Position::Top,
        )
        .into()
    }

    fn identity_row(&self, field: IdentityField, pin: Option<Property>) -> Element<'_, Message> {
        let mut line = row![self.identity_copy_value(field, true, 12.)]
            .spacing(4)
            .align_y(Alignment::Center);
        if let Some(property) = pin {
            if let Some(places) = self.property_places(property) {
                line = line.push(places);
            }
            line = line.push(self.property_pin(property));
        }
        line.into()
    }

    pub(super) fn identity_content<'a>(&'a self, analysis: &'a Analysis) -> Element<'a, Message> {
        let target = self.identity_target();
        let mut body = column![].spacing(5);
        if let Some(names) = self.identity_names() {
            body = body.push(self.identity_copy_value(IdentityField::Name, false, 17.));
            if names.systematic.is_some() {
                body =
                    body.push(self.identity_copy_value(IdentityField::SystematicName, false, 11.));
            }
        } else {
            body = body.push(
                reshiki::accessibility::button(
                    "identity-name-it",
                    "Name it",
                    if self.identity_naming_idle() {
                        "Name it"
                    } else {
                        "Naming…"
                    },
                )
                .style(button::text)
                .on_press_maybe(
                    (target.is_some() && !self.tab.busy && self.identity_naming_idle())
                        .then_some(Message::InspectorAction(Action::NameIdentity)),
                ),
            );
        }
        if let Some(atoms) = &target {
            if self.identity_name_stale() {
                body = body.push(
                    text("Structure changed since you named it.")
                        .size(11)
                        .color(self.theme().palette().warning),
                );
            }
            if let Some(error) = self.identity_naming_error(atoms) {
                body = body.push(text(error).size(11).style(text::danger));
            }
            if let Some(record) = self.identity_record(atoms) {
                if let Some(status) = record
                    .name_status
                    .filter(|status| *status != reshiki::naming::NameStatus::Preferred)
                {
                    body = body.push(hover_hint(
                        text(status.label()).size(11).style(muted_text),
                        status.detail(),
                        tooltip::Position::Top,
                    ));
                }
                if let Some(note) = record.omitted_stereo_note() {
                    body = body.push(text(note).size(11).style(muted_text));
                }
                for warning in &record.warnings {
                    body = body.push(text(warning).size(11).style(muted_text));
                }
            }
        } else if let Some(notice) = self
            .tab
            .inspector_ui
            .identity_scope
            .borrow()
            .notice
            .as_ref()
        {
            body = body.push(text(notice.clone()).size(11).style(muted_text));
        }
        for (field, pin) in [
            (IdentityField::Formula, Some(Property::Formula)),
            (IdentityField::Weight, Some(Property::Weight)),
            (IdentityField::Smiles, None),
            (IdentityField::Inchi, None),
            (IdentityField::InchiKey, None),
        ] {
            body = body.push(self.identity_row(field, pin));
        }
        if analysis.smiles.is_empty() {
            body = body.push(
                text("SMILES cannot represent these hydrogen or partial bonds.")
                    .size(11)
                    .style(muted_text),
            );
        }
        if let Some(notice) = analysis.smiles_notice.as_deref() {
            body = body.push(text(notice).size(11).style(muted_text));
        }
        if self.elemental_analysis(analysis).is_some() {
            body = body.push(self.identity_row(IdentityField::Analysis, Some(Property::Analysis)));
        }
        body.push(
            reshiki::accessibility::button("identity-add-name", "Add name below", "Add name below")
                .padding([7, 9])
                .width(Length::Fill)
                .on_press_maybe(
                    self.identity_caption_action()
                        .map(|_| Message::InspectorAction(Action::AddIdentityName)),
                ),
        )
        .push(self.identity_name_options())
        .into()
    }

    pub(super) fn identity_name_options(&self) -> Option<Element<'_, Message>> {
        let atoms = self.property_target(&[])?;
        let automatic = self.identity_target().as_ref() == Some(&atoms);
        let name_details =
            automatic && self.identity_names().is_some() && self.identity_record(&atoms).is_none();
        Some(
            self.inspector_section_lazy(Section::NameOptions, "Name options", "", false, || {
                column![
                    (!automatic).then(|| text("Custom names apply to the whole molecule.")
                        .size(11)
                        .style(muted_text)),
                    self.molecule_name(atoms, automatic),
                    name_details.then(|| reshiki::accessibility::button(
                        "identity-name-details",
                        "Name it",
                        "Name it"
                    )
                    .style(button::text)
                    .on_press_maybe(
                        (!self.tab.busy && self.identity_naming_idle())
                            .then_some(Message::InspectorAction(Action::NameIdentity))
                    )),
                ]
                .spacing(5)
            }),
        )
    }

    pub(super) fn descriptor_properties(&self) -> Element<'_, Message> {
        let Some(analysis) = self.property_analysis() else {
            return column![].into();
        };
        let key = self.property_key();
        let ids = key
            .as_ref()
            .map(|key| key.atoms.iter().copied().collect::<HashSet<_>>());
        let unpaired = unpaired_electrons(&self.tab.doc, analysis, ids.as_ref());
        let pins = [
            Some(Property::ExactMass),
            Some(Property::LogP),
            Some(Property::Tpsa),
        ];
        let mut body = column![].spacing(7);
        for ((label, hint, value), pin) in
            property_rows(analysis, self.appearance.two_decimal_weight, unpaired)
                .into_iter()
                .skip(1)
                .zip(pins.into_iter().chain(std::iter::repeat(None)))
        {
            let label = text(label).size(11).style(muted_text).width(Length::Fill);
            let label: Element<'_, Message> = match hint {
                Some(hint) => hover_hint(label, hint, tooltip::Position::Top).into(),
                None => label.into(),
            };
            let mut line = row![label, text(value).size(12)]
                .spacing(6)
                .align_y(Alignment::Center);
            if let Some(property) = pin {
                if let Some(places) = self.property_places(property) {
                    line = line.push(places);
                }
                line = line.push(self.property_pin(property));
            }
            body = body.push(line);
        }
        let classes = self.property_classes();
        if !classes.is_empty() {
            body = body.push(
                column![
                    text("Classes").size(11).style(muted_text),
                    row(classes.into_iter().map(|class| hover_hint(
                        container(text(class.term).size(11))
                            .padding([3, 7])
                            .style(container::rounded_box),
                        class.tooltip,
                        tooltip::Position::Top
                    )
                    .into()))
                    .spacing(4)
                    .wrap()
                ]
                .spacing(4),
            );
        }
        body.push(self.molecular_actions()).into()
    }
}

#[cfg(test)]
mod tests;
