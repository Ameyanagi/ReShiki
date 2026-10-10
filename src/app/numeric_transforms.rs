//! Explicit, atomic numeric edits using the same geometry as selection handles.
use super::icons::{Glyph, Icon};
use super::workspace::{accessible_unit_field, control, hover_hint, text_width};
use super::{App, Message};
use iced::widget::{Space, canvas, column, container, row, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::{
    document::{Document, Point},
    editing::{self, Transform},
};

mod focus;
mod reference;

#[cfg(test)]
mod memory_tests;

/// Size of the proportional lock beside H.
const LOCK: f32 = 20.;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Rotation,
    TiltX,
    TiltY,
    Width,
    Height,
    Scale,
}

impl Field {
    fn id(self) -> &'static str {
        match self {
            Self::Rotation => "transform-rotation",
            Self::TiltX => "transform-tilt-x",
            Self::TiltY => "transform-tilt-y",
            Self::Width => "transform-width",
            Self::Height => "transform-height",
            Self::Scale => "transform-scale",
        }
    }

    /// Apply applies edited fields in this order, so sizes are final.
    const ALL: [Self; 6] = [
        Self::Rotation,
        Self::TiltX,
        Self::TiltY,
        Self::Scale,
        Self::Width,
        Self::Height,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Rotation => "Rotate",
            Self::TiltX => "Tilt X",
            Self::TiltY => "Tilt Y",
            Self::Width => "W",
            Self::Height => "H",
            Self::Scale => "Scale",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Rotation => "Rotation",
            Self::Width => "Width",
            Self::Height => "Height",
            other => other.label(),
        }
    }

    fn unit(self) -> &'static str {
        match self {
            Self::Rotation | Self::TiltX | Self::TiltY => "°",
            Self::Width | Self::Height => "pt",
            Self::Scale => "%",
        }
    }

    fn hint(self) -> &'static str {
        match self {
            Self::Rotation => "Relative · positive is clockwise",
            Self::TiltX | Self::TiltY => "Relative · tilts atoms and shapes, up to ±85° per change",
            Self::Width | Self::Height => "Includes labels; fonts and line widths stay fixed",
            Self::Scale => "Relative · scales coordinates uniformly",
        }
    }
}

#[derive(Debug, Clone)]
pub enum Action {
    Reference(reference::Action),
    Input(Field, String),
    /// Enter in one field applies only that field.
    Apply(Field),
    /// The Apply button applies every edited field as one Undo step.
    ApplyAll,
    Proportional(bool),
    More(bool),
}

#[derive(Default, PartialEq)]
struct Key {
    revision: u64,
    epoch: u64,
    ids: Vec<u64>,
}

pub(super) struct State {
    reference: reference::State,
    key: Option<Key>,
    rotation: String,
    tilt_x: String,
    tilt_y: String,
    width: String,
    height: String,
    dimensions: (String, String),
    scale: String,
    proportional: bool,
    /// W or H, whichever was typed last; it wins when both are locked.
    last_size: Field,
    more: bool,
    error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            reference: Default::default(),
            key: None,
            rotation: "0".into(),
            tilt_x: "0".into(),
            tilt_y: "0".into(),
            width: String::new(),
            height: String::new(),
            dimensions: Default::default(),
            scale: "100".into(),
            proportional: true,
            last_size: Field::Width,
            more: false,
            error: None,
        }
    }
}

impl State {
    fn value(&self, field: Field) -> &str {
        match field {
            Field::Rotation => &self.rotation,
            Field::TiltX => &self.tilt_x,
            Field::TiltY => &self.tilt_y,
            Field::Width => &self.width,
            Field::Height => &self.height,
            Field::Scale => &self.scale,
        }
    }

    fn value_mut(&mut self, field: Field) -> &mut String {
        match field {
            Field::Rotation => &mut self.rotation,
            Field::TiltX => &mut self.tilt_x,
            Field::TiltY => &mut self.tilt_y,
            Field::Width => &mut self.width,
            Field::Height => &mut self.height,
            Field::Scale => &mut self.scale,
        }
    }

    /// Fields that differ from their reset value or readout, including tilt
    /// hidden under a closed More.
    fn edited(&self) -> Vec<Field> {
        Field::ALL
            .into_iter()
            .filter(|&field| {
                let value = self.value(field).trim();
                match field {
                    Field::Rotation | Field::TiltX | Field::TiltY => value != "0",
                    Field::Scale => value != "100",
                    Field::Width => value != self.dimensions.0,
                    Field::Height => value != self.dimensions.1,
                }
            })
            .collect()
    }

