//! Pins how the result predicates and modal gates admit, handle, drop or defer each async
//! completion, including the completions that some modals drop today.
use super::preview_passthrough;
use crate::app::{
    App, CleanupPreview, Job, Message, assistant, atom_text, cleanup, clipboard, document_styles,
    generic, import, inspector, naming, nmr, optimization, printing, reaction_mapping, tabs,
    template_library, theme_files, updates,
};
use iced::{Task, futures::StreamExt};
use reshiki::{
    chemistry::nmr::Nucleus,
    document::{Arrow, Document, Point},
    geometry::{Conformer, ForceField, Optimized, Point3},
    reactions::{self, Role},
    recovery::Recovery,
};
use std::{ops::ControlFlow, sync::Arc};

const FAILURE: &str = "Controlled completion failure";

fn drawing() -> App {
    let (mut app, _) = App::new();
    app.tab.busy = false;
    let a = app.tab.doc.add_atom("C", Point::default());
    let b = app.tab.doc.add_atom("C", Point::new(42., 0.));
    app.tab.doc.add_bond(a, b, 1, "plain");
    app.tab.selected = vec![a];
    app.tab.saved = app.tab.doc.clone();
    app
}

/// Start a real session and install the same small conformer used by optimization tests.
fn previewing(mut app: App) -> (App, optimization::Key) {
    let _ = app.optimization_action(optimization::Action::Begin);
    let key = app
        .optimization_key_for_test()
        .unwrap_or_else(|| panic!("A real optimization worker is pending: {}", app.status));
    let optimized = Optimized {
        conformer: Conformer {
            positions: vec![
                Point3::default(),
                Point3 {
                    x: 1.5,
                    y: 0.,
                    z: 0.4,
                },
            ],
            original_atom_count: 2,
            hydrogen_parents: vec![],
        },
        initial_energy: 2.,
        energy: 1.,
        gradient: None,
        converged: true,
        iterations: 20,
        force_field: ForceField::Mmff94s,
        diagnostics: vec![],
    };
    let _ = app.optimization_action(optimization::Action::WorkerDone(
        key,
        Ok(Arc::new(optimized)),
    ));
    assert!(app.tab.optimization.is_some(), "A 3D preview remains open");
    assert!(
        !app.error,
        "The conformer must produce a usable preview: {}",
        app.status
    );
    assert!(
        app.optimization_canvas()
            .is_some_and(|context| context.interactive),
        "The installed conformer must make the 3D preview interactive"
    );
    (app, key)
}

/// Consume only headless worker tasks, retaining their private tickets for Err receipts.
async fn output(task: Task<Message>) -> Message {
    let mut stream = iced_runtime::task::into_stream(task).expect("A worker task starts");
    let Some(iced_runtime::Action::Output(message)) = stream.next().await else {
        panic!("The headless worker must return a completion");
    };
    match message {
        Message::Tab(_, message) => *message,
        message => message,
    }
}

#[derive(Clone, Copy, Debug)]
enum ResultRow {
    NmrFinished,
    NmrExported,
    Generic,
    Naming,
    Mapping,
    Labels,
    Templates,
}

