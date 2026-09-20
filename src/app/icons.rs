use crate::canvas::Tool;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};

#[derive(Clone, Copy)]
pub(super) enum Icon {
    TextAlign(moruno::typography::TextAlign),
    Tool(Tool),
    New,
    Open,
    Save,
    Undo,
    Redo,
    Import,
    Export,
    Inspector,
    Close,
}
pub(super) struct Glyph(pub Icon, pub bool);
impl<Message> canvas::Program<Message> for Glyph {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        _: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut f = Frame::new(renderer, bounds.size());
        let ink = if self.1 {
            Color::from_rgb8(51, 62, 72)
        } else {
            Color::from_rgb8(187, 193, 199)
        };
        let line = |f: &mut Frame, points: &[(f32, f32)]| {
            let path = Path::new(|p| {
                if let Some((x, y)) = points.first() {
                    p.move_to(Point::new(*x, *y));
                }
                for (x, y) in points.iter().skip(1) {
                    p.line_to(Point::new(*x, *y));
                }
            });
            f.stroke(&path, Stroke::default().with_width(1.6).with_color(ink));
        };
        let polygon = |f: &mut Frame, points: &[(f32, f32)]| {
            let path = Path::new(|p| {
                if let Some((x, y)) = points.first() {
                    p.move_to(Point::new(*x, *y));
                }
                for (x, y) in points.iter().skip(1) {
                    p.line_to(Point::new(*x, *y));
                }
                p.close();
            });
            f.fill(&path, ink);
        };
        match self.0 {
            Icon::Tool(Tool::Chain(mode)) => {
                if mode == moruno::chains::ChainMode::Straight {
                    line(
                        &mut f,
                        &[(2., 15.), (7., 8.), (12., 15.), (17., 8.), (22., 15.)],
                    );
                } else {
                    line(
                        &mut f,
                        &[
                            (2., 19.),
                            (7., 13.),
                            (5., 6.),
                            (12., 3.),
                            (17., 9.),
                            (23., 7.),
                        ],
                    );
                }
            }
            Icon::Tool(Tool::Graphic(kind)) => {
                use moruno::{
                    document::Point as World,
                    graphics::{BracketSides, Graphic, GraphicKind, GraphicStyle, PathCommand},
                };
                let (start, end) = match kind {
                    GraphicKind::Orbital(_) => (World::new(12., 12.), World::new(12., 2.)),
                    GraphicKind::Symbol(_) => (World::new(12., 12.), World::new(28., 12.)),
                    _ => (World::new(4., 5.), World::new(21., 20.)),
                };
                let g = Graphic::dragged(
                    1,
                    kind,
                    start,
                    end,
                    GraphicStyle::default(),
                    BracketSides::Both,
                    false,
                );
                let path = Path::new(|b| {
                    for c in g.commands() {
                        match c {
                            PathCommand::Move(p) => b.move_to(Point::new(p.x, p.y)),
                            PathCommand::Line(p) => b.line_to(Point::new(p.x, p.y)),
                            PathCommand::Cubic(a, z, p) => b.bezier_curve_to(
                                Point::new(a.x, a.y),
                                Point::new(z.x, z.y),
                                Point::new(p.x, p.y),
                            ),
                            PathCommand::Close => b.close(),
                        }
                    }
                });
                f.stroke(&path, Stroke::default().with_color(ink).with_width(1.5));
            }
            Icon::Tool(Tool::EditPoints) => {
                line(&mut f, &[(4., 19.), (9., 5.), (20., 12.)]);
                for p in [
                    Point::new(4., 19.),
                    Point::new(9., 5.),
                    Point::new(20., 12.),
                ] {
                    f.fill(&Path::circle(p, 2.5), ink);
                }
            }
            Icon::TextAlign(alignment) => {
                for i in 0..4 {
                    let width =
                        if i % 2 == 0 || alignment == moruno::typography::TextAlign::Justified {
                            16.0
                        } else {
                            10.0
                        };
                    let x = match alignment {
                        moruno::typography::TextAlign::Right => 20.0 - width,
                        moruno::typography::TextAlign::Center => (24.0 - width) / 2.0,
                        _ => 4.0,
                    };
                    line(
                        &mut f,
                        &[(x, 5.0 + i as f32 * 4.5), (x + width, 5.0 + i as f32 * 4.5)],
                    );
                }
            }
            Icon::Tool(Tool::Select) => {
                line(
                    &mut f,
                    &[
                        (5., 3.),
                        (5., 20.),
                        (10., 15.),
                        (14., 22.),
                        (17., 20.),
                        (13., 13.),
                        (21., 12.),
                        (5., 3.),
                    ],
                );
            }
            Icon::Tool(Tool::Lasso) => {
                let path = Path::new(|b| {
                    b.move_to(Point::new(6., 17.));
                    b.bezier_curve_to(
                        Point::new(-1., 12.),
                        Point::new(8., 1.),
                        Point::new(16., 4.),
                    );
                    b.bezier_curve_to(
                        Point::new(27., 8.),
                        Point::new(17., 23.),
                        Point::new(7., 17.),
                    );
                    b.bezier_curve_to(
                        Point::new(1., 13.),
                        Point::new(1., 23.),
                        Point::new(9., 22.),
                    );
                });
                f.stroke(&path, Stroke::default().with_width(1.6).with_color(ink));
            }
            Icon::Tool(Tool::Bond(order)) => {
                let offsets: &[f32] = match order {
                    2 => &[-2., 2.],
                    3 => &[-4., 0., 4.],
                    _ => &[0.],
                };
                for dy in offsets {
                    line(&mut f, &[(4., 18. + dy), (20., 6. + dy)]);
                }
            }
            Icon::Tool(Tool::StyledBond(preset)) => {
                use moruno::bonds::BondPreset as P;
                match preset {
                    P::Dative => {
                        line(&mut f, &[(3., 19.), (20., 5.), (13., 6.)]);
                        line(&mut f, &[(20., 5.), (18., 12.)]);
                    }
                    P::Quadruple => {
                        for dy in [-4.5, -1.5, 1.5, 4.5] {
                            line(&mut f, &[(4., 17. + dy), (20., 7. + dy)]);
                        }
                    }
                    P::HollowWedge => line(&mut f, &[(4., 19.), (17., 3.), (22., 10.), (4., 19.)]),
                    P::Bold => f.stroke(
                        &Path::line(Point::new(4., 19.), Point::new(20., 5.)),
                        Stroke::default().with_width(4.).with_color(ink),
                    ),
                    P::Dotted => {
                        for i in 0..6 {
                            let t = i as f32 / 5.;
                            f.fill(
                                &Path::circle(Point::new(4. + 16. * t, 19. - 14. * t), 1.1),
                                ink,
                            );
                        }
                    }
                    P::Dashed => {
                        for i in 0..4 {
                            let t = i as f32 / 4.;
                            line(
                                &mut f,
                                &[
                                    (4. + 16. * t, 19. - 14. * t),
                                    (4. + 16. * (t + 0.13), 19. - 14. * (t + 0.13)),
                                ],
                            );
                        }
                    }
                    P::Hashed => {
                        for i in 0..6 {
                            let t = i as f32 / 5.;
                            let x = 5. + 14. * t;
                            let y = 19. - 13. * t;
                            line(&mut f, &[(x - 2.5, y - 2.5), (x + 2.5, y + 2.5)]);
                        }
                    }
                    P::CrossedDouble => {
                        line(&mut f, &[(4., 19.), (20., 5.)]);
                        line(&mut f, &[(4., 15.), (20., 9.)]);
                    }
                    _ => {
                        line(&mut f, &[(4., 19.), (20., 7.)]);
                        line(&mut f, &[(4., 15.), (20., 3.)]);
                    }
                }
            }
            Icon::Tool(Tool::Wedge) => polygon(&mut f, &[(4., 19.), (17., 3.), (22., 10.)]),
            Icon::Tool(Tool::Hash) => {
                for i in 0..6 {
                    let t = i as f32 / 5.;
                    let x = 5. + 14. * t;
                    let y = 19. - 13. * t;
                    line(
                        &mut f,
                        &[(x - 3. * t, y - 3. * t), (x + 3. * t, y + 3. * t)],
                    );
                }
            }
            Icon::Tool(Tool::Wavy) => {
                let points: Vec<_> = (0..=32)
                    .map(|i| {
                        let t = i as f32 / 32.;
                        (
                            3. + 18. * t,
                            12. + 3. * (t * std::f32::consts::TAU * 3.).sin(),
                        )
                    })
                    .collect();
                line(&mut f, &points);
            }
            Icon::Tool(Tool::RingPreset(preset)) => {
                let doc = preset.document(7., false);
                for bond in &doc.bonds {
                    let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
                        continue;
                    };
                    let (a, b) = (a.position, b.position);
                    line(&mut f, &[(12. + a.x, 12. + a.y), (12. + b.x, 12. + b.y)]);
                    if bond.order == 2 {
                        line(
                            &mut f,
                            &[
                                (12. + a.x * 0.68, 12. + a.y * 0.68),
                                (12. + b.x * 0.68, 12. + b.y * 0.68),
                            ],
                        );
                    }
                }
            }
            Icon::Tool(Tool::Ring | Tool::Template) => {
                let points: Vec<_> = (0..=6)
                    .map(|i| {
                        let a = i as f32 * std::f32::consts::TAU / 6.;
                        (12. + 9. * a.cos(), 12. + 9. * a.sin())
                    })
                    .collect();
                line(&mut f, &points);
            }
            Icon::Tool(Tool::Arrow) => {
                line(&mut f, &[(3., 12.), (21., 12.)]);
                line(&mut f, &[(15., 6.), (21., 12.), (15., 18.)]);
            }
            Icon::Tool(Tool::Text) => {
                line(&mut f, &[(4., 7.), (4., 4.), (20., 4.), (20., 7.)]);
                line(&mut f, &[(12., 4.), (12., 21.)]);
                line(&mut f, &[(8., 21.), (16., 21.)]);
            }
            Icon::Tool(Tool::Atom) => {
                f.fill_text(canvas::Text {
                    content: "C".into(),
                    position: Point::new(5., 1.),
                    size: 20.into(),
                    color: ink,
                    ..Default::default()
                });
            }
            Icon::Tool(Tool::Erase) => {
                line(
                    &mut f,
                    &[
                        (3., 15.),
                        (14., 4.),
                        (22., 12.),
                        (13., 21.),
                        (9., 21.),
                        (3., 15.),
                    ],
                );
                line(&mut f, &[(8., 10.), (16., 18.)]);
            }
            Icon::New => {
                line(
                    &mut f,
                    &[
                        (6., 3.),
                        (16., 3.),
                        (20., 7.),
                        (20., 21.),
                        (6., 21.),
                        (6., 3.),
                    ],
                );
                line(&mut f, &[(16., 3.), (16., 7.), (20., 7.)]);
                line(&mut f, &[(10., 13.), (16., 13.)]);
                line(&mut f, &[(13., 10.), (13., 16.)]);
            }
            Icon::Open => line(
                &mut f,
                &[
                    (3., 20.),
                    (3., 5.),
                    (10., 5.),
                    (13., 8.),
                    (20., 8.),
                    (20., 11.),
                    (6., 11.),
                    (3., 20.),
                    (19., 20.),
                    (22., 11.),
                    (20., 11.),
                ],
            ),
            Icon::Save => {
                line(
                    &mut f,
                    &[
                        (4., 3.),
                        (18., 3.),
                        (21., 6.),
                        (21., 21.),
                        (4., 21.),
                        (4., 3.),
                    ],
                );
                line(&mut f, &[(8., 3.), (8., 9.), (17., 9.), (17., 3.)]);
                line(&mut f, &[(8., 21.), (8., 14.), (17., 14.), (17., 21.)]);
            }
            Icon::Undo | Icon::Redo => {
                let points = [
                    (5., 9.),
                    (15., 9.),
                    (19., 13.),
                    (19., 17.),
                    (15., 21.),
                    (9., 21.),
                ];
                let flip = |p: (f32, f32)| {
                    if matches!(self.0, Icon::Redo) {
                        (24. - p.0, p.1)
                    } else {
                        p
                    }
                };
                line(&mut f, &points.map(flip));
                line(&mut f, &[(10., 4.), (5., 9.), (10., 14.)].map(flip));
            }
            Icon::Import | Icon::Export => {
                line(&mut f, &[(4., 15.), (4., 21.), (20., 21.), (20., 15.)]);
                let points = if matches!(self.0, Icon::Import) {
                    [(7., 10.), (12., 15.), (17., 10.)]
                } else {
                    [(7., 8.), (12., 3.), (17., 8.)]
                };
                line(&mut f, &points);
                line(&mut f, &[(12., 3.), (12., 15.)]);
            }
            Icon::Inspector => {
                line(
                    &mut f,
                    &[(3., 4.), (21., 4.), (21., 20.), (3., 20.), (3., 4.)],
                );
                line(&mut f, &[(15., 4.), (15., 20.)]);
            }
            Icon::Close => {
                line(&mut f, &[(6., 6.), (18., 18.)]);
                line(&mut f, &[(18., 6.), (6., 18.)]);
            }
        }
        vec![f.into_geometry()]
    }
}