    /// What Apply applies: every edited field, except that locked proportions
    /// keep only the size typed last, since W and H cannot both be met.
    fn to_apply(&self) -> Vec<Field> {
        let mut fields = self.edited();
        let both = [Field::Width, Field::Height]
            .iter()
            .all(|field| fields.contains(field));
        if self.proportional && both {
            let other = if self.last_size == Field::Width {
                Field::Height
            } else {
                Field::Width
            };
            fields.retain(|&field| field != other);
        }
        fields
    }
}

fn extent(doc: &Document, ids: &[u64], field: Field) -> Option<f32> {
    let (lo, hi) = reshiki::scene::selection_bounds(doc, ids)?;
    let world = if field == Field::Width {
        hi.x - lo.x
    } else {
        hi.y - lo.y
    };
    Some(world * doc.drawing_style.points_per_world())
}

fn scale(doc: &mut Document, ids: &[u64], pivot: Point, field: Field, factor: f32, lock: bool) {
    if lock || field == Field::Scale {
        editing::transform_about(doc, ids, pivot, factor, 0.);
    } else {
        let (x, y) = if field == Field::Width {
            (factor, 1.)
        } else {
            (1., factor)
        };
        editing::scale_axes_about(doc, ids, pivot, x, y);
    }
}

/// Fonts and strokes do not scale, so target/current is not generally the
/// required coordinate scale. Solve against the same bounds as the canvas box.
fn size_factor(
    doc: &Document,
    ids: &[u64],
    pivot: Point,
    field: Field,
    target: f32,
    lock: bool,
) -> Result<f32, String> {
    let measure = |factor| {
        let mut trial = doc.clone();
        scale(&mut trial, ids, pivot, field, factor, lock);
        extent(&trial, ids, field).unwrap_or(0.)
    };
    let current = extent(doc, ids, field).ok_or("Select objects to resize")?;
    // Applying an unchanged displayed value must not round the drawing or
    // create a history entry. The fields display two decimal places in pt.
    if (target - current).abs() <= 0.0051 {
        return Ok(1.);
    }
    let (mut low, mut high) = if target < current {
        (0.0001, 1.)
    } else {
        (1., 10_000.)
    };
    let mut lo_size = measure(low);
    let mut hi_size = measure(high);
    // Inward-facing fixed labels can cross as their anchors move: the width
    // first shrinks and then grows. Search that minimum only when the usual
    // scale bracket cannot reach a smaller requested extent.
    if target < current && lo_size > target && (hi_size - lo_size).abs() > 0.0001 {
        let (mut left, mut right) = (0.0001, 10_000.);
        for _ in 0..44 {
            let a = left + (right - left) / 3.;
            let b = right - (right - left) / 3.;
            if measure(a) <= measure(b) {
                right = b;
            } else {
                left = a;
            }
        }
        low = (left + right) / 2.;
        high = if low < 1. { 1. } else { 10_000. };
        lo_size = measure(low);
        hi_size = measure(high);
    }
    if !lo_size.is_finite()
        || !hi_size.is_finite()
        || target < lo_size
        || target > hi_size
        || hi_size - lo_size < 0.0001
    {
        return Err("This size cannot be reached while keeping text and line widths fixed".into());
    }
    for iteration in 0..40 {
        // Extents are usually affine in coordinate scale. Interpolation gets
        // ordinary selections to the target in one or two measurements; a
        // periodic bisection also makes progress when the outer label changes.
        let fraction = (target - lo_size) / (hi_size - lo_size);
        let middle = if iteration % 4 == 3 || !(0.0..1.0).contains(&fraction) {
            (low + high) / 2.
        } else {
            low + (high - low) * fraction
        };
        let measured = measure(middle);
        if (measured - target).abs() < 0.0001 {
            return Ok(middle);
        }
        if measured < target {
            low = middle;
            lo_size = measured;
        } else {
            high = middle;
            hi_size = measured;
        }
    }
    let factor = (low + high) / 2.;
    if (measure(factor) - target).abs() > 0.005 {
        return Err("This size cannot be reached accurately; choose another value".into());
    }
    Ok(factor)
}

