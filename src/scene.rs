use crate::document::{Atom, Document, Point};
use std::fmt::Write;

#[derive(Debug, Clone)]
pub enum Primitive {
    Line(Point, Point, f32),
    Polygon(Vec<Point>),
    Text {
        position: Point,
        text: String,
        size: f32,
        color: [u8; 3],
    },
}
fn color(element: &str) -> [u8; 3] {
    match element {
        "O" => [193, 60, 64],
        "N" => [55, 96, 173],
        "S" => [158, 119, 27],
        "F" | "Cl" | "Br" => [37, 128, 91],
        _ => [34, 49, 54],
    }
}
fn visible(a: &Atom, doc: &Document) -> bool {
    a.element != "C"
        || a.charge != 0
        || a.isotope != 0
        || !doc.bonds.iter().any(|b| b.a == a.id || b.b == a.id)
}
fn label(a: &Atom) -> String {
    let mut s = a.element.clone();
    if a.element != "C" && a.element != "H" && a.label_h > 0 {
        s.push('H');
        if a.label_h > 1 {
            for c in a.label_h.to_string().chars() {
                s.push(
                    "₀₁₂₃₄₅₆₇₈₉"
                        .chars()
                        .nth(c.to_digit(10).unwrap() as usize)
                        .unwrap(),
                );
            }
        }
    }
    s
}

