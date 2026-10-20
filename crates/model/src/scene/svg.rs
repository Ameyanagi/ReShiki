//! SVG serialization of the scene primitives.

use super::{Document, Primitive, STYLE, bounds, primitives};

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

pub(super) fn render_svg(doc: &Document, background: bool) -> String {
    let drawing = primitives(doc);
    let (lo, hi) = bounds(&drawing);
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}pt\" height=\"{}pt\">\n",
        lo.x,
        lo.y,
        hi.x - lo.x,
        hi.y - lo.y,
        (hi.x - lo.x) * STYLE.points_per_world(),
        (hi.y - lo.y) * STYLE.points_per_world()
    );
    let theme = doc.canvas_theme;
    if background {
        let [r, g, b] = theme.background();
        s.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"rgb({r},{g},{b})\"/>\n",
            lo.x,
            lo.y,
            hi.x - lo.x,
            hi.y - lo.y
        ));
    }
    let frame = Frame {
        theme,
        paper: background.then(|| theme.background()),
        lo,
        hi,
        gradients: Default::default(),
    };
    let mut content = String::new();
    push_svg(&mut content, drawing, &frame, &mut 0);
    s.push_str(&content);
    s.push_str("</svg>\n");
    s
}

/// `drawing` alone over `lo..hi`, one SVG unit per drawing unit, in the light
/// theme and without a background.
pub(super) fn render_part(drawing: Vec<Primitive>, lo: super::Point, hi: super::Point) -> String {
    let (width, height) = (hi.x - lo.x, hi.y - lo.y);
    let frame = Frame {
        theme: Default::default(),
        paper: None,
        lo,
        hi,
        gradients: Default::default(),
    };
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {width} {height}\" width=\"{width}\" height=\"{height}\">",
        lo.x, lo.y
    );
    push_svg(&mut s, drawing, &frame, &mut 0);
    s.push_str("</svg>");
    s
}

struct Frame {
    theme: crate::canvas_theme::CanvasTheme,
    /// The painted background; `None` leaves the surround transparent.
    paper: Option<[u8; 3]>,
    lo: super::Point,
    hi: super::Point,
    /// Serial for 3D-model gradient IDs.
    gradients: std::cell::Cell<usize>,
}

/// A gradient definition in drawing units with exact stop colors, and its ID.
fn gradient(
    frame: &Frame,
    kind: &str,
    geometry: &str,
    stops: &[(f32, [u8; 3])],
) -> (String, String) {
    frame.gradients.set(frame.gradients.get() + 1);
    let id = format!("reshiki-3d-{}", frame.gradients.get());
    let stops: String = stops
        .iter()
        .map(|(offset, [r, g, b])| {
            format!("<stop offset=\"{offset}\" stop-color=\"rgb({r},{g},{b})\"/>")
        })
        .collect();
    (
        format!(
            "<defs><{kind} id=\"{id}\" gradientUnits=\"userSpaceOnUse\" {geometry}>{stops}</{kind}></defs>"
        ),
        id,
    )
}

fn outline_paint(outline: Option<super::Outline>) -> String {
    outline.map_or_else(String::new, |o| {
        let [r, g, b] = o.color;
        format!(
            " stroke=\"rgb({r},{g},{b})\" stroke-opacity=\"{}\" stroke-width=\"{}\"",
            o.alpha, o.width
        )
    })
}