fn transform_candidate(
    document: &mut Document,
    selected: &[u64],
    field: Field,
    input: &str,
    lock: bool,
) -> Result<(), String> {
    let ids = document.expand_abbreviation_selection(selected);
    if reshiki::scene::selection_bounds(document, &ids).is_none() {
        return Err("Select objects to transform".into());
    }
    let value = input
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or("Enter a finite number")?;
    match field {
        Field::Rotation => {
            if value.abs() > 36_000. {
                return Err("Enter a rotation between −36000° and 36000°".into());
            }
            let degrees = value % 360.;
            if degrees != 0. {
                editing::transform(document, &ids, Transform::Rotate(degrees));
            }
        }
        Field::TiltX | Field::TiltY => {
            if value.abs() > 85. {
                return Err("Enter a tilt between −85° and 85° per change".into());
            }
            if !crate::canvas::tilt::available(document, &ids) {
                return Err("Select at least two atoms or a shape to tilt".into());
            }
            editing::transform(
                document,
                &ids,
                if field == Field::TiltX {
                    Transform::TiltX(value)
                } else {
                    Transform::TiltY(value)
                },
            );
        }
        Field::Width | Field::Height | Field::Scale => {
            if value <= 0. {
                return Err("Enter a size or percentage greater than zero".into());
            }
            let (lo, hi) = reshiki::scene::selection_bounds(document, &ids)
                .ok_or("Select objects to resize")?;
            let pivot = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
            let factor = if field == Field::Scale {
                value / 100.
            } else {
                size_factor(document, &ids, pivot, field, value, lock)?
            };
            if !(0.0001..=10_000.).contains(&factor) {
                return Err("Use a scale from 0.01% to 1000000% per change".into());
            }
            scale(document, &ids, pivot, field, factor, lock);
        }
    }
    document.validate()
}

impl App {
    pub(super) fn begin_numeric_transform(
        &mut self,
        target: crate::canvas::TransformField,
    ) -> Task<Message> {
        if self.tab.selected.is_empty() {
            return Task::none();
        }
        let field = match target {
            crate::canvas::TransformField::Rotation => Field::Rotation,
            crate::canvas::TransformField::Scale => Field::Scale,
            crate::canvas::TransformField::Width => Field::Width,
            crate::canvas::TransformField::Height => Field::Height,
        };
        let old_width = self.inspector_width();
        self.inspector_open = true;
        self.inspector_tab = super::InspectorTab::Properties;
        self.tab
            .inspector_ui
            .update(super::inspector::Action::Section(
                super::inspector::Section::Transform,
                true,
            ));
        // Revealing the panel must not move the handle the user just clicked.
        let width_change = old_width - self.inspector_width();
        self.tab.camera.center.x += width_change / (2. * self.tab.camera.zoom);
        self.viewport.width += width_change;
        self.sync_numeric_transforms();
        iced::advanced::widget::operate(focus::FieldOperation::new(field))
    }

    pub(super) fn sync_numeric_transforms(&mut self) {
        if self
            .tab
            .numeric_transforms
            .reference
            .stale_stretch(self.tab.file_epoch)
        {
            self.cancel_reference_stretch();
        }
        let key = Key {
            revision: self.tab.revision,
            epoch: self.tab.file_epoch,
            ids: self.tab.selected.clone(),
        };
        if self.tab.numeric_transforms.key.as_ref() == Some(&key) {
            return;
        }
        let State {
            proportional,
            last_size,
            more,
            ref mut reference,
            ..
        } = self.tab.numeric_transforms;
        // A pinned point is tab-local and survives selection changes and edits.
        // Opening another file must not inherit an unrelated construction center.
        let reference = reference.take_for_epoch(self.tab.file_epoch);
        self.tab.numeric_transforms = State {
            key: Some(key),
            proportional,
            last_size,
            more,
            reference,
            ..State::default()
        };
        self.refresh_numeric_dimensions();
    }

