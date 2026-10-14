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
    push_svg(&mut s, drawing, theme);
    s.push_str("</svg>\n");
    s
}

fn push_svg(s: &mut String, drawing: Vec<Primitive>, theme: crate::canvas_theme::CanvasTheme) {
    let [r, g, b] = theme.color([0; 3]);
    let ink = format!("rgb({r},{g},{b})");
    for p in drawing {
        match p {
            Primitive::Opacity { alpha, primitive } => {
                s.push_str(&format!("<g opacity=\"{alpha}\">"));
                push_svg(s, vec![*primitive], theme);
                s.push_str("</g>");
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
            Primitive::Text {
                position,
                text,
                size,
                color,
                style,
            } => {
                let color = theme.color(color);
                s.push_str(&format!("<text xml:space=\"preserve\" x=\"{}\" y=\"{}\" font-family=\"{}\" font-size=\"{size}\" font-weight=\"{}\" font-style=\"{}\" text-decoration=\"{}\" fill=\"rgb({},{},{})\" dominant-baseline=\"text-before-edge\">{}</text>",position.x,position.y,escape(&style.family),if style.bold {"bold"} else {"normal"},if style.italic {"italic"} else {"normal"},if style.underline {"underline"} else {"none"},color[0],color[1],color[2],escape(&text)));
            }
        }
    }
}
