//! Explicit, atomic numeric edits using the same geometry as selection handles.
use super::icons::{Glyph, Icon};
use super::workspace::{command, control, hover_hint, text_width, unit_field};
use super::{App, Message};
use iced::widget::{Space, button, canvas, column, container, row, text, tooltip};
use iced::{Alignment, Element, Length, Task};
use reshiki::{
    document::{Document, Point},
    editing::{self, Transform},
};

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
    key: Option<Key>,
    rotation: String,
    tilt_x: String,
    tilt_y: String,
    width: String,
    height: String,
    dimensions: (String, String),
    scale: String,
    proportional: bool,
    more: bool,
    error: Option<String>,
}

impl Default for State {
    fn default() -> Self {
        Self {
            key: None,
            rotation: "0".into(),
            tilt_x: "0".into(),
            tilt_y: "0".into(),
            width: String::new(),
            height: String::new(),
            dimensions: Default::default(),
            scale: "100".into(),
            proportional: true,
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

    /// Visible fields that differ from their reset value or readout.
    fn edited(&self) -> Vec<Field> {
        Field::ALL
            .into_iter()
            .filter(|&field| {
                let value = self.value(field).trim();
                match field {
                    Field::Rotation => value != "0",
                    Field::TiltX | Field::TiltY => self.more && value != "0",
                    Field::Scale => value != "100",
                    Field::Width => value != self.dimensions.0,
                    Field::Height => value != self.dimensions.1,
                }
            })
            .collect()
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
        reshiki::projection::sync_centroids(doc);
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

fn transformed(
    source: &Document,
    selected: &[u64],
    field: Field,
    input: &str,
    lock: bool,
) -> Result<Document, String> {
    let ids = source.expand_abbreviation_selection(selected);
    if reshiki::scene::selection_bounds(source, &ids).is_none() {
        return Err("Select objects to transform".into());
    }
    let value = input
        .trim()
        .parse::<f32>()
        .ok()
        .filter(|value| value.is_finite())
        .ok_or("Enter a finite number")?;
    let mut result = source.clone();
    match field {
        Field::Rotation => {
            if value.abs() > 36_000. {
                return Err("Enter a rotation between −36000° and 36000°".into());
            }
            let degrees = value % 360.;
            if degrees != 0. {
                editing::transform(&mut result, &ids, Transform::Rotate(degrees));
            }
        }
        Field::TiltX | Field::TiltY => {
            if value.abs() > 85. {
                return Err("Enter a tilt between −85° and 85° per change".into());
            }
            if !crate::canvas::tilt::available(source, &ids) {
                return Err("Select at least two atoms or a shape to tilt".into());
            }
            editing::transform(
                &mut result,
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
            let (lo, hi) =
                reshiki::scene::selection_bounds(source, &ids).ok_or("Select objects to resize")?;
            let pivot = Point::new((lo.x + hi.x) / 2., (lo.y + hi.y) / 2.);
            let factor = if field == Field::Scale {
                value / 100.
            } else {
                size_factor(source, &ids, pivot, field, value, lock)?
            };
            if !(0.0001..=10_000.).contains(&factor) {
                return Err("Use a scale from 0.01% to 1000000% per change".into());
            }
            scale(&mut result, &ids, pivot, field, factor, lock);
        }
    }
    result.validate()?;
    Ok(result)
}

impl App {
    pub(super) fn sync_numeric_transforms(&mut self) {
        let key = Key {
            revision: self.revision,
            epoch: self.file_epoch,
            ids: self.selected.clone(),
        };
        if self.numeric_transforms.key.as_ref() == Some(&key) {
            return;
        }
        let State {
            proportional, more, ..
        } = self.numeric_transforms;
        self.numeric_transforms = State {
            key: Some(key),
            proportional,
            more,
            ..State::default()
        };
        self.refresh_numeric_dimensions();
    }

    // Chemistry checks can refresh hydrogen/CIP labels without a revision or
    // Undo entry. Refresh untouched readouts without discarding typed values.
    pub(super) fn refresh_numeric_dimensions(&mut self) {
        let width = extent(&self.doc, &self.selected, Field::Width)
            .map(|v| format!("{v:.2}"))
            .unwrap_or_default();
        let height = extent(&self.doc, &self.selected, Field::Height)
            .map(|v| format!("{v:.2}"))
            .unwrap_or_default();
        if self.numeric_transforms.width == self.numeric_transforms.dimensions.0 {
            self.numeric_transforms.width = width.clone();
        }
        if self.numeric_transforms.height == self.numeric_transforms.dimensions.1 {
            self.numeric_transforms.height = height.clone();
        }
        self.numeric_transforms.dimensions = (width, height);
    }

    // Validate against the caption that would be committed, without ending its
    // draft or consuming history for invalid and unchanged transforms.
    fn numeric_transform_candidate(&self, fields: &[Field]) -> Result<Option<Document>, String> {
        let source = self.inline_candidate()?;
        let selected = if self.inline_text.is_some() {
            let id = self.inline_label_id().unwrap_or_else(|| self.doc.next_id());
            source
                .annotations
                .iter()
                .filter(|a| a.id == id)
                .map(|a| a.id)
                .collect()
        } else {
            self.selected.clone()
        };
        let mut document = source.clone();
        for &field in fields {
            document = transformed(
                &document,
                &selected,
                field,
                self.numeric_transforms.value(field),
                self.numeric_transforms.proportional,
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
            Action::Input(field, input) => {
                *self.numeric_transforms.value_mut(field) = input;
                self.numeric_transforms.error = None;
            }
            Action::Proportional(lock) => self.numeric_transforms.proportional = lock,
            Action::More(open) => self.numeric_transforms.more = open,
            Action::Apply(field) => self.apply_numeric_transforms(&[field]),
            Action::ApplyAll => self.apply_numeric_transforms(&self.numeric_transforms.edited()),
        }
        Task::none()
    }

    fn apply_numeric_transforms(&mut self, fields: &[Field]) {
        match self.numeric_transform_candidate(fields) {
            Ok(document) => {
                if document.is_some() && !self.finish_inline(true) {
                    return;
                }
                self.numeric_transforms.error = None;
                self.numeric_transforms.key = None;
                self.error = false;
                self.status = if document.is_none() {
                    "No transform needed".into()
                } else {
                    "Drawing updated".into()
                };
                if let Some(document) = document {
                    let before = std::mem::replace(&mut self.doc, document);
                    self.changed(before);
                }
            }
            Err(error) => {
                self.status = error.clone();
                self.error = true;
                self.numeric_transforms.error = Some(error);
            }
        }
    }

    /// Rotate | Scale and W 🔒 H, with tilt under More and one Apply.
    pub(super) fn numeric_transform_panel(&self) -> Element<'_, Message> {
        let state = &self.numeric_transforms;
        let selected = !self.selected.is_empty();
        let tilt = selected && crate::canvas::tilt::available(&self.doc, &self.selected);
        let cell = |label: Element<'static, Message>, field: Field, label_width: f32| {
            let enabled = if matches!(field, Field::TiltX | Field::TiltY) {
                tilt
            } else {
                selected
            };
            let input =
                crate::appearance::text_input("", state.value(field))
                    .on_input_maybe(enabled.then_some(move |value| {
                        Message::NumericTransform(Action::Input(field, value))
                    }))
                    .on_submit_maybe(
                        enabled.then_some(Message::NumericTransform(Action::Apply(field))),
                    );
            row![
                container(label).width(label_width),
                hover_hint(
                    unit_field(input, field.unit()),
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
            button(
                canvas(Glyph(Icon::Lock(state.proportional), true))
                    .width(LOCK)
                    .height(LOCK),
            )
            .padding(0)
            .style(control(state.proportional))
            .on_press(Message::NumericTransform(Action::Proportional(
                !state.proportional,
            ))),
            if state.proportional {
                "Proportions locked · W and H scale together"
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
        let edited = selected && !state.edited().is_empty();
        body = body.push(
            row![
                command(
                    if state.more {
                        "▾ More: tilt X / Y"
                    } else {
                        "▸ More: tilt X / Y"
                    },
                    Message::NumericTransform(Action::More(!state.more)),
                ),
                Space::new().width(Length::Fill),
                hover_hint(
                    button(text("Apply").size(12))
                        .padding([6, 14])
                        .style(crate::appearance::primary)
                        .on_press_maybe(
                            edited.then_some(Message::NumericTransform(Action::ApplyAll))
                        ),
                    if edited {
                        "Apply every edited field as one Undo step · Enter applies only its field"
                    } else {
                        "Edit a value first · Enter applies only its field"
                    },
                    tooltip::Position::Top,
                ),
            ]
            .align_y(Alignment::Center),
        );
        if let Some(error) = &state.error {
            body = body.push(text(error).size(12).style(text::danger));
        }
        body.into()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use reshiki::{
        document::{Annotation, Arrow},
        engine::{ChemistryEngine, LocalEngine, Request},
        graphics::{Graphic, GraphicKind},
    };

    fn fixture() -> App {
        let (mut app, _) = App::new();
        app.busy = false;
        app.doc = reshiki::rings::Preset::Regular.document(42., false);
        app.doc.atoms[0].element = "N".into();
        app.doc.annotations.push(Annotation {
            id: app.doc.next_id(),
            position: Point::new(95., 60.),
            text: "Fixed text".into(),
            format: Default::default(),
        });
        app.doc.arrows.push(Arrow::new(
            app.doc.next_id(),
            Point::new(90., 0.),
            Point::new(180., 0.),
            Default::default(),
            Default::default(),
        ));
        app.doc.graphics.push(Graphic::dragged(
            app.doc.next_id(),
            GraphicKind::Rectangle,
            Point::new(210., -30.),
            Point::new(260., 45.),
            Default::default(),
            Default::default(),
            false,
        ));
        app.selected = app.doc.all_ids();
        app.doc.group_selection(&app.selected).unwrap();
        app.doc.add_atom("O", Point::new(400., 150.));
        app.sync_numeric_transforms();
        app
    }

    fn apply(app: &mut App, field: Field, input: impl ToString) {
        let _ = app.update(Message::NumericTransform(Action::Input(
            field,
            input.to_string(),
        )));
        let _ = app.update(Message::NumericTransform(Action::Apply(field)));
    }

    #[test]
    fn numeric_edits_are_atomic_and_preserve_mixed_group_styles_and_unselected_objects() {
        for (field, input) in [
            (Field::Rotation, "72"),
            (Field::TiltX, "25.5"),
            (Field::TiltY, "-33.25"),
            (Field::Width, "160"),
            (Field::Height, "90"),
            (Field::Scale, "125"),
        ] {
            let mut app = fixture();
            let original = app.doc.clone();
            let ids = app.selected.clone();
            apply(&mut app, field, input);
            assert!(!app.error, "{field:?}: {}", app.status);
            let changed = app.doc.clone();
            assert_ne!(changed, original);
            assert_eq!(changed.atoms.last(), original.atoms.last());
            assert_eq!(changed.bonds, original.bonds);
            assert_eq!(changed.groups, original.groups);
            assert_eq!(
                changed.annotations[0].format,
                original.annotations[0].format
            );
            assert_eq!(changed.arrows[0].style, original.arrows[0].style);
            assert_eq!(changed.graphics[0].style, original.graphics[0].style);
            assert_eq!(changed.drawing_style, original.drawing_style);
            assert_eq!(app.selected, ids);
            let reopened: Document =
                serde_json::from_slice(&serde_json::to_vec(&changed).unwrap()).unwrap();
            assert_eq!(reopened, changed);
            let _ = app.update(Message::Undo);
            assert_eq!(app.doc, original);
            assert!(!app.history.can_undo(), "Each Apply is exactly one edit");
            let _ = app.update(Message::Redo);
            assert_eq!(app.doc, changed);
        }
    }

    #[test]
    fn apply_button_applies_every_edited_field_as_one_undo_step() {
        let mut app = fixture();
        let original = app.doc.clone();
        let _ = app.update(Message::NumericTransform(Action::ApplyAll));
        assert!(!app.error);
        assert_eq!(app.doc, original, "Nothing edited is a no-op");
        assert!(!app.history.can_undo());
        let mut sequential = fixture();
        apply(&mut sequential, Field::Rotation, "72");
        apply(&mut sequential, Field::Scale, "125");
        for (field, input) in [(Field::Scale, "125"), (Field::Rotation, "72")] {
            let _ = app.update(Message::NumericTransform(Action::Input(
                field,
                input.into(),
            )));
        }
        let _ = app.update(Message::NumericTransform(Action::ApplyAll));
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.doc, sequential.doc, "Rotation applies before scale");
        let changed = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo(), "One Apply is one Undo step");
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, changed);
    }

    #[test]
    fn apply_button_rejects_all_edits_when_one_field_is_invalid() {
        let mut app = fixture();
        let original = app.doc.clone();
        let _ = app.update(Message::NumericTransform(Action::More(true)));
        for (field, input) in [(Field::Rotation, "72"), (Field::TiltX, "90")] {
            let _ = app.update(Message::NumericTransform(Action::Input(
                field,
                input.into(),
            )));
        }
        let _ = app.update(Message::NumericTransform(Action::ApplyAll));
        assert!(app.error);
        assert!(app.status.starts_with("Tilt X: "), "{}", app.status);
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
        assert_eq!(app.numeric_transforms.rotation, "72", "Typed values stay");
        app.selected = vec![app.doc.graphics[0].id];
        let _ = app.update(Message::Tick);
        assert_eq!(app.numeric_transforms.rotation, "0");
        assert!(
            app.numeric_transforms.more,
            "More stays open across selections"
        );
        // Closed More hides tilt, so Apply leaves a typed tilt alone.
        for (field, input) in [(Field::Rotation, "72"), (Field::TiltX, "90")] {
            let _ = app.update(Message::NumericTransform(Action::Input(
                field,
                input.into(),
            )));
        }
        let _ = app.update(Message::NumericTransform(Action::More(false)));
        let _ = app.update(Message::NumericTransform(Action::ApplyAll));
        assert!(!app.error, "{}", app.status);
        assert_ne!(app.doc, original);
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
    }

    #[test]
    fn point_dimensions_account_for_fixed_labels_and_lock_only_coordinate_proportions() {
        for lock in [true, false] {
            for field in [Field::Width, Field::Height] {
                let mut app = fixture();
                let original = app.doc.clone();
                let current = extent(&app.doc, &app.selected, field).unwrap();
                let target = current * 1.65;
                let _ = app.update(Message::NumericTransform(Action::Proportional(lock)));
                apply(&mut app, field, target);
                assert!(!app.error, "{}", app.status);
                assert!((extent(&app.doc, &app.selected, field).unwrap() - target).abs() < 0.005);
                let old = &original.graphics[0];
                let new = &app.doc.graphics[0];
                let x_scale = new.axis_x.x / old.axis_x.x;
                let y_scale = new.axis_y.y / old.axis_y.y;
                if lock {
                    assert!((x_scale - y_scale).abs() < 0.0001);
                } else if field == Field::Width {
                    assert_eq!(old.axis_y, new.axis_y);
                } else {
                    assert_eq!(old.axis_x, new.axis_x);
                }
            }
        }
    }

    #[test]
    fn invalid_and_noop_values_do_not_mutate_or_consume_history() {
        let mut app = fixture();
        let original = app.doc.clone();
        for field in Field::ALL {
            for input in ["", "-", "hello", "NaN", "inf", "-inf", "1e40"] {
                apply(&mut app, field, input);
                assert!(app.error, "{field:?} accepted {input}");
                assert_eq!(app.doc, original);
                assert!(!app.history.can_undo());
            }
        }
        for (field, input) in [
            (Field::Rotation, "36001"),
            (Field::TiltX, "86"),
            (Field::TiltY, "-86"),
            (Field::Width, "0"),
            (Field::Height, "-1"),
            (Field::Scale, "0"),
            (Field::Scale, "-100"),
            (Field::Scale, "1000001"),
        ] {
            apply(&mut app, field, input);
            assert!(app.error);
            assert_eq!(app.doc, original);
            assert!(!app.history.can_undo());
        }
        for (field, input) in [
            (Field::Rotation, "360"),
            (Field::Rotation, "-720"),
            (Field::TiltX, "0"),
            (Field::TiltY, "0"),
            (Field::Scale, "100"),
        ] {
            apply(&mut app, field, input);
            assert!(!app.error);
            assert_eq!(app.doc, original);
            assert!(!app.history.can_undo());
        }
        app.numeric_transforms.key = None;
        app.sync_numeric_transforms();
        for field in [Field::Width, Field::Height] {
            let _ = app.update(Message::NumericTransform(Action::Apply(field)));
            assert_eq!(app.doc, original, "Displayed rounded sizes are no-ops");
            assert!(!app.history.can_undo());
        }
    }

    #[test]
    fn numeric_apply_preserves_caption_drafts_until_a_valid_change() {
        use iced::widget::text_editor::{Action as TextAction, Edit as TextEdit};
        for (field, input, invalid) in [
            (Field::Width, "NaN", true),
            (Field::Width, "0", true),
            (Field::Width, "1000", true),
            (Field::Scale, "100", false),
            (Field::Rotation, "360", false),
        ] {
            let mut app = fixture();
            let original = app.doc.clone();
            apply(&mut app, Field::Scale, "125");
            let redo = app.doc.clone();
            let _ = app.update(Message::Undo);
            let id = app.doc.annotations[0].id;
            let _ = app.update(Message::InlineText(
                super::super::inline_text::Action::Begin(Some(id), Point::default()),
            ));
            let _ = app.update(Message::CaptionAction(TextAction::Edit(TextEdit::Paste(
                " draft".to_owned().into(),
            ))));
            let draft = app.caption.clone();
            let format = app.caption_format.clone();
            let revision = app.revision;
            let draft_history = app.text_history_available(false);
            apply(&mut app, field, input);
            assert_eq!(app.error, invalid, "{field:?}: {}", app.status);
            assert_eq!(app.doc, original);
            assert_eq!(app.caption, draft);
            assert_eq!(app.caption_editor.text(), draft);
            assert_eq!(app.caption_format, format);
            assert_eq!(app.revision, revision);
            assert_eq!(app.text_history_available(false), draft_history);
            assert!(app.inline_text.is_some());
            assert_eq!(app.selected, vec![id]);
            assert!(!app.history.can_undo());
            assert!(app.history.can_redo());
            let _ = app.update(Message::InlineText(
                super::super::inline_text::Action::Finish(false),
            ));
            let _ = app.update(Message::Redo);
            assert_eq!(app.doc, redo);
        }
        let mut app = fixture();
        let original = app.doc.clone();
        let id = app.doc.annotations[0].id;
        let _ = app.update(Message::InlineText(
            super::super::inline_text::Action::Begin(Some(id), Point::default()),
        ));
        let _ = app.update(Message::CaptionAction(TextAction::Edit(TextEdit::Paste(
            " draft".to_owned().into(),
        ))));
        let mut committed = original.clone();
        committed.annotations[0].text = app.caption.clone();
        committed.annotations[0].format = app.caption_format.clone();
        apply(&mut app, Field::Scale, "125");
        assert!(!app.error, "{}", app.status);
        assert!(app.inline_text.is_none());
        assert_eq!(app.doc.annotations[0].text, committed.annotations[0].text);
        assert_ne!(
            app.doc.annotations[0].position,
            committed.annotations[0].position
        );
        let transformed = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(
            app.doc, committed,
            "Transform is one Undo step after committing the caption"
        );
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
        let _ = app.update(Message::Redo);
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, transformed);
    }

    #[test]
    fn degenerate_sizes_and_fixed_text_report_unreachable_targets_without_changes() {
        for vertical in [true, false] {
            let (mut app, _) = App::new();
            let a = app.doc.add_atom("C", Point::default());
            let b = app.doc.add_atom(
                "C",
                if vertical {
                    Point::new(0., 42.)
                } else {
                    Point::new(42., 0.)
                },
            );
            app.doc.add_bond(a, b, 1, "plain");
            app.selected = vec![a, b];
            app.sync_numeric_transforms();
            let original = app.doc.clone();
            apply(
                &mut app,
                if vertical {
                    Field::Width
                } else {
                    Field::Height
                },
                "10",
            );
            assert!(app.error);
            assert_eq!(app.doc, original);
            assert!(!app.history.can_undo());
        }
        let mut app = fixture();
        app.selected = vec![app.doc.annotations[0].id];
        app.sync_numeric_transforms();
        let original = app.doc.clone();
        for field in [Field::Width, Field::Height] {
            apply(&mut app, field, "100");
            assert!(app.error);
            assert_eq!(app.doc, original);
            assert!(!app.history.can_undo());
        }
        app.selected.clear();
        app.sync_numeric_transforms();
        apply(&mut app, Field::Rotation, "72");
        assert!(app.error);
        assert_eq!(app.doc, original);
    }

    #[test]
    fn pending_numbers_reset_with_selection_and_undo_but_typing_does_not_edit() {
        let mut app = fixture();
        let original = app.doc.clone();
        let _ = app.update(Message::NumericTransform(Action::Input(
            Field::Width,
            "160".into(),
        )));
        assert_eq!(app.numeric_transforms.width, "160");
        assert_eq!(app.doc, original);
        assert!(!app.history.can_undo());
        app.selected = vec![app.doc.graphics[0].id];
        let _ = app.update(Message::Tick);
        assert_ne!(app.numeric_transforms.width, "160");
        let old_width = app.numeric_transforms.width.clone();
        apply(&mut app, Field::Scale, "200");
        assert_eq!(app.numeric_transforms.scale, "100");
        assert_ne!(app.numeric_transforms.width, old_width);
        let _ = app.update(Message::Undo);
        assert_eq!(app.numeric_transforms.width, old_width);
    }

    #[test]
    fn computed_label_refresh_updates_dimensions_without_discarding_pending_input() {
        let (mut app, _) = App::new();
        let oxygen = app.doc.add_atom("O", Point::default());
        app.selected = vec![oxygen];
        app.sync_numeric_transforms();
        let old_width = app.numeric_transforms.width.clone();
        let _ = app.update(Message::NumericTransform(Action::Input(
            Field::Height,
            "65".into(),
        )));
        let _ = app.update(Message::NumericTransform(Action::Input(
            Field::Rotation,
            "72".into(),
        )));
        let mut checked = app.doc.clone();
        checked.atom_mut(oxygen).unwrap().label_h = 2;
        let _ = app.update(Message::EngineDone {
            revision: app.revision,
            kind: super::super::Job::Analyze,
            result: Box::new(Ok(reshiki::engine::Response {
                document: Some(checked),
                analysis: None,
                output: None,
                engine_version: "test".into(),
                warnings: vec![],
            })),
        });
        assert_ne!(app.numeric_transforms.width, old_width);
        assert_eq!(app.numeric_transforms.height, "65");
        assert_eq!(app.numeric_transforms.rotation, "72");
        assert!(!app.history.can_undo());
    }

    #[test]
    fn collapsed_abbreviations_keep_hidden_atoms_and_projection_depth_in_scale() {
        let (mut app, _) = App::new();
        let n = app.doc.add_atom("N", Point::new(-42., 0.));
        let c = app.doc.add_atom("C", Point::default());
        app.doc.add_bond(n, c, 1, "plain");
        app.doc =
            reshiki::atom_text::apply(&app.doc, c, "Boc", reshiki::atom_text::Mode::Auto).unwrap();
        let ids = app.doc.all_ids();
        reshiki::projection::tilt(&mut app.doc, &ids, 25., true);
        app.selected = vec![n, c];
        app.sync_numeric_transforms();
        let original = app.doc.clone();
        apply(&mut app, Field::Scale, "150");
        assert!(!app.error, "{}", app.status);
        assert_eq!(app.doc.bonds, original.bonds);
        assert_eq!(app.doc.abbreviations, original.abbreviations);
        for (now, old) in app.doc.atoms.iter().zip(&original.atoms) {
            assert!((now.depth - old.depth * 1.5).abs() < 0.0001);
        }
        let changed = app.doc.clone();
        let _ = app.update(Message::Undo);
        assert_eq!(app.doc, original);
        apply(&mut app, Field::Scale, "NaN");
        apply(&mut app, Field::Rotation, "360");
        let _ = app.update(Message::Redo);
        assert_eq!(app.doc, changed, "Invalid and no-op edits retain redo");
    }

    #[test]
    fn crossed_fixed_labels_can_reach_smaller_bounds_without_collapsing() {
        use reshiki::atom_labels::HydrogenPosition;
        let mut doc = Document::default();
        let left = doc.add_atom("O", Point::new(-18., 0.));
        let right = doc.add_atom("O", Point::new(18., 0.));
        doc.atom_mut(left).unwrap().label_h = 1;
        doc.atom_mut(left).unwrap().display.hydrogen_position = HydrogenPosition::Right;
        doc.atom_mut(right).unwrap().label_h = 1;
        doc.atom_mut(right).unwrap().display.hydrogen_position = HydrogenPosition::Left;
        let ids = vec![left, right];
        let mut target_doc = doc.clone();
        editing::scale_axes_about(&mut target_doc, &ids, Point::default(), 0.65, 1.);
        let target = extent(&target_doc, &ids, Field::Width).unwrap();
        let changed = transformed(&doc, &ids, Field::Width, &target.to_string(), false).unwrap();
        assert!((extent(&changed, &ids, Field::Width).unwrap() - target).abs() < 0.005);
    }

    #[tokio::test]
    async fn exact_numeric_transforms_keep_tetrahedral_and_double_bond_identity() {
        let engine = LocalEngine::default();
        let original = engine
            .execute(Request::import_smiles("C[C@H](O)/C=C/F"))
            .await
            .unwrap()
            .document
            .unwrap();
        let expected = engine
            .execute(Request::molecule("analyze", original.clone()))
            .await
            .unwrap()
            .analysis
            .unwrap();
        let (mut app, _) = App::new();
        app.doc = original.clone();
        app.selected = app.doc.all_ids();
        app.sync_numeric_transforms();
        for (field, input) in [
            (Field::Rotation, "72"),
            (Field::TiltX, "21.5"),
            (Field::TiltY, "-17.25"),
            (Field::Scale, "125"),
            (Field::Width, "100"),
            (Field::Height, "65"),
        ] {
            let _ = app.update(Message::NumericTransform(Action::Proportional(false)));
            apply(&mut app, field, input);
            assert!(!app.error, "{}", app.status);
            let bonds_without_computed_labels = |doc: &Document| {
                doc.bonds
                    .iter()
                    .cloned()
                    .map(|mut bond| {
                        bond.cip_label = None;
                        bond
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(
                bonds_without_computed_labels(&app.doc),
                bonds_without_computed_labels(&original)
            );
            for (now, old) in app.doc.atoms.iter().zip(&original.atoms) {
                assert_eq!(now.stereo, old.stereo);
            }
            let actual = engine
                .execute(Request::molecule("analyze", app.doc.clone()))
                .await
                .unwrap()
                .analysis
                .unwrap();
            assert_eq!(actual.inchikey, expected.inchikey, "{field:?}");
            assert_eq!(actual.formula, expected.formula);
        }
    }

    #[tokio::test]
    #[ignore = "Manual GPU input and layout check; writes actual renderer evidence"]
    async fn numeric_panel_layout_input_and_renderer_evidence() {
        use iced::advanced::{layout, mouse, renderer::Headless, widget::Tree};
        use iced::keyboard::{self, Key, key};
        let directory = std::env::temp_dir().join("reshiki-numeric-transforms-qa");
        std::fs::create_dir_all(&directory).unwrap();
        let mut app = fixture();
        // Tilt shown, and an edit that enables Apply.
        app.numeric_transforms.more = true;
        app.numeric_transforms.scale = "125".into();
        std::fs::write(
            directory.join("numeric-transforms.rsk"),
            serde_json::to_vec_pretty(&app.doc).unwrap(),
        )
        .unwrap();
        for width in [246, 268] {
            let height = 460;
            let size = iced::Size::new(width as f32, height as f32);
            let mut renderer = <iced::Renderer as Headless>::new(
                iced::Font::with_name(reshiki::style::ui_font_family()),
                iced::Pixels(16.),
                None,
            )
            .await
            .unwrap();
            let mut view = app.numeric_transform_panel();
            let mut tree = Tree::new(view.as_widget());
            let node = view.as_widget_mut().layout(
                &mut tree,
                &renderer,
                &layout::Limits::new(iced::Size::ZERO, size),
            );
            assert!(node.size().height <= size.height);
            let layout = iced::advanced::Layout::new(&node);
            fn inside(layout: iced::advanced::Layout<'_>, width: f32) {
                let bounds = layout.bounds();
                assert!(bounds.x >= 0. && bounds.x + bounds.width <= width + 0.01);
                layout.children().for_each(|child| inside(child, width));
            }
            inside(layout, width as f32);
            // Rotate | Scale, W | 🔒 H, Tilt X | Tilt Y, then More … Apply.
            let rows: Vec<_> = layout.children().collect();
            let input = |row: usize, column: usize| {
                let cell = rows[row].children().nth(column).unwrap();
                let stack = cell.children().nth(1).unwrap();
                stack.children().next().unwrap().bounds()
            };
            let fields = [
                (Field::Rotation, input(0, 0)),
                (Field::Scale, input(0, 1)),
                (Field::Width, input(1, 0)),
                (Field::Height, input(1, 1)),
                (Field::TiltX, input(2, 0)),
                (Field::TiltY, input(2, 1)),
            ];
            fn first(layout: iced::advanced::Layout<'_>) -> iced::advanced::Layout<'_> {
                layout.children().next().unwrap()
            }
            // The lock leads the H label: cell → label container → row → button.
            let lock = first(first(first(rows[1].children().nth(1).unwrap()))).bounds();
            let more = rows[3].children().next().unwrap().bounds();
            let apply_all = rows[3].children().nth(2).unwrap().bounds();
            let mut messages = Vec::new();
            let mut event = |event, cursor| {
                view.as_widget_mut().update(
                    &mut tree,
                    &event,
                    layout,
                    cursor,
                    &renderer,
                    &mut iced::advanced::clipboard::Null,
                    &mut iced::advanced::Shell::new(&mut messages),
                    &iced::Rectangle::with_size(size),
                );
            };
            event(
                iced::Event::Window(iced::window::Event::RedrawRequested(
                    std::time::Instant::now(),
                )),
                mouse::Cursor::Unavailable,
            );
            let click = |event: &mut dyn FnMut(iced::Event, mouse::Cursor),
                         bounds: iced::Rectangle| {
                let cursor = mouse::Cursor::Available(bounds.center());
                event(
                    iced::Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)),
                    cursor,
                );
                event(
                    iced::Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)),
                    cursor,
                );
            };
            let press = |event: &mut dyn FnMut(iced::Event, mouse::Cursor),
                         key: Key,
                         physical_key,
                         modifiers,
                         text| {
                event(
                    iced::Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
                    mouse::Cursor::Unavailable,
                );
                event(
                    iced::Event::Keyboard(keyboard::Event::KeyPressed {
                        modified_key: key.clone(),
                        key,
                        physical_key: key::Physical::Code(physical_key),
                        location: keyboard::Location::Standard,
                        modifiers,
                        text,
                        repeat: false,
                    }),
                    mouse::Cursor::Unavailable,
                );
            };
            let enter = |event: &mut dyn FnMut(iced::Event, mouse::Cursor)| {
                press(
                    event,
                    Key::Named(key::Named::Enter),
                    key::Code::Enter,
                    keyboard::Modifiers::empty(),
                    None,
                )
            };
            click(&mut event, fields[0].1);
            press(
                &mut event,
                Key::Character("a".into()),
                key::Code::KeyA,
                keyboard::Modifiers::COMMAND,
                None,
            );
            press(
                &mut event,
                Key::Character("7".into()),
                key::Code::Digit7,
                keyboard::Modifiers::empty(),
                Some("7".into()),
            );
            press(
                &mut event,
                Key::Character("2".into()),
                key::Code::Digit2,
                keyboard::Modifiers::empty(),
                Some("2".into()),
            );
            for (_, bounds) in fields {
                click(&mut event, bounds);
                enter(&mut event);
            }
            for bounds in [lock, more, apply_all] {
                click(&mut event, bounds);
            }
            assert!(messages.iter().any(|m| matches!(m, Message::NumericTransform(Action::Input(Field::Rotation, value)) if value == "72")), "{messages:?}");
            for field in Field::ALL {
                assert!(messages.iter().any(|m| matches!(m, Message::NumericTransform(Action::Apply(actual)) if *actual == field)), "Enter in {field:?}: {messages:?}");
            }
            let sent = |wanted: fn(&Action) -> bool| {
                messages
                    .iter()
                    .any(|m| matches!(m, Message::NumericTransform(action) if wanted(action)))
            };
            assert!(sent(|a| matches!(a, Action::Proportional(false))));
            assert!(sent(|a| matches!(a, Action::More(false))));
            assert!(sent(|a| matches!(a, Action::ApplyAll)), "{messages:?}");
            let theme = app.theme();
            view.as_widget().draw(
                &tree,
                &mut renderer,
                &theme,
                &iced::advanced::renderer::Style::default(),
                layout,
                mouse::Cursor::Unavailable,
                &iced::Rectangle::with_size(size),
            );
            let pixels = Headless::screenshot(
                &mut renderer,
                iced::Size::new(width, height),
                1.,
                theme.palette().background,
            );
            image::save_buffer(
                directory.join(format!("numeric-panel-{width}.png")),
                &pixels,
                width,
                height,
                image::ColorType::Rgba8,
            )
            .unwrap();
        }
        let original = app.doc.clone();
        apply(&mut app, Field::Rotation, "72");
        apply(&mut app, Field::Scale, "125");
        for (name, doc) in [("original", original), ("rotated-scaled", app.doc.clone())] {
            std::fs::write(
                directory.join(format!("{name}.png")),
                reshiki::export::drawing(&doc, "png").unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    #[ignore = "Manual explicit-Apply latency check on a 1000-atom document"]
    fn numeric_size_apply_latency() {
        let mut doc = Document::default();
        for index in 0..1000 {
            let id = doc.add_atom(
                "C",
                Point::new(index as f32 * 36., (index % 2) as f32 * 21.),
            );
            if index > 0 {
                doc.add_bond(id - 1, id, 1, "plain");
            }
        }
        let all = doc.all_ids();
        for (name, ids) in [
            ("small selection", all[..6].to_vec()),
            ("whole selection", all),
        ] {
            let target = extent(&doc, &ids, Field::Width).unwrap() * 1.5;
            let start = std::time::Instant::now();
            let changed =
                transformed(&doc, &ids, Field::Width, &target.to_string(), false).unwrap();
            println!("1000 atoms, {name}: {:?}", start.elapsed());
            assert!((extent(&changed, &ids, Field::Width).unwrap() - target).abs() < 0.005);
        }
        let left = doc.add_atom("O", Point::new(-18., -100.));
        let right = doc.add_atom("O", Point::new(18., -100.));
        for (id, position) in [
            (left, reshiki::atom_labels::HydrogenPosition::Right),
            (right, reshiki::atom_labels::HydrogenPosition::Left),
        ] {
            let atom = doc.atom_mut(id).unwrap();
            atom.label_h = 1;
            atom.display.hydrogen_position = position;
        }
        let ids = [left, right];
        let mut sample = doc.clone();
        editing::scale_axes_about(&mut sample, &ids, Point::new(0., -100.), 0.65, 1.);
        let target = extent(&sample, &ids, Field::Width).unwrap();
        let start = std::time::Instant::now();
        let changed = transformed(&doc, &ids, Field::Width, &target.to_string(), false).unwrap();
        println!("1002 atoms, inward-label fallback: {:?}", start.elapsed());
        assert!((extent(&changed, &ids, Field::Width).unwrap() - target).abs() < 0.005);
    }
}
