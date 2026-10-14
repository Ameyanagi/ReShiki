use super::*;
use crate::session::parse;
use std::ffi::OsString;

#[test]
fn launch_flag_opens_the_same_unbound_examples_tab() {
    let (mut app, _) = App::new();
    let _ = app.open_startup(parse(["--shortcut-examples"].map(OsString::from)));
    assert_eq!(app.document_name(), "Shortcut examples");
    assert!(app.tab.path.is_none());
    assert!(!app.dirty());
}