async fn pending_message(app: &mut App, row: ResultRow, directory: &tempfile::TempDir) -> Message {
    match row {
        ResultRow::NmrFinished => {
            let Message::Nmr(nmr::Action::Finished(key, _)) =
                output(app.nmr_action(nmr::Action::Predict(Nucleus::H1))).await
            else {
                panic!("NMR prediction returns Finished");
            };
            Message::Nmr(nmr::Action::Finished(key, Err(FAILURE.into())))
        }
        // Exported has no pending slot; it is an observable status receipt at dispatch.
        ResultRow::NmrExported => Message::Nmr(nmr::Action::Exported(Err(FAILURE.into()))),
        ResultRow::Generic => {
            let atom = app.tab.doc.atoms[1].id;
            app.tab.doc = reshiki::atom_text::apply(
                &app.tab.doc,
                atom,
                "[Cl,Br]",
                reshiki::atom_text::Mode::Auto,
            )
            .unwrap();
            app.tab.selected = app.tab.doc.all_ids();
            assert!(
                app.generic_action(generic::Action::Expand).units() > 0,
                "Generic: expansion is pending: {}",
                app.status
            );
            Message::Generic(generic::Action::Expanded {
                source: app.tab.id,
                epoch: app.tab.file_epoch,
                result: Box::new(Err(FAILURE.into())),
            })
        }
        ResultRow::Naming => {
            // An empty snapshot fails before any naming helper or chemistry work is needed.
            let drawing = std::mem::take(&mut app.tab.doc);
            let selected = std::mem::take(&mut app.tab.selected);
            let message = output(app.naming_action(naming::Action::Generate)).await;
            app.tab.doc = drawing;
            app.tab.selected = selected;
            let Message::Naming(naming::Action::Finished(ticket, _)) = message else {
                panic!("Naming returns Finished");
            };
            Message::Naming(naming::Action::Finished(
                ticket,
                Box::new(Err(FAILURE.into())),
            ))
        }
        ResultRow::Mapping => {
            let reactant = app.tab.doc.atoms[0].id;
            let product = app.tab.doc.add_atom("C", Point::new(250., 0.));
            let arrow = app.tab.doc.next_id();
            app.tab.doc.arrows.push(Arrow::new(
                arrow,
                Point::new(80., 0.),
                Point::new(180., 0.),
                Default::default(),
                Default::default(),
            ));
            reactions::assign(&mut app.tab.doc, arrow, &[reactant], Role::Reactant).unwrap();
            reactions::assign(&mut app.tab.doc, arrow, &[product], Role::Product).unwrap();
            app.tab.selected = vec![arrow];
            let Message::Mapping(reaction_mapping::Action::Ready(epoch, serial, _)) =
                output(app.mapping_action(reaction_mapping::Action::Auto)).await
            else {
                panic!("Auto-map returns Ready");
            };
            Message::Mapping(reaction_mapping::Action::Ready(
                epoch,
                serial,
                Err(FAILURE.into()),
            ))
        }
        ResultRow::Labels => {
            app.tab.labels_dirty = true;
            let Message::LabelsReady(key, _) = output(app.start_label_refresh()).await else {
                panic!("Label refresh returns LabelsReady");
            };
            Message::LabelsReady(key, Err(FAILURE.into()))
        }
        ResultRow::Templates => {
            let mut fragment = Document::default();
            fragment.add_atom("O", Point::default());
            app.templates.path = Some(directory.path().join("templates.json"));
            app.templates.draft = Some(fragment);
            app.templates.editing = true;
            app.templates.name = "Hydroxyl".into();
            let task = app.template_message(template_library::Action::SaveDetails);
            assert!(
                task.units() > 0 && app.templates.pending(),
                "Templates: the library transaction is pending"
            );
            // The first transaction in this fresh library has serial 1; never execute its write.
            Message::Templates(template_library::Action::Finished(1, Err(FAILURE.into())))
        }
    }
}

