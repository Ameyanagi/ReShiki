//! Chemical names retain their source text alongside rich text typography.
use super::Representation;
use crate::typography::{NameMarkup, NameRun};

/// Plain text, HTML and RTF for a name whose markup spans are sorted,
/// nonoverlapping and on UTF-8 boundaries. A browser drops the RTF.
pub(super) fn representations(plain: &str, markup: &NameMarkup) -> Vec<Representation> {
    let text = &markup.text;
    let mut html = String::from(r#"<meta charset="utf-8">"#);
    let mut rtf = String::from(r"{\rtf1\ansi\uc1 ");
    let mut end = 0;
    for (range, run) in &markup.runs {
        html_text(&mut html, &text[end..range.start]);
        rtf_text(&mut rtf, &text[end..range.start]);
        let (open, close, control) = match run {
            NameRun::Italic => ("<i>", "</i>", r"{\i "),
            NameRun::Superscript => ("<sup>", "</sup>", r"{\super "),
            NameRun::Subscript => ("<sub>", "</sub>", r"{\sub "),
            // Cocoa/TextEdit ignores small-caps flags, so scale uppercase letters to 80%.
            NameRun::SmallCaps => (r#"<span style="font-size:80%">"#, "</span>", r"{\fs19 "),
        };
        html.push_str(open);
        html_text(&mut html, &text[range.clone()]);
        html.push_str(close);
        rtf.push_str(control);
        rtf_text(&mut rtf, &text[range.clone()]);
        rtf.push('}');
        end = range.end;
    }
    html_text(&mut html, &text[end..]);
    rtf_text(&mut rtf, &text[end..]);
    rtf.push('}');
    // Richest first: some recipients take the first type they support.
    vec![
        Representation::new("public.html", html.as_bytes()),
        Representation::new("public.rtf", rtf.as_bytes()),
        Representation::new("public.utf8-plain-text", plain.as_bytes()),
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
        let markup = NameMarkup {
            text: text.into(),
            runs: vec![
                (2..3, NameRun::Italic),
                (5..6, NameRun::Italic),
                (6..7, NameRun::Superscript),
            ],
        };
        let representations = representations(text, &markup);
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
        let markup = NameMarkup {
            text: text.into(),
            runs: vec![],
        };
        let representations = representations(text, &markup);
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
        let markup = NameMarkup {
            text: text.into(),
            runs: vec![],
        };
        let representations = representations(text, &markup);
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
    fn adjacent_runs_do_not_leak_into_following_text() {
        let text = "N6-H2";
        let markup = NameMarkup {
            text: text.into(),
            runs: vec![
                (0..1, NameRun::Italic),
                (1..2, NameRun::Superscript),
                (4..5, NameRun::Subscript),
            ],
        };
        let representations = representations(text, &markup);
        assert_eq!(
            flavor(&representations, "public.html"),
            r#"<meta charset="utf-8"><i>N</i><sup>6</sup>-H<sub>2</sub>"#
        );
        assert_eq!(
            flavor(&representations, "public.rtf"),
            r"{\rtf1\ansi\uc1 {\i N}{\super 6}-H{\sub 2}}"
        );
    }

    #[test]
    fn plain_text_keeps_engine_names_while_rich_text_uses_display_markup() {
        for (plain, html, rtf) in [
            (
                "L-alanine",
                r#"<span style="font-size:80%">L</span>-alanine"#,
                r"{\fs19 L}-alanine",
            ),
            ("1,1'-biphenyl", "1,1′-biphenyl", r"1,1\u8242?-biphenyl"),
        ] {
            let markup = crate::typography::name_markup(plain);
            let representations = representations(plain, &markup);
            assert_eq!(flavor(&representations, "public.utf8-plain-text"), plain);
            assert_eq!(
                flavor(&representations, "public.html"),
                format!(r#"<meta charset="utf-8">{html}"#)
            );
            assert_eq!(
                flavor(&representations, "public.rtf"),
                format!(r"{{\rtf1\ansi\uc1 {rtf}}}")
            );
        }
    }
}
