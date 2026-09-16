use crate::canvas::Tool;
use iced::widget::canvas::{self, Frame, Geometry, Path, Stroke};
use iced::{Color, Point, Rectangle, Renderer, Theme, mouse};

#[derive(Clone, Copy)]
pub(super) enum Icon {
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
            Icon::Tool(Tool::Ring) => {
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