fn push_svg(s: &mut String, drawing: Vec<Primitive>, frame: &Frame, halos: &mut usize) {
    let theme = frame.theme;
    let [r, g, b] = theme.color([0; 3]);
    let ink = format!("rgb({r},{g},{b})");
    for p in drawing {
        match p {
            Primitive::Opacity { alpha, primitive } => {
                let mut inner = String::new();
                push_svg(&mut inner, vec![*primitive], frame, halos);
                s.push_str(&format!("<g opacity=\"{alpha}\">{inner}</g>"));
            }
            Primitive::Halo { radius, primitives } => {
                if let Some([r, g, b]) = frame.paper {
                    push_halo(s, &primitives, radius, &format!("rgb({r},{g},{b})"));
                } else {
                    // Knock the halo out of everything drawn so far, so the
                    // destination shows through exactly where paper would.
                    *halos += 1;
                    let id = format!("reshiki-label-halo-{halos}");
                    let (lo, hi) = (frame.lo, frame.hi);
                    let mut mask = format!(
                        "<mask id=\"{id}\" maskUnits=\"userSpaceOnUse\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\"><rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"white\"/><g id=\"{id}-ink\">",
                        lo.x,
                        lo.y,
                        hi.x - lo.x,
                        hi.y - lo.y,
                        lo.x,
                        lo.y,
                        hi.x - lo.x,
                        hi.y - lo.y
                    );
                    push_halo(&mut mask, &primitives, radius, "black");
                    mask.push_str("</g></mask>");
                    let earlier = std::mem::take(s);
                    s.push_str(&format!("{mask}<g mask=\"url(#{id})\">{earlier}</g>"));
                }
            }
            Primitive::Picture(g) => {
                use base64::Engine as _;
                if let Some(picture) = &g.picture {
                    let data = base64::engine::general_purpose::STANDARD.encode(picture.png());
                    s.push_str(&format!("<image id=\"reshiki-picture-{}\" width=\"1\" height=\"1\" preserveAspectRatio=\"none\" transform=\"matrix({} {} {} {} {} {})\" href=\"data:image/png;base64,{}\"/>",g.id,g.axis_x.x,g.axis_x.y,g.axis_y.x,g.axis_y.y,g.origin.x,g.origin.y,data));
                }
            }
            Primitive::Path {
                commands,
                style,
                filled,
            } => {
                let stroke = theme.color(style.stroke.rgb());
                let fill = style.fill.map(|color| theme.color(color.rgb()));
                let path = path_data(&commands);
                let fill = if filled {
                    fill.map(|c| format!("rgb({},{},{})", c[0], c[1], c[2]))
                        .unwrap_or_else(|| "none".into())
                } else {
                    "none".into()
                };
                let dashes = style
                    .dashes()
                    .iter()
                    .map(f32::to_string)
                    .collect::<Vec<_>>()
                    .join(" ");
                let dash = if dashes.is_empty() {
                    String::new()
                } else {
                    format!(" stroke-dasharray=\"{dashes}\"")
                };
                s.push_str(&format!("<path d=\"{path}\" fill=\"{fill}\" stroke=\"rgb({},{},{})\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"{dash}/>",stroke[0],stroke[1],stroke[2],style.width()));
            }
            Primitive::Line(a, b, w) => {
                s.push_str(&format!("<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"{ink}\" stroke-width=\"{w}\" stroke-linecap=\"round\"/>",a.x,a.y,b.x,b.y));
            }
            Primitive::Polygon(points) => {
                s.push_str(&format!(
                    "<polygon points=\"{}\" fill=\"{ink}\"/>",
                    points
                        .iter()
                        .map(|p| format!("{},{}", p.x, p.y))
                        .collect::<Vec<_>>()
                        .join(" ")
                ));
            }
            Primitive::Sphere {
                center,
                radius,
                focal,
                stops,
                outline,
            } => {
                // `fr` is SVG 2: the vendored usvg 0.45 behind PNG and PDF
                // export ignores it and starts the gradient at radius 0.
                let (defs, id) = gradient(
                    frame,
                    "radialGradient",
                    &format!(
                        "cx=\"{}\" cy=\"{}\" r=\"{radius}\" fx=\"{}\" fy=\"{}\" fr=\"{}\"",
                        center.x,
                        center.y,
                        focal.x,
                        focal.y,
                        radius * crate::models3d::FOCAL_RADIUS
                    ),
                    &stops,
                );
                s.push_str(&format!(
                    "{defs}<circle cx=\"{}\" cy=\"{}\" r=\"{radius}\" fill=\"url(#{id})\"{}/>",
                    center.x,
                    center.y,
                    outline_paint(outline)
                ));
            }
            Primitive::ShadedQuad {
                corners,
                from,
                to,
                stops,
                outline,
            } => {
                let (defs, id) = gradient(
                    frame,
                    "linearGradient",
                    &format!(
                        "x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\"",
                        from.x, from.y, to.x, to.y
                    ),
                    &stops,
                );
                let [a, b, c, d] = corners;
                s.push_str(&format!(
                    "{defs}<polygon points=\"{},{} {},{} {},{} {},{}\" fill=\"url(#{id})\"/>",
                    a.x, a.y, b.x, b.y, c.x, c.y, d.x, d.y
                ));
                if outline.is_some() {
                    s.push_str(&format!(
                        "<path d=\"M{} {} L{} {} M{} {} L{} {}\" fill=\"none\"{}/>",
                        a.x,
                        a.y,
                        b.x,
                        b.y,
                        d.x,
                        d.y,
                        c.x,
                        c.y,
                        outline_paint(outline)
                    ));
                }
            }
            Primitive::Wire {
                from,
                to,
                width,
                color: [r, g, b],
            } => s.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"rgb({r},{g},{b})\" stroke-width=\"{width}\" stroke-linecap=\"round\"/>",
                from.x, from.y, to.x, to.y
            )),
            Primitive::Card {
                lo,
                hi,
                radius,
                fill: [r, g, b],
                outline,
            } => s.push_str(&format!(
                "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"{radius}\" fill=\"rgb({r},{g},{b})\"{}/>",
                lo.x,
                lo.y,
                hi.x - lo.x,
                hi.y - lo.y,
                outline_paint(Some(outline))
            )),
            Primitive::Text {
                position,
                text,
                size,
                color,
                style,
            } => {
                let [r, g, b] = theme.color(color);
                push_text(
                    s,
                    position,
                    &text,
                    size,
                    &style,
                    &format!("fill=\"rgb({r},{g},{b})\""),
                );
            }
        }
    }
}

