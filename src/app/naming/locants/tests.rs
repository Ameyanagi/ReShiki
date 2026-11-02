use super::*;
use crate::app::naming::test_support::record;

#[test]
fn wrapped_locant_mouse_area_dispatches_enter_and_exit_without_a_font_renderer() {
    use iced::advanced::{Layout, Shell, layout, widget::Tree};
    use iced::{Event, Rectangle, Size, mouse};
    let mut view: Element<'_, Message, iced::Theme, ()> = row![
        Space::new().width(80).height(20),
        hover_segment::<()>(
            Space::new().width(20).height(20),
            LocantTarget::Preview,
            3,
            "5"
        ),
    ]
    .spacing(0)
    .wrap()
    .vertical_spacing(0)
    .into();
    let mut tree = Tree::new(view.as_widget());
    let viewport = Rectangle::with_size(Size::new(90., 60.));
    let node = view.as_widget_mut().layout(
        &mut tree,
        &(),
        &layout::Limits::new(Size::ZERO, viewport.size()),
    );
    let segment = Layout::new(&node).children().nth(1).unwrap().bounds();
    assert_eq!(segment.x, 0.);
    assert_eq!(segment.y, 20.);
    for (point, entered) in [
        (segment.center(), true),
        (iced::Point::new(85., 55.), false),
    ] {
        let mut messages = vec![];
        view.as_widget_mut().update(
            &mut tree,
            &Event::Mouse(mouse::Event::CursorMoved { position: point }),
            Layout::new(&node),
            mouse::Cursor::Available(point),
            &(),
            &mut iced::advanced::clipboard::Null,
            &mut Shell::new(&mut messages),
            &viewport,
        );
        assert_eq!(messages.len(), 1);
        assert!(
            matches!(&messages[0], Message::Naming(Action::HoverLocant { target: LocantTarget::Preview, token: 3, locant, hovered })
            if locant == "5" && *hovered == entered)
        );
    }
}

#[test]
fn tokenizer_recognizes_parent_fusion_interior_indicated_hydrogen_and_lambda_locants() {
    let name = "5-chloro-1H-4a,7a-dihydro-3a¹-5λ⁶-indole";
    let found = tokens(name)
        .into_iter()
        .map(|token| (name[token.range].to_owned(), token.locant))
        .collect::<Vec<_>>();
    assert_eq!(
        found,
        [
            ("5".into(), "5".into()),
            ("1H".into(), "1".into()),
            ("4a".into(), "4a".into()),
            ("7a".into(), "7a".into()),
            ("3a¹".into(), "3a¹".into()),
            ("5λ⁶".into(), "5".into()),
        ]
    );
}

#[test]
fn tokenizer_leaves_primed_substituent_and_bracketed_locants_unlinked() {
    let name = "2-(3-methyl-4-(5-ethyl)phenyl)-1',2′,3″-tricyclo[3.2.2.0¹,⁵]nonan-7-ol";
    let found = tokens(name)
        .into_iter()
        .map(|token| token.locant)
        .collect::<Vec<_>>();
    assert_eq!(found, ["2", "7"]);
    assert!(tokens("N1,N2-bis(2-methyl)-2′H-indole").is_empty());
}

#[test]
fn splitting_formatted_name_spans_preserves_full_name_typography() {
    let name = "L-(2R)-N′-5lambda^6-3a^1-tricyclo[3.2.2.0^{1,5}]nonane";
    let displayed = reshiki::typography::name_markup(name).unicode().text;
    let mut spans = super::super::aliases::chemical_name_spans(name, 13., "Test Family");
    for span in &mut spans {
        span.text = Cow::Owned(span.text.replace('\u{2060}', ""));
    }
    let mut ranges = vec![];
    let mut end = 0;
    for token in tokens(&displayed) {
        ranges.extend([end..token.range.start, token.range.clone()]);
        end = token.range.end;
    }
    ranges.push(end..displayed.len());
    let split: Vec<_> = ranges
        .into_iter()
        .flat_map(|range| range_spans(&spans, range))
        .collect();
    assert_eq!(
        split
            .iter()
            .map(|span| span.text.as_ref())
            .collect::<String>(),
        displayed
    );
    let italic = split
        .iter()
        .filter(|span| span.font.unwrap().style == iced::font::Style::Italic)
        .map(|span| span.text.as_ref())
        .collect::<String>();
    assert_eq!(italic, "RN");
    assert!(
        split
            .iter()
            .all(|span| span.font.unwrap().family == iced::font::Family::Name("Test Family"))
    );
    assert_eq!(split[0].size, Some(iced::Pixels(13. * 0.8)));
}

async fn indole_preview() -> Result<Preview, String> {
    // The real OPSIN parser and native importer run in-process; no application
    // executable, display server or installed fonts are required.
    let parser = opsin::Parser::new().map_err(|error| error.to_string())?;
    let parsed = parser.parse("5-chloro-1H-indole", &opsin::ParseOptions::strict());
    assert_eq!(parsed.status, opsin::Status::Success);
    let (smiles, atoms) = parsed
        .structure
        .ok_or("No parsed indole")?
        .semantic_cxsmiles_with_locants()
        .map_err(|error| error.to_string())?;
    let mut record = record(&smiles);
    record.title = "5-chloro-1H-indole".into();
    record.systematic_name = Some(record.title.clone());
    record.locants = Some(naming::Locants {
        smiles: smiles.clone(),
        atoms,
    });
    prepare_preview(
        LocalEngine::in_process(),
        record,
        smiles,
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())
}

