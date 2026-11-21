use super::*;
use crate::{
    arrows::{ArrowStyle, Body, Preset},
    canvas_theme::{CanvasTheme, ColorTheme},
    document::{Annotation, Arrow},
    graphics::{
        Graphic, GraphicKind, GraphicStyle,
        table::{Cell, Table},
    },
    palette::Color,
};

fn rectangle(id: u64, fill: Color, layer: i32) -> Graphic {
    let mut g = Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        Point::default(),
        Point::new(300., 100.),
        GraphicStyle {
            fill: Some(fill),
            ..Default::default()
        },
        Default::default(),
        false,
    );
    g.layer = layer;
    g
}

fn table(members: Vec<Vec<u64>>) -> Graphic {
    let mut g = rectangle(1000, Color::Paper, -1);
    g.kind = GraphicKind::Table;
    g.axis_x = Point::new(200., 0.);
    g.style.fill = None;
    let count = members.len() as f32;
    g.table = Some(Box::new(Table {
        cells: members
            .into_iter()
            .enumerate()
            .map(|(i, members)| Cell {
                id: i as u32 + 1,
                lo: Point::new(i as f32 / count, 0.),
                hi: Point::new((i + 1) as f32 / count, 1.),
                members,
                borders: vec![],
            })
            .collect(),
    }));
    g
}

fn caption(id: u64, x: f32, text: &str) -> Annotation {
    Annotation {
        id,
        position: Point::new(x, 65.),
        text: text.into(),
        format: Default::default(),
        angle: 0.,
    }
}

fn text_color(parts: &[Primitive], wanted: &str) -> Option<[u8; 3]> {
    parts.iter().find_map(|p| match p {
        Primitive::Text { text, color, .. } if text == wanted => Some(*color),
        Primitive::Clip { primitives, .. }
        | Primitive::Rotated { primitives, .. }
        | Primitive::Halo { primitives, .. } => text_color(primitives, wanted),
        Primitive::Opacity { primitive, .. } => {
            text_color(std::slice::from_ref(primitive.as_ref()), wanted)
        }
        _ => None,
    })
}

#[test]
fn cells_read_ordinary_backdrops_in_table_paint_order_with_themes_and_gradient_arrows() {
    for canvas in CanvasTheme::ALL {
        for theme in [ColorTheme::Publication, ColorTheme::Presentation] {
            for (table_layer, light_layer, ink) in [
                (-1, None, CanvasTheme::Dark),
                (-1, Some(-1), CanvasTheme::Dark),
                (-1, Some(0), CanvasTheme::Dark),
                (-1, Some(-2), CanvasTheme::Light),
                (1, Some(0), CanvasTheme::Light),
            ] {
                let mut doc = Document {
                    canvas_theme: canvas,
                    color_theme: theme,
                    ..Default::default()
                };
                let atom = doc.add_atom("O", Point::new(35., 30.));
                doc.annotations.push(caption(40, 15., "cell"));
                let mut style = ArrowStyle::preset(Preset::Forward);
                style.set_body(Body::Gradient);
                doc.arrows.push(Arrow::new(
                    50,
                    Point::new(15., 50.),
                    Point::new(75., 50.),
                    Preset::Forward,
                    style,
                ));
                doc.graphics
                    .push(rectangle(300, Color::Custom([24; 3]), -3));
                let mut g = table(vec![vec![atom, 40, 50], vec![]]);
                g.layer = table_layer;
                let corners = g.table.as_ref().unwrap().cells[0]
                    .frame(&g)
                    .unwrap()
                    .corners();
                doc.graphics.push(g);
                if let Some(layer) = light_layer {
                    doc.graphics
                        .push(rectangle(301, Color::Custom([240; 3]), layer));
                }
                doc.validate().unwrap();
                let before = doc.clone();
                let scene = crate::scene::primitives(&doc);
                assert_eq!(doc, before);
                let (clip_index, content) = scene
                    .iter()
                    .enumerate()
                    .find_map(|(i, p)| match p {
                        Primitive::Clip {
                            corners: c,
                            primitives,
                        } if *c == corners => Some((i, primitives)),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(
                    text_color(content, "O"),
                    Some(canvas.color(crate::canvas_theme::element_color(&doc, "O", ink))),
                    "{canvas:?}/{theme:?}, table {table_layer}, light {light_layer:?}",
                );
                assert_eq!(
                    text_color(content, "cell"),
                    Some(canvas.color(ink.color([0; 3])))
                );
                let stops = content
                    .iter()
                    .find_map(|p| match p {
                        Primitive::Gradient { stops, .. } => Some(stops),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(stops.first().unwrap().1, ink.background());
                assert_eq!(stops.last().unwrap().1, ink.color([0; 3]));
                for (rgb, before_table) in [(24, true), (240, ink.is_light())] {
                    if rgb == 240 && light_layer.is_none() {
                        continue;
                    }
                    let index = scene
                        .iter()
                        .position(|p| {
                            matches!(p, Primitive::Path { style, filled: true, .. }
                            if style.fill.is_some_and(|fill| canvas.color(fill.rgb()) == [rgb; 3]))
                        })
                        .unwrap();
                    assert_eq!(index < clip_index, before_table);
                }
            }
        }
    }
}

#[test]
fn cell_owned_fills_keep_their_clip_and_layer_without_recoloring_other_scopes() {
    for front in [false, true] {
        let mut doc = Document::default();
        let first = doc.add_atom("O", Point::new(35., 30.));
        let second = doc.add_atom("O", Point::new(135., 30.));
        doc.add_atom("O", Point::new(250., 30.));
        doc.annotations
            .extend([caption(40, 15., "left"), caption(41, 115., "right")]);
        doc.graphics.push(rectangle(
            50,
            Color::Custom([24; 3]),
            if front { 0 } else { -1 },
        ));
        doc.graphics
            .push(table(vec![vec![first, 40, 50], vec![second, 41]]));
        doc.validate().unwrap();
        let before = doc.clone();
        let scene = crate::scene::primitives(&doc);
        assert_eq!(doc, before);
        let clips: Vec<_> = scene
            .iter()
            .filter_map(|p| match p {
                Primitive::Clip { primitives, .. } => Some(primitives),
                _ => None,
            })
            .collect();
        let [left, right] = clips.as_slice() else {
            panic!("two cell clips")
        };
        let ink = if front { [0; 3] } else { [255; 3] };
        assert_eq!(text_color(left, "O"), Some(ink));
        assert_eq!(text_color(left, "left"), Some(ink));
        assert_eq!(text_color(right, "O"), Some([0; 3]));
        assert_eq!(text_color(right, "right"), Some([0; 3]));
        let ordinary: Vec<_> = scene
            .iter()
            .filter(|p| !matches!(p, Primitive::Clip { .. }))
            .cloned()
            .collect();
        assert_eq!(text_color(&ordinary, "O"), Some([0; 3]));
        assert!(
            !ordinary
                .iter()
                .any(|p| matches!(p, Primitive::Path { filled: true, .. }))
        );
    }
}