    // Chemistry checks can refresh hydrogen/CIP labels without a revision or
    // Undo entry. Refresh untouched readouts without discarding typed values.
    pub(super) fn refresh_numeric_dimensions(&mut self) {
        let (width, height) = reshiki::scene::selection_bounds(&self.tab.doc, &self.tab.selected)
            .map(|(lo, hi)| {
                let scale = self.tab.doc.drawing_style.points_per_world();
                (
                    format!("{:.2}", (hi.x - lo.x) * scale),
                    format!("{:.2}", (hi.y - lo.y) * scale),
                )
            })
            .unwrap_or_default();
        if self.tab.numeric_transforms.width == self.tab.numeric_transforms.dimensions.0 {
            self.tab.numeric_transforms.width = width.clone();
        }
        if self.tab.numeric_transforms.height == self.tab.numeric_transforms.dimensions.1 {
            self.tab.numeric_transforms.height = height.clone();
        }
        self.tab.numeric_transforms.dimensions = (width, height);
    }

    // Validate against the caption that would be committed, without ending its
    // draft or consuming history for invalid and unchanged transforms.
    fn numeric_transform_candidate(&self, fields: &[Field]) -> Result<Option<Document>, String> {
        let source = self.inline_candidate()?;
        let selected = if self.tab.inline_text.is_some() {
            let id = self
                .inline_label_id()
                .unwrap_or_else(|| self.tab.doc.next_id());
            source
                .annotations
                .iter()
                .filter(|a| a.id == id)
                .map(|a| a.id)
                .collect()
        } else {
            self.tab.selected.clone()
        };
        let mut document = source.clone();
        for &field in fields {
            transform_candidate(
                &mut document,
                &selected,
                field,
                self.tab.numeric_transforms.value(field),
                self.tab.numeric_transforms.proportional,
            )
            .map_err(|error| {
                if fields.len() > 1 {
                    format!("{}: {error}", field.name())
                } else {
                    error
                }
            })?;
        }
        Ok((document != source).then_some(document))
    }

    pub(super) fn numeric_transform_action(&mut self, action: Action) -> Task<Message> {
        match action {
            Action::Reference(action) => self.reference_transform_action(action),
            Action::Input(field, input) => {
                *self.tab.numeric_transforms.value_mut(field) = input;
                self.tab.numeric_transforms.error = None;
                if matches!(field, Field::Width | Field::Height) {
                    self.tab.numeric_transforms.last_size = field;
                }
            }
            Action::Proportional(lock) => self.tab.numeric_transforms.proportional = lock,
            Action::More(open) => self.tab.numeric_transforms.more = open,
            Action::Apply(field) => self.apply_numeric_transforms(&[field], true),
            Action::ApplyAll => {
                self.apply_numeric_transforms(&self.tab.numeric_transforms.to_apply(), false)
            }
        }
        Task::none()
    }

    /// Applies `fields`; with `keep`, values typed in other fields stay for
    /// their own Enter or Apply.
    fn apply_numeric_transforms(&mut self, fields: &[Field], keep: bool) {
        match self.numeric_transform_candidate(fields) {
            Ok(document) => {
                if document.is_some() && !self.finish_inline(true) {
                    return;
                }
                let state = &self.tab.numeric_transforms;
                let pending: Vec<_> = state
                    .edited()
                    .into_iter()
                    .filter(|field| keep && !fields.contains(field))
                    .map(|field| (field, state.value(field).to_owned()))
                    .collect();
                self.tab.numeric_transforms.error = None;
                self.tab.numeric_transforms.key = None;
                self.error = false;
                self.status = if document.is_none() {
                    "No transform needed".into()
                } else {
                    "Drawing updated".into()
                };
                if let Some(document) = document {
                    let before = std::mem::replace(&mut self.tab.doc, document);
                    self.changed(before);
                }
                self.sync_numeric_transforms();
                for (field, value) in pending {
                    *self.tab.numeric_transforms.value_mut(field) = value;
                }
            }
            Err(error) => {
                self.status = error.clone();
                self.error = true;
                self.tab.numeric_transforms.error = Some(error);
            }
        }
    }

