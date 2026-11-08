use super::*;

fn app(document: Document) -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    app.tab.selected = document.all_ids();
    app.tab.doc = document;
    app
}

fn begin(app: &mut App) -> LocantsTicket {
    let _ = app.label_action(Action::IupacLocants);
    app.tab.labels.locants_job.as_ref().unwrap().ticket
}

fn complete(app: &mut App, ticket: LocantsTicket, numbers: HashMap<u64, String>) {
    let _ = app.label_action(Action::LocantsFinished(ticket, Box::new(Ok(numbers))));
}

#[test]
fn iupac_locants_keep_naphthalene_fusion_numbers_in_one_undo_step() {
    let parser = opsin::Parser::with_budget({
        let policy = reshiki_process_heap::policy::Capabilities::detect().resolve();
        opsin::Budget::new(
            policy.heap_bytes(),
            policy.work_units(),
            Some(policy.analysis_timeout()),
            policy.reservation_ceiling(),
            move |bytes| {
                policy
                    .try_reserve(bytes)
                    .map(|guard| Box::new(guard) as Box<dyn Send + Sync>)
            },
        )
    })
    .unwrap();
    let parsed = parser.parse("naphthalene", &opsin::ParseOptions::strict());
    let (smiles, atoms) = parsed
        .structure
        .unwrap()
        .semantic_cxsmiles_with_locants()
        .unwrap();
    let document = reshiki::templates::LIBRARY
        .iter()
        .find(|template| template.name == "Naphthalene")
        .unwrap()
        .document
        .clone();
    let mut app = app(document);
    assert!(!app.tab.labels.ring_atoms_only);
    let numbers = naming::Locants { smiles, atoms }.for_document(&app.tab.doc);
    assert_eq!(numbers.len(), 10);
    let before = app.tab.doc.clone();
    let ticket = begin(&mut app);
    complete(&mut app, ticket, numbers);
    let numbered = app.tab.doc.clone();
    let fusion_numbers: HashSet<_> = numbered
        .atoms
        .iter()
        .filter(|atom| {
            numbered
                .bonds
                .iter()
                .filter(|bond| bond.a == atom.id || bond.b == atom.id)
                .count()
                == 3
        })
        .map(|atom| atom.display.number.as_ref().unwrap().text.as_str())
        .collect();
    assert_eq!(fusion_numbers, HashSet::from(["4a", "8a"]));
    assert!(
        numbered
            .atoms
            .iter()
            .all(|atom| atom.display.number.is_some())
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    let _ = app.update(Message::Redo);
    assert_eq!(app.tab.doc, numbered);
}

#[tokio::test]
async fn abbreviation_selection_numbers_the_complete_chemical_parent() {
    let mut document = reshiki::templates::LIBRARY
        .iter()
        .find(|template| template.name == "Naphthalene")
        .unwrap()
        .document
        .clone();
    let atoms: Vec<_> = document.atoms.iter().map(|atom| atom.id).collect();
    document.contract(&atoms, "Nap", "").unwrap();
    let anchor = document.abbreviations[0].anchor;
    let numbers = generate_locant_numbers(document, vec![anchor], false, naming::Cancel::default())
        .await
        .unwrap();
    assert_eq!(numbers.len(), atoms.len());
    assert!(numbers.values().any(|locant| locant == "4a"));
    assert!(numbers.values().any(|locant| locant == "8a"));
}

#[test]
fn ring_only_locants_skip_chain_atoms_and_preserve_their_numbers() {
    let mut document = Document::default();
    let ring = reshiki::editing::ring(&mut document, Point::default(), 6, false, 42.);
    let chain = document.add_atom("C", Point::new(90., 0.));
    document.add_bond(ring[0], chain, 1, "plain");
    document.atom_mut(chain).unwrap().display.number = Some(Number {
        text: "chain".into(),
        offset: None,
        style: labels::number_style(),
    });
    let ids = document.all_ids();
    let all: HashMap<_, _> = ids
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, (i + 1).to_string()))
        .collect();
    assert_eq!(
        filter_locant_numbers(&document, all.clone(), false)
            .unwrap()
            .len(),
        7
    );
    let numbers = filter_locant_numbers(&document, all, true).unwrap();
    assert_eq!(numbers.len(), 6);
    assert!(!numbers.contains_key(&chain));
    let mut app = app(document);
    let ticket = begin(&mut app);
    complete(&mut app, ticket, numbers);
    assert!(app.tab.labels.locants_preview.is_none());
    assert_eq!(
        app.tab
            .doc
            .atom(chain)
            .unwrap()
            .display
            .number
            .as_ref()
            .unwrap()
            .text,
        "chain"
    );
    assert!(
        ring.iter()
            .all(|id| app.tab.doc.atom(*id).unwrap().display.number.is_some())
    );
}

