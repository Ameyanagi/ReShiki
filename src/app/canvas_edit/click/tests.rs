use super::*;
use reshiki::typography::{Script, ScriptRun};

#[test]
fn atom_palette_replacement_clears_manual_label_runs() {
    let (mut app, _) = App::new();
    let mut source = Document::default();
    let id = source.add_atom("*", Point::default());
    source = reshiki::atom_text::apply_with_scripts(
        &source,
        id,
        "M2+",
        &[ScriptRun {
            start: 1,
            end: 3,
            script: Script::Superscript,
        }],
        reshiki::atom_text::Mode::Text,
    )
    .unwrap();
    app.tab.doc = source.clone();
    app.choose_element("N".into());
    app.canvas_click(Point::default(), source);
    let atom = app.tab.doc.atom(id).unwrap();
    assert_eq!(atom.element, "N");
    assert!(atom.display.variable.is_none());
    assert!(atom.display.label_scripts.is_empty());
    app.tab.doc.validate().unwrap();
}