#[tokio::test]
async fn indole_insertion_numbers_only_when_enabled_and_leaves_preview_unchanged()
-> Result<(), String> {
    let preview = indole_preview().await?;
    for import in [false, true] {
        for numbered in [false, true] {
            let (mut app, _) = App::new();
            app.tab.busy = false;
            app.tab.doc = Document::default();
            assert!(!app.tab.naming.number_by_locants);
            app.tab.naming.set_preview(preview.clone());
            let _ = app.naming_action(Action::NumberByLocants(numbered));
            if import {
                app.insert_import_preview(false);
            } else {
                app.insert_name_preview();
            }
            assert!(!app.error, "{}", app.status);
            assert_eq!(
                app.tab.naming.preview.as_ref().unwrap().document,
                preview.document
            );
            let labels: Vec<_> = app
                .tab
                .doc
                .atoms
                .iter()
                .filter_map(|atom| atom.display.number.as_ref())
                .collect();
            if numbered {
                assert_eq!(labels.len(), 9);
                assert!(labels.iter().any(|number| number.text == "7a"));
                assert!(
                    labels
                        .iter()
                        .all(|number| number.style == labels::number_style())
                );
                let five = app
                    .tab
                    .doc
                    .atoms
                    .iter()
                    .find(|atom| atom.display.number.as_ref().is_some_and(|n| n.text == "5"))
                    .ok_or("Missing C5")?;
                assert_eq!(five.element, "C");
                assert!(app.tab.doc.bonds.iter().any(|bond| {
                    let other = if bond.a == five.id {
                        Some(bond.b)
                    } else if bond.b == five.id {
                        Some(bond.a)
                    } else {
                        None
                    };
                    other
                        .and_then(|id| app.tab.doc.atom(id))
                        .is_some_and(|atom| atom.element == "Cl")
                }));
            } else {
                assert!(labels.is_empty());
            }
            app.step_history(false);
            assert!(app.tab.doc.atoms.is_empty());
        }
    }
    let modified = prepare_preview(
        LocalEngine::in_process(),
        preview.record.clone(),
        "CCO".into(),
        Default::default(),
    )
    .await
    .map_err(|error| error.to_string())?;
    assert!(
        insertion_document(&modified, true)
            .atoms
            .iter()
            .all(|atom| atom.display.number.is_none())
    );
    Ok(())
}

#[tokio::test]
async fn locant_hover_maps_preview_and_named_selection_without_document_or_history_changes()
-> Result<(), String> {
    let preview = indole_preview().await?;
    let five = preview
        .locants
        .iter()
        .find_map(|(&id, locant)| (locant == "5").then_some(id))
        .ok_or("Missing C5")?;
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.doc = preview.document.clone();
    app.tab.selected = app.tab.doc.all_ids();
    app.tab.naming.structure_atoms = editing::analysis_atoms(&app.tab.doc, &app.tab.selected);
    app.tab.naming.set_preview(preview.clone());
    let ticket = Ticket {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        serial: 1,
    };
    app.tab.naming.serial = 1;
    app.tab.naming.pending = Some(Pending::Generate(ticket));
    let _ = app.naming_action(Action::Finished(
        ticket,
        Box::new(Ok(Outcome::Structure(preview.record.clone()))),
    ));
    assert!(app.tab.naming.structure.is_some());
    let before = app.tab.doc.clone();
    let selected = app.tab.selected.clone();
    for target in [LocantTarget::Preview, LocantTarget::Structure] {
        let hover = |token, locant: &str, hovered| Action::HoverLocant {
            target,
            token,
            locant: locant.into(),
            hovered,
        };
        let _ = app.naming_action(hover(0, "5", true));
        let found = if target == LocantTarget::Structure {
            app.naming_canvas_highlight()
        } else {
            app.name_preview_highlight()
        };
        assert_eq!(found, [five]);
        // The same locant can appear twice, as in 2,2-dimethylpropane.
        let _ = app.naming_action(hover(9, "5", false));
        assert!(
            app.tab.naming.locant_hover.is_some(),
            "An old exit must not clear the next token"
        );
        let _ = app.naming_action(hover(0, "5", false));
        assert!(app.tab.naming.locant_hover.is_none());
        let _ = app.naming_action(hover(0, "99", true));
        assert!(app.tab.naming.locant_hover.is_none());
        assert_eq!(app.tab.doc, before);
        assert_eq!(app.tab.selected, selected);
        assert!(!app.tab.history.can_undo());
    }
    // The generated name keeps highlighting the atoms it named, not the selection.
    app.tab.selected.clear();
    app.hover_name_locant(LocantTarget::Structure, 0, "5".into(), true);
    assert_eq!(app.naming_canvas_highlight(), [five]);
    // A hidden inspector never sends the exit.
    app.inspector_open = false;
    app.invalidate_locant_hover();
    assert!(app.tab.naming.locant_hover.is_none());
    app.hover_name_locant(LocantTarget::Structure, 0, "5".into(), true);
    app.tab.revision += 1;
    assert!(app.naming_canvas_highlight().is_empty());
    app.hover_name_locant(LocantTarget::Preview, 0, "5".into(), true);
    let same_preview = app.tab.naming.preview.as_ref().unwrap().clone();
    app.tab.naming.set_preview(same_preview);
    assert!(app.tab.naming.locant_hover.is_none());
    app.hover_name_locant(LocantTarget::Preview, 0, "5".into(), true);
    let _ = app.naming_action(Action::Smiles("CCO".into()));
    assert!(app.tab.naming.locant_hover.is_none());
    Ok(())
}