/// Columns: document result, updates background, atom-text background, preview, 3D admission.
async fn predicate_cases(key: optimization::Key) -> Vec<(&'static str, Message, [bool; 5])> {
    let mut app = drawing();
    let directory = tempfile::tempdir().unwrap();
    let nmr = pending_message(&mut app, ResultRow::NmrFinished, &directory).await;
    let naming = pending_message(&mut app, ResultRow::Naming, &directory).await;
    let labels = pending_message(&mut app, ResultRow::Labels, &directory).await;
    let Message::Imports(import::Action::Loaded(ticket, _)) =
        output(app.import_action(import::Action::Files(vec![
            directory.path().join("missing.smi"),
        ])))
        .await
    else {
        panic!("Headless file import returns Loaded");
    };
    app.tab.busy = false;
    app.tab.recovery = Some(Recovery::in_directory(directory.path()).unwrap());
    app.clear_recovery();
    let Message::Autosaved(autosave_key, _) = output(app.start_autosave()).await else {
        panic!("Recovery worker returns Autosaved");
    };
    let copy = clipboard::CopyAsKey {
        epoch: app.tab.file_epoch,
        revision: app.tab.revision,
        format: reshiki::clipboard::CopyFormat::Smiles,
        selected: vec![],
    };
    let mut cases = vec![
        (
            "EngineDone",
            Message::EngineDone {
                revision: 0,
                kind: Job::Analyze,
                result: Box::new(Err(FAILURE.into())),
            },
            [true, true, true, true, true],
        ),
        (
            "Optimization WorkerDone",
            Message::Optimization(optimization::Action::WorkerDone(key, Err(FAILURE.into()))),
            [true, true, false, false, true],
        ),
        ("Nmr Finished", nmr, [true; 5]),
        (
            "Nmr Exported",
            Message::Nmr(nmr::Action::Exported(Err(FAILURE.into()))),
            [true; 5],
        ),
        (
            "Exported",
            Message::Exported(Err(FAILURE.into())),
            [true, true, true, true, true],
        ),
        (
            "FigureExported",
            Message::FigureExported(Err(FAILURE.into())),
            [true, true, true, true, true],
        ),
        (
            "DrawingStyle Loaded",
            Message::DrawingStyle(document_styles::Action::Loaded(0, 0, Err(FAILURE.into()))),
            [true, true, false, false, true],
        ),
        (
            "DrawingStyle Saved",
            Message::DrawingStyle(document_styles::Action::Saved(Err(FAILURE.into()))),
            [true, true, false, false, true],
        ),
        (
            "Assistant Poll",
            Message::Assistant(assistant::Action::Poll),
            [true, true, true, false, true],
        ),
        (
            "Assistant Done",
            Message::Assistant(assistant::Action::Done {
                serial: 0,
                epoch: 0,
                revision: 0,
                replace: vec![],
                result: Box::new(Err(FAILURE.into())),
            }),
            [true, true, true, false, true],
        ),
        (
            "Assistant Connected",
            Message::Assistant(assistant::Action::Connected(
                0,
                Err(reshiki::assistant::codex::ConnectionError::Cancelled),
            )),
            [false, true, true, false, true],
        ),
        (
            "Assistant PreferencesSaved",
            Message::Assistant(assistant::Action::PreferencesSaved(Err(FAILURE.into()))),
            [false, true, true, false, true],
        ),
        (
            "Assistant ImageRead",
            Message::Assistant(assistant::Action::ImageRead {
                serial: 0,
                epoch: 0,
                image_only: false,
                result: Err(FAILURE.into()),
            }),
            [false, true, true, false, true],
        ),
        (
            "Assistant TextPasted",
            Message::Assistant(assistant::Action::TextPasted {
                serial: 0,
                epoch: 0,
                text: None,
            }),
            [false, true, true, false, true],
        ),
        ("LabelsReady", labels, [true, true, false, false, true]),
        (
            "Mapping Ready",
            Message::Mapping(reaction_mapping::Action::Ready(0, 0, Err(FAILURE.into()))),
            [true, true, false, false, true],
        ),
        (
            "Inspector PropertiesCalculated",
            Message::InspectorAction(inspector::Action::PropertiesCalculated(
                app.property_request_key(),
                Box::new(Err(FAILURE.into())),
            )),
            [true, true, false, false, true],
        ),
        (
            "Imports Loaded",
            Message::Imports(import::Action::Loaded(
                ticket,
                Box::new(Err(FAILURE.into())),
            )),
            [true, true, true, false, true],
        ),
        ("Naming Finished", naming, [true, true, true, true, true]),
        (
            "ClipboardWritten",
            Message::ClipboardWritten {
                epoch: 0,
                revision: 0,
                cut_ids: vec![],
                result: Err(FAILURE.into()),
            },
            [true, true, true, true, true],
        ),
        (
            "CopyAsPrepared",
            Message::CopyAsPrepared(copy.clone(), Box::new(Err(FAILURE.into()))),
            [true, true, true, true, true],
        ),
        (
            "CopyAsWritten",
            Message::CopyAsWritten(copy, Err(FAILURE.into())),
            [true, true, true, true, true],
        ),
        (
            "ClipboardRead",
            Message::ClipboardRead {
                epoch: 0,
                revision: 0,
                result: Box::new(Err(FAILURE.into())),
            },
            [true, true, true, true, true],
        ),
        (
            "Pasted",
            Message::Pasted(None),
            [true, true, false, false, true],
        ),
        // Pictures Loaded is omitted: Ticket has private fields and the only construction API
        // opens the native replacement file chooser, which this headless matrix cannot execute.
        (
            "Templates Finished",
            Message::Templates(template_library::Action::Finished(0, Err(FAILURE.into()))),
            [false; 5],
        ),
        (
            "Templates Imported",
            Message::Templates(template_library::Action::Imported(Err(FAILURE.into()))),
            [false; 5],
        ),
        (
            "Templates WarningAcknowledged",
            Message::Templates(template_library::Action::WarningAcknowledged(0)),
            [false; 5],
        ),
        (
            "Generic Expanded",
            Message::Generic(generic::Action::Expanded {
                source: app.tab.id,
                epoch: 0,
                result: Box::new(Err(FAILURE.into())),
            }),
            [false; 5],
        ),
        (
            "Autosaved",
            Message::Autosaved(autosave_key, Err(FAILURE.into())),
            [false, false, false, false, true],
        ),
        (
            "Saved",
            Message::Saved(0, Box::default(), Err(FAILURE.into())),
            [false, false, true, true, true],
        ),
        (
            "FilePrepared",
            Message::FilePrepared(None),
            [false, false, false, false, true],
        ),
        (
            "ThemeFile Loaded",
            Message::ThemeFile(theme_files::Action::Loaded(0, 0, Err(FAILURE.into()))),
            [false, true, false, false, true],
        ),
        (
            "ThemeFile Saved",
            Message::ThemeFile(theme_files::Action::Saved(Err(FAILURE.into()))),
            [false, true, false, false, true],
        ),
    ];
    if reshiki::printing::available() {
        // Execute preparation only. Dispatching its Ok receipt would open a native dialog.
        let Message::Printing(printing::Action::Prepared(ticket, _)) =
            output(app.print_action(printing::Action::Start(reshiki::printing::Scope::Document)))
                .await
        else {
            panic!("Headless print preparation returns Prepared");
        };
        cases.extend([
            (
                "Printing Prepared",
                Message::Printing(printing::Action::Prepared(ticket, Err(FAILURE.into()))),
                [true, true, true, false, true],
            ),
            (
                "Printing Finished",
                Message::Printing(printing::Action::Finished(ticket, Err(FAILURE.into()))),
                [true, true, true, false, true],
            ),
        ]);
    }
    // Linux omits both Printing receipts: Start refuses before constructing its private Ticket.
    cases
}