#[test]
fn ring_only_locants_include_macrocycles_larger_than_display_cycle_limits() {
    let mut document = Document::default();
    let atoms: Vec<_> = (0..65)
        .map(|i| {
            let angle = std::f32::consts::TAU * i as f32 / 65.;
            document.add_atom("C", Point::new(250. * angle.cos(), 250. * angle.sin()))
        })
        .collect();
    for (&a, &b) in atoms
        .iter()
        .zip(atoms.iter().cycle().skip(1))
        .take(atoms.len())
    {
        document.add_bond(a, b, 1, "plain");
    }
    let numbers = atoms
        .iter()
        .enumerate()
        .map(|(i, &id)| (id, (i + 1).to_string()))
        .collect();
    assert_eq!(
        filter_locant_numbers(&document, numbers, true)
            .unwrap()
            .len(),
        65
    );
}

#[test]
fn existing_numbers_require_explicit_replace_and_cancel_keeps_them() {
    let mut document = Document::default();
    let atom = document.add_atom("C", Point::default());
    document.atom_mut(atom).unwrap().display.number = Some(Number {
        text: "custom".into(),
        offset: Some(Point::new(3., 4.)),
        style: labels::number_style(),
    });
    let mut app = app(document);
    let before = app.tab.doc.clone();
    let numbers = HashMap::from([(atom, "3a^12".into())]);
    let ticket = begin(&mut app);
    complete(&mut app, ticket, numbers.clone());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
    assert_eq!(
        app.tab
            .labels
            .locants_replacements(&app.tab.doc, app.tab.file_epoch, app.tab.revision),
        Some(1)
    );
    let _ = app.label_action(Action::CancelLocants);
    assert_eq!(app.tab.doc, before);
    assert!(app.tab.labels.locants_preview.is_none());
    let ticket = begin(&mut app);
    complete(&mut app, ticket, numbers);
    let _ = app.label_action(Action::ReplaceLocants);
    assert_eq!(
        app.tab
            .doc
            .atom(atom)
            .unwrap()
            .display
            .number
            .as_ref()
            .unwrap()
            .text,
        "3a¹²"
    );
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}

#[test]
fn stale_or_cancelled_locants_cannot_number_a_changed_drawing() {
    for stale in ["revision", "epoch", "cancel", "confirmation"] {
        let mut document = Document::default();
        let atom = document.add_atom("C", Point::default());
        let mut app = app(document);
        let ticket = begin(&mut app);
        let numbers = HashMap::from([(atom, "1".into())]);
        match stale {
            "revision" => app.tab.revision += 1,
            "epoch" => app.tab.file_epoch += 1,
            "cancel" => {
                let _ = app.label_action(Action::CancelLocants);
            }
            "confirmation" => {
                app.tab.labels.locants_preview = Some(LocantsPreview {
                    ticket,
                    numbers: numbers.clone(),
                });
                app.tab.revision += 1;
                assert_eq!(
                    app.tab.labels.locants_replacements(
                        &app.tab.doc,
                        app.tab.file_epoch,
                        app.tab.revision
                    ),
                    None
                );
                let _ = app.label_action(Action::ReplaceLocants);
            }
            _ => unreachable!(),
        }
        let before = app.tab.doc.clone();
        complete(&mut app, ticket, numbers);
        assert_eq!(app.tab.doc, before, "{stale}");
        assert!(!app.tab.history.can_undo(), "{stale}");
    }
}

#[test]
fn drawing_edits_cancel_pending_locants_and_generation_errors_use_the_status_bar() {
    let mut document = Document::default();
    document.add_atom("C", Point::default());
    let mut app = app(document);
    let first = begin(&mut app);
    assert_eq!(app.label_action(Action::IupacLocants).units(), 0);
    app.tab.revision += 1;
    app.invalidate_locant_numbering();
    assert!(!app.tab.labels.locants_running());
    let ticket = begin(&mut app);
    assert_ne!(ticket, first);
    let before = app.tab.doc.clone();
    let message = Message::Labels(Action::LocantsFinished(
        ticket,
        Box::new(Err("This structure cannot be named locally".into())),
    ));
    assert_eq!(
        super::super::results::async_result(&message),
        Some(super::super::results::Owner::Document)
    );
    let _ = app.update(message);
    assert_eq!(app.status, "This structure cannot be named locally");
    assert!(app.error);
    assert!(!app.tab.labels.locants_running());
    assert_eq!(app.tab.doc, before);
    assert!(!app.tab.history.can_undo());
}
