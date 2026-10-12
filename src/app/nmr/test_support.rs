use super::{Action, App, Document, Nucleus, Report, engine};
use std::sync::Arc;

pub(super) fn ethyl_acetate_doc() -> Document {
    Document::from_json(include_bytes!(
        "../../../tests/fixtures/nmr/ethyl-acetate.rsk"
    ))
    .unwrap()
}

pub(super) fn predicted(document: &Document, nucleus: Nucleus) -> Arc<Report> {
    let request = engine::prepare(document, &[]).unwrap();
    Arc::new(engine::predict(&request, nucleus).unwrap())
}

pub(super) fn accept(app: &mut App, report: Arc<Report>) {
    assert!(app.tab.nmr.job.pending.is_none());
    let _ = app.nmr_action(Action::Predict(report.nucleus));
    let key = app.tab.nmr.job.pending.clone().unwrap();
    let _ = app.nmr_action(Action::Finished(key, Ok(report.clone())));
    assert!(app.tab.nmr.job.pending.is_none());
    assert_eq!(app.tab.nmr.job.result_epoch, Some(app.tab.file_epoch));
    assert!(Arc::ptr_eq(
        app.tab.nmr.job.result.as_ref().unwrap(),
        &report
    ));
}
