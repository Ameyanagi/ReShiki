//! One TLC placement factory for the live gesture and its committed graphic.
use reshiki::{
    document::Point,
    graphics::{
        BracketSides, Graphic, GraphicKind, GraphicStyle, LinePattern, TlcLane, TlcPlate, TlcSpot,
    },
};

pub(crate) fn spot(style: &GraphicStyle, rf: f64) -> TlcSpot {
    TlcSpot {
        rf,
        width_pt: 325095. / 65536.,
        height_pt: 325095. / 65536.,
        style: GraphicStyle {
            fill: Some(style.stroke),
            pattern: LinePattern::Solid,
            effects: Default::default(),
            corner_pt: None,
            ..style.clone()
        },
        show_rf: false,
        label: None,
    }
}

pub(crate) fn dragged(
    id: u64,
    kind: GraphicKind,
    start: Point,
    end: Point,
    style: GraphicStyle,
    sides: BracketSides,
    constrain: bool,
) -> Graphic {
    if kind != GraphicKind::TlcPlate {
        return Graphic::dragged(id, kind, start, end, style, sides, constrain);
    }
    // Remembered rectangle fill, gradients and shadows do not become plate data.
    // The source style stays available when another drawing tool is chosen.
    let style = GraphicStyle {
        fill: None,
        pattern: LinePattern::Solid,
        effects: Default::default(),
        corner_pt: None,
        ..style
    };
    let mut graphic = Graphic::dragged(
        id,
        GraphicKind::Rectangle,
        start,
        end,
        style,
        BracketSides::Both,
        false,
    );
    graphic.kind = GraphicKind::TlcPlate;
    graphic.tlc = Some(Box::new(TlcPlate {
        lanes: (0..3)
            .map(|_| TlcLane {
                spots: vec![spot(&graphic.style, 0.15)],
            })
            .collect(),
        ..Default::default()
    }));
    graphic
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn default_native_plate_is_valid_and_has_three_filled_point15_spots() {
        let style = GraphicStyle {
            fill: Some(reshiki::palette::Color::Custom([40, 80, 120])),
            effects: reshiki::graphics::Effects {
                shaded: true,
                ..Default::default()
            },
            ..Default::default()
        };
        let g = dragged(
            9,
            GraphicKind::TlcPlate,
            Point::new(20., 30.),
            Point::new(100., 430.),
            style,
            BracketSides::Both,
            true,
        );
        g.validate().unwrap();
        let t = g.tlc.as_deref().unwrap();
        assert_eq!(t.lanes.len(), 3);
        assert!(t.show_origin && t.show_solvent_front && t.show_borders);
        assert!(g.style.fill.is_none() && g.style.effects.is_plain());
        for lane in &t.lanes {
            let s = &lane.spots[0];
            assert_eq!(s.rf, 0.15);
            assert_eq!(s.width_pt, 325095. / 65536.);
            assert!(s.style.fill.is_some());
            assert!(!s.show_rf);
        }
        assert_eq!(g.axis_y, Point::new(0., 400.)); // remembered square constraint cannot squash the plate
    }
}
