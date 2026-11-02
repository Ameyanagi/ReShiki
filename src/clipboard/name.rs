//! Chemical names retain their source text alongside rich text typography.
use super::Representation;
use crate::typography::{Script, TextSpan, TextStyle};

/// Plain text, HTML and RTF for a name whose markup spans are sorted,
/// nonoverlapping and on UTF-8 boundaries. A browser drops the RTF.
pub(super) fn representations(text: &str, spans: &[TextSpan]) -> Vec<Representation> {
    let mut html = String::from(r#"<meta charset="utf-8">"#);
    let mut rtf = String::from(r"{\rtf1\ansi\uc1 ");
    let mut end = 0;
    for span in spans {
        html_text(&mut html, &text[end..span.start]);
        rtf_text(&mut rtf, &text[end..span.start]);
        html_run(&mut html, &text[span.start..span.end], &span.style);
        rtf_run(&mut rtf, &text[span.start..span.end], &span.style);
        end = span.end;
    }
    html_text(&mut html, &text[end..]);
    rtf_text(&mut rtf, &text[end..]);
    rtf.push('}');
    // Richest first: some recipients take the first type they support.
    vec![
        Representation::new("public.html", html.as_bytes()),
        Representation::new("public.rtf", rtf.as_bytes()),
        Representation::new("public.utf8-plain-text", text.as_bytes()),
    ]
}

fn html_text(output: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            _ => output.push(c),
        }
    }
}

fn html_run(output: &mut String, text: &str, style: &TextStyle) {
    if style.italic {
        output.push_str("<i>");
    }
    match style.script {
        Script::Normal => {}
        Script::Subscript => output.push_str("<sub>"),
        Script::Superscript => output.push_str("<sup>"),
    }
    html_text(output, text);
    match style.script {
        Script::Normal => {}
        Script::Subscript => output.push_str("</sub>"),
        Script::Superscript => output.push_str("</sup>"),
    }
    if style.italic {
        output.push_str("</i>");
    }
}

fn rtf_text(output: &mut String, text: &str) {
    use std::fmt::Write as _;
    for unit in text.encode_utf16() {
        match unit {
            0x5c => output.push_str(r"\\"),
            0x7b => output.push_str(r"\{"),
            0x7d => output.push_str(r"\}"),
            0..=0x7f => output.push(char::from(unit as u8)),
            // RTF uses signed UTF-16 code units, including both halves of
            // supplementary characters. Each has one fallback character.
            _ => {
                let _ = write!(output, "\\u{}?", unit as i16);
            }
        }
    }
}

fn rtf_run(output: &mut String, text: &str, style: &TextStyle) {
    if style.italic {
        output.push_str(r"{\i ");
    }
    match style.script {
        Script::Normal => {}
        Script::Subscript => output.push_str(r"{\sub "),
        Script::Superscript => output.push_str(r"{\super "),
    }
    rtf_text(output, text);
    if style.script != Script::Normal {
        output.push('}');
    }
    if style.italic {
        output.push('}');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flavor(representations: &[Representation], kind: &str) -> String {
        String::from_utf8(
            representations
                .iter()
                .find(|representation| representation.kind == kind)
                .unwrap()
                .bytes()
                .unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn name_spans_keep_plain_text_and_scope_italic_and_superscript() {
        let text = "(1R)-N6-methyl";
        let spans = vec![
            TextSpan {
                start: 2,
                end: 3,
                style: TextStyle {
                    italic: true,
                    ..Default::default()
                },
            },
            TextSpan {
                start: 5,
                end: 6,
                style: TextStyle {
                    italic: true,
                    ..Default::default()
                },
            },
            TextSpan {
                start: 6,
                end: 7,
                style: TextStyle {
                    script: Script::Superscript,
                    ..Default::default()
                },
            },
        ];
        let representations = representations(text, &spans);
        assert_eq!(flavor(&representations, "public.utf8-plain-text"), text);
        assert_eq!(
            flavor(&representations, "public.html"),
            r#"<meta charset="utf-8">(1<i>R</i>)-<i>N</i><sup>6</sup>-methyl"#
        );
        assert_eq!(
            flavor(&representations, "public.rtf"),
            r"{\rtf1\ansi\uc1 (1{\i R})-{\i N}{\super 6}-methyl}"
        );
    }

    #[test]
    fn rich_text_escapes_source_without_changing_plain_text() {
        let text = r"\{&<>}";
        let representations = representations(text, &[]);
        assert_eq!(flavor(&representations, "public.utf8-plain-text"), text);
        assert_eq!(
            flavor(&representations, "public.html"),
            r#"<meta charset="utf-8">\{&amp;&lt;&gt;}"#
        );
        assert_eq!(
            flavor(&representations, "public.rtf"),
            r"{\rtf1\ansi\uc1 \\\{&<>\}}"
        );
    }

    #[test]
    fn unicode_is_utf8_in_html_and_signed_utf16_in_rtf() {
        let text = "é界髙😀";
        let representations = representations(text, &[]);
        assert_eq!(flavor(&representations, "public.utf8-plain-text"), text);
        assert_eq!(
            flavor(&representations, "public.html"),
            format!(r#"<meta charset="utf-8">{text}"#)
        );
        assert_eq!(
            flavor(&representations, "public.rtf"),
            r"{\rtf1\ansi\uc1 \u233?\u30028?\u-25895?\u-10179?\u-8704?}"
        );
    }

    #[test]
    fn styles_can_nest_without_leaking_into_following_text() {
        let text = "N6-H2";
        let spans = [
            TextSpan {
                start: 1,
                end: 2,
                style: TextStyle {
                    italic: true,
                    script: Script::Superscript,
                    ..Default::default()
                },
            },
            TextSpan {
                start: 4,
                end: 5,
                style: TextStyle {
                    script: Script::Subscript,
                    ..Default::default()
                },
            },
        ];
        let representations = representations(text, &spans);
        assert_eq!(
            flavor(&representations, "public.html"),
            r#"<meta charset="utf-8">N<i><sup>6</sup></i>-H<sub>2</sub>"#
        );
        assert_eq!(
            flavor(&representations, "public.rtf"),
            r"{\rtf1\ansi\uc1 N{\i {\super 6}}-H{\sub 2}}"
        );
    }
}