fn path_data(commands: &[crate::graphics::PathCommand]) -> String {
    use crate::graphics::PathCommand;
    let mut path = String::new();
    for c in commands {
        match c {
            PathCommand::Move(p) => path.push_str(&format!("M{} {} ", p.x, p.y)),
            PathCommand::Line(p) => path.push_str(&format!("L{} {} ", p.x, p.y)),
            PathCommand::Cubic(a, b, c) => path.push_str(&format!(
                "C{} {} {} {} {} {} ",
                a.x, a.y, b.x, b.y, c.x, c.y
            )),
            PathCommand::Close => path.push_str("Z "),
        }
    }
    path
}

fn push_text(
    s: &mut String,
    position: super::Point,
    text: &str,
    size: f32,
    style: &crate::typography::TextStyle,
    paint: &str,
) {
    s.push_str(&format!("<text xml:space=\"preserve\" x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{size}\" font-weight=\"{}\" font-style=\"{}\" text-decoration=\"{}\" {paint} dominant-baseline=\"text-before-edge\">{}</text>",position.x,position.y,escape(&style.family),if style.bold {"bold"} else {"normal"},if style.italic {"italic"} else {"normal"},if style.underline {"underline"} else {"none"},escape(text)));
}

/// The halo shapes: each label shape filled and stroked `radius` wider in `color`.
fn push_halo(s: &mut String, primitives: &[Primitive], radius: f32, color: &str) {
    let spread = |width: f32| {
        format!(
            "stroke=\"{color}\" stroke-width=\"{}\" stroke-linecap=\"round\" stroke-linejoin=\"round\"",
            width + 2. * radius
        )
    };
    for p in primitives {
        match p {
            Primitive::Opacity { primitive, .. } => {
                push_halo(s, std::slice::from_ref(primitive.as_ref()), radius, color)
            }
            Primitive::Path {
                commands,
                style,
                filled,
            } => {
                let fill = if *filled && style.fill.is_some() {
                    color
                } else {
                    "none"
                };
                let width = if style.width_pt > 0. {
                    style.width()
                } else {
                    0.
                };
                s.push_str(&format!(
                    "<path d=\"{}\" fill=\"{fill}\" {}/>",
                    path_data(commands),
                    spread(width)
                ));
            }
            Primitive::Line(a, b, width) => s.push_str(&format!(
                "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" {}/>",
                a.x,
                a.y,
                b.x,
                b.y,
                spread(*width)
            )),
            Primitive::Polygon(points) => s.push_str(&format!(
                "<polygon points=\"{}\" fill=\"{color}\" {}/>",
                points
                    .iter()
                    .map(|p| format!("{},{}", p.x, p.y))
                    .collect::<Vec<_>>()
                    .join(" "),
                spread(0.)
            )),
            Primitive::Text {
                position,
                text,
                size,
                style,
                ..
            } => push_text(
                s,
                *position,
                text,
                *size,
                style,
                &format!("fill=\"{color}\" {}", spread(0.)),
            ),
            Primitive::Picture(_)
            | Primitive::Halo { .. }
            | Primitive::Sphere { .. }
            | Primitive::ShadedQuad { .. }
            | Primitive::Wire { .. }
            | Primitive::Card { .. } => {}
        }
    }
}
