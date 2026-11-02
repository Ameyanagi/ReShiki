use super::{BracketSides, Graphic};
use crate::{
    document::Point,
    style,
    typography::{Script, TextStyle},
};

/// Upright annotation positioned in drawing coordinates. `position` is the
/// text's top-left origin and `size` already includes superscript scaling.
pub struct GraphicLabel<'a> {
    pub text: &'a str,
    pub position: Point,
    pub size: f32,
    pub style: TextStyle,
}

impl GraphicLabel<'_> {
    pub fn ink_boxes(&self) -> Vec<(Point, Point)> {
        style::text_ink_boxes(self.text, self.size, &self.style)
            .into_iter()
            .map(|(lo, hi)| {
                (
                    self.position.offset(lo.x, lo.y),
                    self.position.offset(hi.x, hi.y),
                )
            })
            .collect()
    }
}

impl Graphic {
    /// The closing bracket's superscript follows its affine frame while its
    /// text stays upright. A left-only bracket has no closing corner.
    pub fn superscript_label(&self, text_style: &TextStyle) -> Option<GraphicLabel<'_>> {
        let text = self
            .superscript
            .as_deref()
            .filter(|text| !text.is_empty())?;
        if !self.kind.brackets() || self.sides == BracketSides::Left {
            return None;
        }
        let style = TextStyle {
            color: self.style.stroke,
            script: Script::Superscript,
            formula: false,
            ..text_style.clone()
        };
        let size = style.size();
        let corner = self.origin.offset(self.axis_x.x, self.axis_x.y);
        let x = self.axis_x.distance(Point::default());
        let y = self.axis_y.distance(Point::default());
        let right = if x == 0. {
            Point::new(1., 0.)
        } else {
            Point::new(self.axis_x.x / x, self.axis_x.y / x)
        };
        let down = if y == 0. {
            Point::new(0., 1.)
        } else {
            Point::new(self.axis_y.x / y, self.axis_y.y / y)
        };
        let gap = self.style.width() * 0.5 + size * 0.15;
        Some(GraphicLabel {
            text,
            position: corner.offset(
                right.x * gap - down.x * size * 0.65,
                right.y * gap - down.y * size * 0.65,
            ),
            size,
            style,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        document::Document,
        graphics::{GraphicKind, GraphicStyle},
        scene::{self, Primitive},
    };

    fn bracket() -> Graphic {
        Graphic::dragged(
            1,
            GraphicKind::Brackets,
            Point::new(10., 20.),
            Point::new(110., 100.),
            GraphicStyle::default(),
            BracketSides::Both,
            false,
        )
    }

    #[test]
    fn bracket_superscript_is_optional_in_old_native_documents() {
        let original = bracket();
        let old = serde_json::to_value(&original).unwrap();
        assert!(old.get("superscript").is_none());
        assert_eq!(serde_json::from_value::<Graphic>(old).unwrap(), original);
        for text in ["‡", "+", "−", "•+", "•−", "2−", "custom <&>"] {
            let mut graphic = original.clone();
            graphic.superscript = Some(text.into());
            let encoded = serde_json::to_value(&graphic).unwrap();
            assert_eq!(encoded["superscript"], text);
            assert_eq!(serde_json::from_value::<Graphic>(encoded).unwrap(), graphic);
            graphic.validate().unwrap();
        }
    }

    #[test]
    fn bracket_superscript_tracks_translation_resize_and_rotation_at_typography_size() {
        let mut graphic = bracket();
        graphic.superscript = Some("•+".into());
        let style = TextStyle {
            size_pt: 16.,
            family: "test font".into(),
            ..Default::default()
        };
        let label = graphic.superscript_label(&style).unwrap();
        let position = label.position;
        let size = TextStyle {
            script: Script::Superscript,
            ..style.clone()
        }
        .size();
        assert_eq!(label.size, size);
        assert_eq!(label.style.family, style.family);
        assert!(position.x > 110. && position.y < 20.);
        graphic.map_positions(|p| p.offset(35., -18.));
        assert_eq!(
            graphic.superscript_label(&style).unwrap().position,
            position.offset(35., -18.)
        );
        graphic.axis_x.x += 50.;
        assert_eq!(
            graphic.superscript_label(&style).unwrap().position,
            position.offset(85., -18.)
        );
        graphic.map_positions(|p| Point::new(-p.y, p.x));
        let rotated = graphic.superscript_label(&style).unwrap();
        assert_eq!(
            rotated.position,
            Point::new(-(position.y - 18.), position.x + 85.)
        );
        assert_eq!(rotated.size, size);
        assert_eq!(rotated.style, label_style(&style, graphic.style.stroke));
    }

    fn label_style(style: &TextStyle, color: crate::palette::Color) -> TextStyle {
        TextStyle {
            color,
            script: Script::Superscript,
            formula: false,
            ..style.clone()
        }
    }

    #[test]
    fn bracket_superscript_render_selection_and_hit_share_the_positioned_ink() {
        let mut doc = Document::default();
        let mut graphic = bracket();
        graphic.superscript = Some("2−".into());
        let style = doc.drawing_style.text_style();
        let label = graphic.superscript_label(&style).unwrap();
        let boxes = label.ink_boxes();
        let (lo, hi) = graphic.bounds_with_text_style(&style);
        for (a, b) in &boxes {
            assert!(a.x >= lo.x && a.y >= lo.y && b.x <= hi.x && b.y <= hi.y);
            assert!(graphic.hit_with_text_style(a.lerp(*b, 0.5), 0., &style));
        }
        let expected = (
            label.position,
            label.text.to_string(),
            label.size,
            label.style.clone(),
        );
        doc.graphics.push(graphic);
        let texts: Vec<_> = scene::primitives(&doc)
            .into_iter()
            .filter_map(|p| {
                if let Primitive::Text {
                    position,
                    text,
                    size,
                    style,
                    ..
                } = p
                {
                    Some((position, text, size, style))
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(texts, [expected]);
        assert_eq!(scene::selection_bounds(&doc, &[1]), Some((lo, hi)));
        assert!(doc.atoms.is_empty());
    }

    #[test]
    fn absent_empty_and_left_only_bracket_superscripts_do_not_add_ink() {
        let mut graphic = bracket();
        let original_bounds = graphic.bounds();
        assert!(graphic.superscript_label(&TextStyle::default()).is_none());
        graphic.superscript = Some(String::new());
        assert!(graphic.superscript_label(&TextStyle::default()).is_none());
        assert_eq!(graphic.bounds(), original_bounds);
        graphic.superscript = Some("+".into());
        graphic.sides = BracketSides::Left;
        assert!(graphic.superscript_label(&TextStyle::default()).is_none());
        graphic.kind = GraphicKind::Rectangle;
        assert!(graphic.validate().is_err());
    }

    #[test]
    fn a_fast_eraser_stroke_crossing_only_the_superscript_erases_its_bracket() {
        let mut doc = Document::default();
        doc.drawing_style.font_size_pt = 20.;
        let mut graphic = bracket();
        graphic.superscript = Some("custom".into());
        let label = graphic
            .superscript_label(&doc.drawing_style.text_style())
            .unwrap();
        let (lo, hi) = label.ink_boxes()[0];
        let middle = lo.lerp(hi, 0.5);
        doc.graphics.push(graphic);
        crate::erasing::stroke(
            &mut doc,
            Point::new(middle.x, lo.y - 5.),
            Point::new(middle.x, hi.y + 5.),
            0.,
        );
        assert!(doc.graphics.is_empty());
    }
}
