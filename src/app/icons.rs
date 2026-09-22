use crate::canvas::Tool;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};

#[derive(Clone, Copy)]
pub(super) enum Icon {
    TextAlign(reshiki::typography::TextAlign),
    Tool(Tool),
    Ring(u8, bool),
    Arrow(reshiki::arrows::Preset),
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
        self.paint(&mut f);
        vec![f.into_geometry()]
    }
}
impl Glyph {
    pub(super) fn paint(&self, f: &mut Frame) {
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
                if mode == reshiki::chains::ChainMode::Straight {
                    line(f, &[(2., 15.), (7., 8.), (12., 15.), (17., 8.), (22., 15.)]);
                } else {
                    line(
                        f,
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
                use reshiki::{
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
                line(f, &[(4., 19.), (9., 5.), (20., 12.)]);
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
                        if i % 2 == 0 || alignment == reshiki::typography::TextAlign::Justified {
                            16.0
                        } else {
                            10.0
                        };
                    let x = match alignment {
                        reshiki::typography::TextAlign::Right => 20.0 - width,
                        reshiki::typography::TextAlign::Center => (24.0 - width) / 2.0,
                        _ => 4.0,
                    };
                    line(
                        f,
                        &[(x, 5.0 + i as f32 * 4.5), (x + width, 5.0 + i as f32 * 4.5)],
                    );
                }
            }
            Icon::Tool(Tool::Select) => {
                line(
                    f,
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
                    line(f, &[(4., 18. + dy), (20., 6. + dy)]);
                }
            }
            Icon::Tool(Tool::StyledBond(preset)) => {
                use reshiki::bonds::BondPreset as P;
                match preset {
                    P::Dative => {
                        line(f, &[(3., 19.), (20., 5.), (13., 6.)]);
                        line(f, &[(20., 5.), (18., 12.)]);
                    }
                    P::Quadruple => {
                        for dy in [-4.5, -1.5, 1.5, 4.5] {
                            line(f, &[(4., 17. + dy), (20., 7. + dy)]);
                        }
                    }
                    P::HollowWedge => line(f, &[(4., 19.), (17., 3.), (22., 10.), (4., 19.)]),
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
                                f,
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
                            line(f, &[(x - 2.5, y - 2.5), (x + 2.5, y + 2.5)]);
                        }
                    }
                    P::CrossedDouble => {
                        line(f, &[(4., 19.), (20., 5.)]);
                        line(f, &[(4., 15.), (20., 9.)]);
                    }
                    _ => {
                        line(f, &[(4., 19.), (20., 7.)]);
                        line(f, &[(4., 15.), (20., 3.)]);
                    }
                }
            }
            Icon::Tool(Tool::Wedge) => polygon(f, &[(4., 19.), (17., 3.), (22., 10.)]),
            Icon::Tool(Tool::Hash) => {
                for i in 0..6 {
                    let t = i as f32 / 5.;
                    let x = 5. + 14. * t;
                    let y = 19. - 13. * t;
                    line(f, &[(x - 3. * t, y - 3. * t), (x + 3. * t, y + 3. * t)]);
                }
            }
            Icon::Tool(Tool::Wavy) => {
                use reshiki::{document::Point as World, graphics::PathCommand};
                let path = Path::new(|p| {
                    for command in
                        reshiki::bonds::wavy_path(World::new(3., 12.), World::new(21., 12.), 6., 3.)
                    {
                        match command {
                            PathCommand::Move(a) => p.move_to(Point::new(a.x, a.y)),
                            PathCommand::Cubic(a, b, c) => p.bezier_curve_to(
                                Point::new(a.x, a.y),
                                Point::new(b.x, b.y),
                                Point::new(c.x, c.y),
                            ),
                            _ => {}
                        }
                    }
                });
                f.stroke(&path, Stroke::default().with_width(1.6).with_color(ink));
            }
            Icon::Tool(Tool::RingPreset(preset)) => {
                let doc = preset.document(7., false);
                for bond in &doc.bonds {
                    let (Some(a), Some(b)) = (doc.atom(bond.a), doc.atom(bond.b)) else {
                        continue;
                    };
                    let (a, b) = (a.position, b.position);
                    line(f, &[(12. + a.x, 12. + a.y), (12. + b.x, 12. + b.y)]);
                    if bond.order == 2 {
                        line(
                            f,
                            &[
                                (12. + a.x * 0.68, 12. + a.y * 0.68),
                                (12. + b.x * 0.68, 12. + b.y * 0.68),
                            ],
                        );
                    }
                }
            }
            Icon::Ring(_, _) | Icon::Tool(Tool::Ring | Tool::Template) => {
                let (size, aromatic) = match self.0 {
                    Icon::Ring(size, aromatic) => (size, aromatic),
                    _ => (6, false),
                };
                let points: Vec<_> = (0..=size)
                    .map(|i| {
                        let a = i as f32 * std::f32::consts::TAU / size as f32;
                        (12. + 9. * a.cos(), 12. + 9. * a.sin())
                    })
                    .collect();
                line(f, &points);
                if aromatic {
                    f.stroke(
                        &Path::circle(Point::new(12., 12.), 5.7),
                        Stroke::default().with_width(1.4).with_color(ink),
                    );
                }
            }
            Icon::Arrow(_) | Icon::Tool(Tool::Arrow) => {
                use reshiki::{
                    arrows::{ArrowStyle, Preset},
                    document::{Arrow, Point as World},
                    graphics::PathCommand,
                };
                let preset = match self.0 {
                    Icon::Arrow(preset) => preset,
                    _ => Preset::Forward,
                };
                if preset == Preset::Forward {
                    line(f, &[(3., 12.), (21., 12.)]);
                    line(f, &[(15., 6.), (21., 12.), (15., 18.)]);
                    return;
                }
                let style = ArrowStyle {
                    head_length_pt: 8.,
                    head_width_pt: 4.,
                    gap_pt: 3.2,
                    ..ArrowStyle::preset(preset)
                };
                let arrow = Arrow::new(1, World::default(), World::new(80., 0.), preset, style);
                let (lo, hi) = arrow.bounds();
                let scale = (20. / (hi.x - lo.x).max(1.)).min(18. / (hi.y - lo.y).max(1.));
                let point = |p: World| {
                    Point::new(
                        12. + (p.x - (lo.x + hi.x) / 2.) * scale,
                        12. + (p.y - (lo.y + hi.y) / 2.) * scale,
                    )
                };
                for part in arrow.paths() {
                    let path = Path::new(|b| {
                        for command in part.commands {
                            match command {
                                PathCommand::Move(p) => b.move_to(point(p)),
                                PathCommand::Line(p) => b.line_to(point(p)),
                                PathCommand::Cubic(a, z, p) => {
                                    b.bezier_curve_to(point(a), point(z), point(p))
                                }
                                PathCommand::Close => b.close(),
                            }
                        }
                    });
                    if part.filled {
                        f.fill(&path, ink);
                    } else {
                        f.stroke(&path, Stroke::default().with_width(1.4).with_color(ink));
                    }
                }
            }
            Icon::Tool(Tool::Text) => {
                line(f, &[(4., 7.), (4., 4.), (20., 4.), (20., 7.)]);
                line(f, &[(12., 4.), (12., 21.)]);
                line(f, &[(8., 21.), (16., 21.)]);
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
                    f,
                    &[
                        (3., 15.),
                        (14., 4.),
                        (22., 12.),
                        (13., 21.),
                        (9., 21.),
                        (3., 15.),
                    ],
                );
                line(f, &[(8., 10.), (16., 18.)]);
            }
            Icon::New => {
                line(
                    f,
                    &[
                        (6., 3.),
                        (16., 3.),
                        (20., 7.),
                        (20., 21.),
                        (6., 21.),
                        (6., 3.),
                    ],
                );
                line(f, &[(16., 3.), (16., 7.), (20., 7.)]);
                line(f, &[(10., 13.), (16., 13.)]);
                line(f, &[(13., 10.), (13., 16.)]);
            }
            Icon::Open => line(
                f,
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
                    f,
                    &[
                        (4., 3.),
                        (18., 3.),
                        (21., 6.),
                        (21., 21.),
                        (4., 21.),
                        (4., 3.),
                    ],
                );
                line(f, &[(8., 3.), (8., 9.), (17., 9.), (17., 3.)]);
                line(f, &[(8., 21.), (8., 14.), (17., 14.), (17., 21.)]);
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
                line(f, &points.map(flip));
                line(f, &[(10., 4.), (5., 9.), (10., 14.)].map(flip));
            }
            Icon::Import | Icon::Export => {
                line(f, &[(4., 15.), (4., 21.), (20., 21.), (20., 15.)]);
                let points = if matches!(self.0, Icon::Import) {
                    [(7., 10.), (12., 15.), (17., 10.)]
                } else {
                    [(7., 8.), (12., 3.), (17., 8.)]
                };
                line(f, &points);
                line(f, &[(12., 3.), (12., 15.)]);
            }
            Icon::Inspector => {
                line(f, &[(3., 4.), (21., 4.), (21., 20.), (3., 20.), (3., 4.)]);
                line(f, &[(15., 4.), (15., 20.)]);
            }
            Icon::Close => {
                line(f, &[(6., 6.), (18., 18.)]);
                line(f, &[(18., 6.), (6., 18.)]);
            }
        }
    }
}