    /// Rotate | Scale and W 🔒 H, with tilt under More and one Apply.
    pub(super) fn numeric_transform_panel(&self) -> Element<'_, Message> {
        let state = &self.tab.numeric_transforms;
        let selected = !self.tab.selected.is_empty();
        let tilt = selected && crate::canvas::tilt::available(&self.tab.doc, &self.tab.selected);
        let cell = |label: Element<'static, Message>, field: Field, label_width: f32| {
            let enabled = if matches!(field, Field::TiltX | Field::TiltY) {
                tilt
            } else {
                selected
            };
            let input = reshiki::accessibility::text_input(
                field.id(),
                format!("{} ({})", field.name(), field.unit()),
                "",
                state.value(field),
            )
            .style(crate::appearance::input_style)
            .on_input_maybe(
                enabled
                    .then_some(move |value| Message::NumericTransform(Action::Input(field, value))),
            )
            .on_submit_maybe(enabled.then_some(Message::NumericTransform(Action::Apply(field))));
            row![
                container(label).width(label_width),
                hover_hint(
                    accessible_unit_field(input, field.unit()),
                    match field {
                        _ if enabled => field.hint(),
                        Field::TiltX | Field::TiltY =>
                            "Select at least two atoms or a shape to tilt",
                        _ => "Select objects to transform",
                    },
                    tooltip::Position::Top
                ),
            ]
            .spacing(4)
            .align_y(Alignment::Center)
            .width(Length::Fill)
        };
        let label = |field: Field| text(field.label()).size(12);
        let lock = hover_hint(
            reshiki::accessibility::button(
                "transform-proportions",
                "Lock width and height proportions",
                canvas(Glyph(Icon::Lock(state.proportional), true))
                    .width(LOCK)
                    .height(LOCK),
            )
            .padding(0)
            .checked(state.proportional)
            .style(control(state.proportional))
            .on_press(Message::NumericTransform(Action::Proportional(
                !state.proportional,
            ))),
            if state.proportional {
                "Proportions locked · W and H scale together\nIf both are edited, Apply uses the one edited last"
            } else {
                "Proportions unlocked · W and H change one axis"
            },
            tooltip::Position::Top,
        );
        // Label columns take the widest label; the lock sits in the second
        // column beside H, between the W and H fields.
        let widest = |labels: &[f32]| labels.iter().copied().fold(0., f32::max) + 2.;
        let width = |field: Field| text_width(field.label(), 12.);
        let left = widest(&[Field::Rotation, Field::Width, Field::TiltX].map(width));
        let right = widest(&[
            width(Field::Scale),
            width(Field::TiltY),
            LOCK + 3. + width(Field::Height),
        ]);
        let pair = |a: Field, b_label: Element<'static, Message>, b: Field| {
            row![cell(label(a).into(), a, left), cell(b_label, b, right)]
                .spacing(8)
                .align_y(Alignment::Center)
        };
        let mut body = column![
            pair(Field::Rotation, label(Field::Scale).into(), Field::Scale),
            pair(
                Field::Width,
                row![lock, label(Field::Height)]
                    .spacing(3)
                    .align_y(Alignment::Center)
                    .into(),
                Field::Height
            ),
        ]
        .spacing(8);
        if state.more {
            body = body.push(pair(Field::TiltX, label(Field::TiltY).into(), Field::TiltY));
        }
        let pending = state.to_apply();
        let edited = selected && !pending.is_empty();
        let hidden_tilt = !state.more
            && pending
                .iter()
                .any(|f| matches!(f, Field::TiltX | Field::TiltY));
        body = body.push(
            row![
                reshiki::accessibility::button(
                    "transform-more",
                    "More transform controls: tilt X and Y",
                    text(match (state.more, hidden_tilt) {
                        (true, _) => "▾ More: tilt X / Y",
                        (false, false) => "▸ More: tilt X / Y",
                        (false, true) => "▸ More: tilt X / Y · edited",
                    })
                    .size(12)
                )
                .padding([7, 9])
                .style(control(false))
                .expanded(state.more)
                .on_press(Message::NumericTransform(Action::More(!state.more))),
                Space::new().width(Length::Fill),
                hover_hint(
                    reshiki::accessibility::button(
                        "transform-apply",
                        "Apply numeric transforms",
                        text("Apply").size(12)
                    )
                    .padding([6, 14])
                    .style(crate::appearance::primary)
                    .on_press_maybe(edited.then_some(Message::NumericTransform(Action::ApplyAll))),
                    if edited {
                        let names: Vec<_> = pending.iter().map(|f| f.name()).collect();
                        format!(
                            "Apply {} as one Undo step · Enter applies only its field",
                            names.join(", ")
                        )
                    } else {
                        "Edit a value first · Enter applies only its field".to_owned()
                    },
                    tooltip::Position::Top,
                ),
            ]
            .align_y(Alignment::Center),
        );
        if let Some(error) = &state.error {
            body = body.push(text(error).size(12).style(text::danger));
        }
        body = body.push(self.reference_transform_panel());
        body.into()
    }
}

#[cfg(test)]
mod tests;
