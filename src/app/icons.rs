use crate::canvas::Tool;
use iced::widget::canvas::{self, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};
mod chrome;
mod commands;

#[derive(Clone, Copy)]
pub(super) enum Icon {
    Sun,
    Moon,
    ColorTiles(bool),
    TextAlign(reshiki::typography::TextAlign),
    Tool(Tool),
    Ring(u8, bool),
    Arrow(reshiki::arrows::Preset),
    New,
    Open,
    Save,
    SaveAs,
    Assistant(bool),
    Check,
    Cleanup,
    Trash,
    Undo,
    Redo,
    Import,
    Export,
    Inspector,
    Keyboard,
    More,
    Lock(bool),
}
pub(super) struct Glyph(pub Icon, pub bool);
impl<Message> canvas::Program<Message> for Glyph {
    type State = ();
    fn draw(
        &self,
        _: &(),
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        _: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut f = crate::canvas::layered::Frame::new(renderer, bounds.size()).with_theme(theme);
        self.paint(&mut f);
        f.finish()
    }
}
impl Glyph {
    pub(super) fn paint(&self, f: &mut crate::canvas::layered::Frame<'_>) {
        let ink = if self.1 {
            Color::from_rgb8(51, 62, 72)
        } else {
            Color::from_rgb8(187, 193, 199)
        };
        match self.0 {
            Icon::ColorTiles(soft) => chrome::color_tiles(f, ink, soft),
            Icon::Trash => commands::trash(f, ink),
            Icon::Sun => chrome::sun(f, ink),
            Icon::Moon => chrome::moon(f, ink),

            Icon::Keyboard => chrome::keyboard(f, ink),
            Icon::Tool(Tool::Chain(mode)) => {
                if mode == reshiki::chains::ChainMode::Straight {
                    line(
                        f,
                        ink,
                        &[(2., 15.), (7., 8.), (12., 15.), (17., 8.), (22., 15.)],
                    );
                } else {
                    line(
                        f,
                        ink,
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
                line(f, ink, &[(4., 19.), (9., 5.), (20., 12.)]);
                for p in [
                    Point::new(4., 19.),
                    Point::new(9., 5.),
                    Point::new(20., 12.),
                ] {
                    f.fill(&Path::circle(p, 2.5), ink);
                }
            }
            Icon::TextAlign(alignment) => chrome::text_align(f, ink, alignment),
            Icon::Tool(Tool::Select) => {
                line(
                    f,
                    ink,
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
            Icon::Tool(Tool::Tilt) => {
                // A foreshortened ring and curved rotation arrow.
                line(
                    f,
                    ink,
                    &[
                        (3., 13.),
                        (8., 8.),
                        (18., 8.),
                        (22., 13.),
                        (17., 18.),
                        (7., 18.),
                        (3., 13.),
                    ],
                );
                let arc = Path::new(|p| {
                    p.move_to(Point::new(5., 6.));
                    p.bezier_curve_to(Point::new(8., 0.), Point::new(19., 0.), Point::new(22., 6.));
                });
                f.stroke(&arc, Stroke::default().with_width(1.6).with_color(ink));
                line(f, ink, &[(17., 4.), (22., 6.), (22., 1.)]);
            }
            Icon::Tool(Tool::Bond(order)) => {
                let offsets: &[f32] = match order {
                    2 => &[-2., 2.],
                    3 => &[-4., 0., 4.],
                    _ => &[0.],
                };
                for dy in offsets {
                    line(f, ink, &[(4., 18. + dy), (20., 6. + dy)]);
                }
            }
            Icon::Tool(Tool::StyledBond(preset)) => {
                use reshiki::bonds::BondPreset as P;
                match preset {
                    P::Dative => {
                        line(f, ink, &[(3., 19.), (20., 5.), (13., 6.)]);
                        line(f, ink, &[(20., 5.), (18., 12.)]);
                    }
                    P::Quadruple => {
                        for dy in [-4.5, -1.5, 1.5, 4.5] {
                            line(f, ink, &[(4., 17. + dy), (20., 7. + dy)]);
                        }
                    }
                    P::HollowWedge => line(f, ink, &[(4., 19.), (17., 3.), (22., 10.), (4., 19.)]),
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
                                ink,
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
                            line(f, ink, &[(x - 2.5, y - 2.5), (x + 2.5, y + 2.5)]);
                        }
                    }
                    P::CrossedDouble => {
                        line(f, ink, &[(4., 19.), (20., 5.)]);
                        line(f, ink, &[(4., 15.), (20., 9.)]);
                    }
                    _ => {
                        line(f, ink, &[(4., 19.), (20., 7.)]);
                        line(f, ink, &[(4., 15.), (20., 3.)]);
                    }
                }
            }
            Icon::Tool(Tool::Wedge) => polygon(f, ink, &[(4., 19.), (17., 3.), (22., 10.)]),
            Icon::Tool(Tool::Hash) => {
                for i in 0..6 {
                    let t = i as f32 / 5.;
                    let x = 5. + 14. * t;
                    let y = 19. - 13. * t;
                    line(
                        f,
                        ink,
                        &[(x - 3. * t, y - 3. * t), (x + 3. * t, y + 3. * t)],
                    );
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
                    line(f, ink, &[(12. + a.x, 12. + a.y), (12. + b.x, 12. + b.y)]);
                    if bond.order == 2 {
                        line(
                            f,
                            ink,
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
                line(f, ink, &points);
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
                    line(f, ink, &[(3., 12.), (21., 12.)]);
                    line(f, ink, &[(15., 6.), (21., 12.), (15., 18.)]);
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
                line(f, ink, &[(4., 7.), (4., 4.), (20., 4.), (20., 7.)]);
                line(f, ink, &[(12., 4.), (12., 21.)]);
                line(f, ink, &[(8., 21.), (16., 21.)]);
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
                    ink,
                    &[
                        (3., 15.),
                        (14., 4.),
                        (22., 12.),
                        (13., 21.),
                        (9., 21.),
                        (3., 15.),
                    ],
                );
                line(f, ink, &[(8., 10.), (16., 18.)]);
            }
            Icon::New => commands::new_document(f, ink),
            Icon::Open => commands::open(f, ink),
            Icon::Save => commands::save(f, ink),
            Icon::SaveAs => commands::save_as(f, ink),
            Icon::Assistant(working) => commands::assistant(f, ink, working),
            Icon::Check => commands::check(f, ink),
            Icon::Cleanup => commands::cleanup(f, ink),
            Icon::Undo | Icon::Redo => commands::undo_redo(f, ink, self.0),
            Icon::Import | Icon::Export => commands::import_export(f, ink, self.0),
            Icon::Inspector => commands::inspector(f, ink),
            Icon::More => chrome::more(f, ink),
            Icon::Lock(locked) => chrome::lock(f, ink, locked),
        }
    }
}

fn line(f: &mut crate::canvas::layered::Frame<'_>, ink: Color, points: &[(f32, f32)]) {
    let path = Path::new(|p| {
        if let Some((x, y)) = points.first() {
            p.move_to(Point::new(*x, *y));
        }
        for (x, y) in points.iter().skip(1) {
            p.line_to(Point::new(*x, *y));
        }
    });
    f.stroke(&path, Stroke::default().with_width(1.6).with_color(ink));
}

fn polygon(f: &mut crate::canvas::layered::Frame<'_>, ink: Color, points: &[(f32, f32)]) {
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
}

#[cfg(test)]
mod pixel_tests;