pub fn primitives(doc: &Document) -> Vec<Primitive> {
    let mut out = vec![];
    for b in &doc.bonds {
        let (Some(a), Some(z)) = (doc.atom(b.a), doc.atom(b.b)) else {
            continue;
        };
        let length = a.position.distance(z.position);
        if length < 0.1 {
            continue;
        }
        let ux = (z.position.x - a.position.x) / length;
        let uy = (z.position.y - a.position.y) / length;
        let start = if visible(a, doc) {
            a.position.offset(ux * 8.0, uy * 8.0)
        } else {
            a.position
        };
        let end = if visible(z, doc) {
            z.position.offset(-ux * 8.0, -uy * 8.0)
        } else {
            z.position
        };
        let nx = -uy;
        let ny = ux;
        match b.display.as_str() {
            "wedge" => out.push(Primitive::Polygon(vec![
                start,
                end.offset(nx * 4.0, ny * 4.0),
                end.offset(-nx * 4.0, -ny * 4.0),
            ])),
            "hash" => {
                for i in 1..=8 {
                    let t = i as f32 / 9.0;
                    let p = Point::new(
                        start.x + (end.x - start.x) * t,
                        start.y + (end.y - start.y) * t,
                    );
                    out.push(Primitive::Line(
                        p.offset(nx * t * 4.0, ny * t * 4.0),
                        p.offset(-nx * t * 4.0, -ny * t * 4.0),
                        1.0,
                    ));
                }
            }
            "wavy" => {
                let mut prev = start;
                for i in 1..=32 {
                    let t = i as f32 / 32.0;
                    let offset = (t * std::f32::consts::TAU * 4.0).sin() * 2.0;
                    let p = Point::new(
                        start.x + (end.x - start.x) * t + nx * offset,
                        start.y + (end.y - start.y) * t + ny * offset,
                    );
                    out.push(Primitive::Line(prev, p, 1.2));
                    prev = p;
                }
            }
            _ => {
                let offsets: &[f32] = match b.order {
                    2 => &[-2.0, 2.0],
                    3 => &[-3.2, 0.0, 3.2],
                    _ => &[0.0],
                };
                for offset in offsets {
                    out.push(Primitive::Line(
                        start.offset(nx * offset, ny * offset),
                        end.offset(nx * offset, ny * offset),
                        1.35,
                    ));
                }
                if b.order == 4 {
                    for i in 0..5 {
                        let t = i as f32 / 5.0;
                        let v = (i as f32 + 0.5) / 5.0;
                        out.push(Primitive::Line(
                            Point::new(
                                start.x + (end.x - start.x) * t + nx * 3.5,
                                start.y + (end.y - start.y) * t + ny * 3.5,
                            ),
                            Point::new(
                                start.x + (end.x - start.x) * v + nx * 3.5,
                                start.y + (end.y - start.y) * v + ny * 3.5,
                            ),
                            1.0,
                        ));
                    }
                }
            }
        }
    }
    for a in &doc.atoms {
        if visible(a, doc) {
            out.push(Primitive::Text {
                position: a.position.offset(-4.0, -7.0),
                text: label(a),
                size: 13.0,
                color: color(&a.element),
            });
            if a.isotope > 0 {
                out.push(Primitive::Text {
                    position: a.position.offset(-14.0, -12.0),
                    text: a.isotope.to_string(),
                    size: 8.0,
                    color: color(&a.element),
                });
            }
            if a.charge != 0 {
                let amount = if a.charge.abs() > 1 {
                    a.charge.abs().to_string()
                } else {
                    String::new()
                };
                out.push(Primitive::Text {
                    position: a
                        .position
                        .offset(label(a).chars().count() as f32 * 7.0 - 3.0, -12.0),
                    text: format!("{amount}{}", if a.charge > 0 { "+" } else { "−" }),
                    size: 9.0,
                    color: color(&a.element),
                });
            }
        }
    }
    for a in &doc.arrows {
        out.push(Primitive::Line(a.start, a.end, 1.3));
        let angle = (a.end.y - a.start.y).atan2(a.end.x - a.start.x);
        out.push(Primitive::Polygon(vec![
            a.end,
            a.end.offset(
                -angle.cos() * 10.0 - angle.sin() * 3.5,
                -angle.sin() * 10.0 + angle.cos() * 3.5,
            ),
            a.end.offset(
                -angle.cos() * 10.0 + angle.sin() * 3.5,
                -angle.sin() * 10.0 - angle.cos() * 3.5,
            ),
        ]));
    }
    for a in &doc.annotations {
        out.push(Primitive::Text {
            position: a.position,
            text: a.text.clone(),
            size: 12.0,
            color: [34, 49, 54],
        });
    }
    out
}
fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
pub fn svg(doc: &Document) -> String {
    let (lo, hi) = doc.bounds();
    let mut s = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"{} {} {} {}\" width=\"{}\" height=\"{}\">\n",
        lo.x,
        lo.y,
        hi.x - lo.x,
        hi.y - lo.y,
        (hi.x - lo.x) * 2.0,
        (hi.y - lo.y) * 2.0
    );
    for p in primitives(doc) {
        match p {
            Primitive::Line(a, b, w) => {
                writeln!(s,"<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#223136\" stroke-width=\"{w}\" stroke-linecap=\"round\"/>",a.x,a.y,b.x,b.y).unwrap();
            }
            Primitive::Polygon(points) => {
                writeln!(
                    s,
                    "<polygon points=\"{}\" fill=\"#223136\"/>",
                    points
                        .iter()
                        .map(|p| format!("{},{}", p.x, p.y))
                        .collect::<Vec<_>>()
                        .join(" ")
                )
                .unwrap();
            }
            Primitive::Text {
                position,
                text,
                size,
                color,
            } => {
                writeln!(s,"<text x=\"{}\" y=\"{}\" font-family=\"Arial, sans-serif\" font-size=\"{size}\" fill=\"rgb({},{},{})\" dominant-baseline=\"text-before-edge\">{}</text>",position.x,position.y,color[0],color[1],color[2],escape(&text)).unwrap();
            }
        }
    }
    s.push_str("</svg>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn svg_preserves_annotations_and_escapes_xml() {
        let mut d = Document::default();
        d.annotations.push(crate::document::Annotation {
            id: 1,
            position: Point::default(),
            text: "A < B & C".into(),
        });
        let xml = svg(&d);
        assert!(xml.contains("A &lt; B &amp; C"));
        assert!(!xml.contains("NaN"));
    }
}