#[tokio::test]
async fn every_constructible_completion_pins_all_five_admission_predicates() {
    let (mut app, key) = previewing(drawing());
    let cases = predicate_cases(key).await;
    assert_eq!(
        cases.len(),
        if reshiki::printing::available() {
            35
        } else {
            33
        },
        "All constructible completion variants must be represented"
    );
    for (name, message, expected) in cases {
        let actual = [
            tabs::document_result(&message),
            updates::background(&message),
            atom_text::background(&message),
            preview_passthrough(&message),
            app.optimization_gate(&message).is_none(),
        ];
        for ((column, actual), expected) in [
            "document_result",
            "updates::background",
            "atom_text::background",
            "preview_passthrough",
            "optimization_gate",
        ]
        .into_iter()
        .zip(actual)
        .zip(expected)
        {
            assert_eq!(actual, expected, "{name}: {column}, {message:?}");
        }
        assert!(
            app.tab.optimization.is_some(),
            "{name}: preview survives admission"
        );
    }
}

#[derive(Clone, Copy, Debug)]
enum Modal {
    None,
    AtomText,
    Cleanup,
    Optimization,
    Updates,
    Restarting,
    Exit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Outcome {
    Continue,
    Handled,
    Dropped,
    Deferred,
    Discarded,
}

fn open_modal(mut app: App, modal: Modal, directory: &tempfile::TempDir) -> App {
    match modal {
        Modal::None => {}
        Modal::AtomText => {
            let id = app.tab.doc.atoms[0].id;
            let _ = app.atom_text_action(atom_text::Action::Begin(Some(id)));
            assert!(app.tab.atom_text.is_some(), "AtomText: the draft is open");
        }
        Modal::Cleanup => {
            app.tab.cleanup = Some(CleanupPreview {
                job: cleanup::CleanupJob {
                    options: Default::default(),
                    selection: vec![],
                    serial: app.tab.cleanup_serial,
                    epoch: app.tab.file_epoch,
                },
                warnings: vec![],
                document: app.tab.doc.clone(),
                analysis: None,
                revision: app.tab.revision,
                epoch: app.tab.file_epoch,
                original: false,
            });
        }
        Modal::Optimization => {
            // Generic expansion is app-scoped; its source may be behind the preview tab.
            // Geometry preparation cannot keep generic definitions from another component.
            if reshiki::generic::present(&app.tab.doc) {
                app.add_tab();
            }
            let a = app.tab.doc.add_atom("C", Point::new(500., 0.));
            let b = app.tab.doc.add_atom("C", Point::new(542., 0.));
            app.tab.doc.add_bond(a, b, 1, "plain");
            app.tab.selected = vec![a];
            app = previewing(app).0;
        }
        Modal::Updates => app.updates.open = true,
        Modal::Restarting => app.updates.restarting = true,
        Modal::Exit => {
            app.tab.saved = app.tab.doc.clone();
            app.tab.recovery = Some(Recovery::in_directory(directory.path()).unwrap());
            let _ = app.update(Message::Close(iced::window::Id::unique()));
            assert!(app.exit.frozen(), "Close waits for the recovery worker");
        }
    }
    app
}

#[tokio::test]
async fn pending_completions_pin_continue_handle_drop_and_defer_for_every_modal() {
    use Outcome::{Continue, Deferred, Discarded, Dropped, Handled};
    // Columns: no modal, atom text, cleanup, 3D optimization, updates, restarting, exit frozen.
    let rows = [
        (
            ResultRow::NmrFinished,
            [
                Continue, Continue, Continue, Continue, Continue, Dropped, Deferred,
            ],
        ),
        (
            ResultRow::NmrExported,
            [
                Continue, Continue, Continue, Continue, Continue, Dropped, Deferred,
            ],
        ),
        (
            ResultRow::Generic,
            [
                Continue, Discarded, Discarded, Discarded, Discarded, Discarded, Discarded,
            ],
        ),
        (
            ResultRow::Naming,
            [
                Continue, Continue, Continue, Continue, Continue, Dropped, Deferred,
            ],
        ),
        (
            ResultRow::Mapping,
            [
                Handled, Handled, Handled, Handled, Handled, Handled, Deferred,
            ],
        ),
        (
            ResultRow::Labels,
            [
                Handled, Handled, Handled, Handled, Handled, Handled, Deferred,
            ],
        ),
        (ResultRow::Templates, [Handled; 7]),
    ];
    for (row, expected) in rows {
        for (modal, expected) in [
            Modal::None,
            Modal::AtomText,
            Modal::Cleanup,
            Modal::Optimization,
            Modal::Updates,
            Modal::Restarting,
            Modal::Exit,
        ]
        .into_iter()
        .zip(expected)
        {
            let directory = tempfile::tempdir().unwrap();
            let mut app = drawing();
            let message = pending_message(&mut app, row, &directory).await;
            let source = app.tab.id;
            let context = format!("{row:?} under {modal:?}: {message:?}");
            let mut app = open_modal(app, modal, &directory);
            let deferred = app.tabs.deferred_results.len();
            let gate = app.gate(message);
            let handled = match row {
                ResultRow::NmrFinished => app.tab.nmr.notice.as_deref() == Some(FAILURE),
                ResultRow::NmrExported | ResultRow::Generic => app.error && app.status == FAILURE,
                // Naming's notice is private. Generate is a no-op while its request is pending.
                ResultRow::Naming => app.naming_action(naming::Action::Generate).units() > 0,
                ResultRow::Mapping => app.error && app.status == FAILURE,
                ResultRow::Labels => app.tab.chemistry_notice.as_deref() == Some(FAILURE),
                ResultRow::Templates => {
                    !app.templates.pending() && app.templates.notice.as_deref() == Some(FAILURE)
                }
            };
            let observed = if matches!(gate, ControlFlow::Continue(_)) {
                Continue
            } else if app.tabs.deferred_results.len() > deferred {
                Deferred
            } else if matches!(row, ResultRow::Generic)
                && !app.error
                && app.status == generic::EXPANSION_DISCARDED
                && !app.generic.is_pending()
            {
                Discarded
            } else if handled {
                Handled
            } else {
                Dropped
            };
            assert_eq!(observed, expected, "{context}");
            if matches!(row, ResultRow::Naming) {
                assert!(
                    !handled,
                    "{context}: naming completion remains pending at the gate"
                );
            }
            assert_eq!(
                app.tabs.deferred_results.len(),
                deferred + usize::from(expected == Deferred),
                "{context}: deferred queue"
            );
            if expected == Deferred {
                let (id, _) = app.tabs.deferred_results.last().unwrap();
                assert_eq!(*id, app.tab.id, "{context}: deferred owner");
            }
            match modal {
                Modal::None => {}
                Modal::AtomText => {
                    assert!(
                        app.tab.atom_text.is_some(),
                        "{context}: atom-text draft survives"
                    );
                }
                Modal::Cleanup => {
                    assert!(
                        app.tab.cleanup.is_some(),
                        "{context}: cleanup preview survives"
                    );
                }
                Modal::Optimization => {
                    assert!(
                        app.tab.optimization.is_some(),
                        "{context}: 3D preview survives"
                    );
                }
                Modal::Updates => assert!(app.updates.open, "{context}: updates stays open"),
                Modal::Restarting => {
                    assert!(app.updates.restarting, "{context}: restart remains active");
                }
                Modal::Exit => {
                    // A template-write failure intentionally cancels close in its early handler.
                    assert_eq!(
                        app.exit.frozen(),
                        !matches!(row, ResultRow::Templates),
                        "{context}: exit remains frozen except after template-write failure"
                    );
                }
            }
            // Retry APIs distinguish a discarded completion from one still pending at the gate.
            let retry = match row {
                ResultRow::NmrFinished => Some(app.nmr_action(nmr::Action::Predict(Nucleus::H1))),
                ResultRow::Generic => Some(
                    app.in_tab(source, |app| {
                        let ids = vec![app.tab.doc.atoms[0].id, app.tab.doc.atoms[1].id];
                        let selected = std::mem::replace(&mut app.tab.selected, ids);
                        let task = app.generic_action(generic::Action::Expand);
                        app.tab.selected = selected;
                        task
                    })
                    .expect("The generic expansion source remains open"),
                ),
                _ => None,
            };
            if let Some(task) = retry {
                if expected == Discarded {
                    assert!(
                        task.units() > 0,
                        "{context}: discarded work can start again"
                    );
                } else {
                    assert_eq!(
                        task.units(),
                        0,
                        "{context}: completion remains pending at the gate"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn every_constructible_completion_is_classified() {
    let (_, key) = previewing(drawing());
    let cases = predicate_cases(key).await;
    for (name, message, expected) in cases {
        let owner = crate::app::results::async_result(&message);
        assert!(owner.is_some(), "{name}: completion is classified");
        assert_eq!(
            owner == Some(crate::app::results::Owner::Document),
            expected[0],
            "{name}: document ownership"
        );
    }
}
