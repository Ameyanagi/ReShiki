use super::*;
use objc2_foundation::{NSString, NSURL};

fn event() -> Retained<NSAppleEventDescriptor> {
    NSAppleEventDescriptor::appleEventWithEventClass_eventID_targetDescriptor_returnID_transactionID(
        CORE,
        OPEN_DOCUMENTS,
        None,
        -1,
        0,
    )
}

#[test]
fn file_descriptors_preserve_unicode_spaces_and_order_and_deduplicate() -> Result<(), String> {
    let event = event();
    let list = NSAppleEventDescriptor::listDescriptor();
    for (i, path) in [
        "/tmp/日本語 drawing.rsk",
        "/tmp/second.rsk",
        "/tmp/日本語 drawing.rsk",
    ]
    .into_iter()
    .enumerate()
    {
        let url = NSURL::fileURLWithPath(&NSString::from_str(path));
        list.insertDescriptor_atIndex(
            &NSAppleEventDescriptor::descriptorWithFileURL(&url),
            i as isize + 1,
        );
    }
    event.setParamDescriptor_forKeyword(&list, DIRECT_OBJECT);
    assert_eq!(
        paths(&event)?,
        vec![
            PathBuf::from("/tmp/日本語 drawing.rsk"),
            PathBuf::from("/tmp/second.rsk")
        ]
    );
    Ok(())
}

#[test]
fn malformed_or_empty_document_events_are_checked_errors() {
    let event = event();
    assert!(paths(&event).is_err());
    let list = NSAppleEventDescriptor::listDescriptor();
    event.setParamDescriptor_forKeyword(&list, DIRECT_OBJECT);
    assert!(paths(&event).is_err());
    list.insertDescriptor_atIndex(
        &NSAppleEventDescriptor::descriptorWithString(&NSString::from_str("invalid")),
        1,
    );
    event.setParamDescriptor_forKeyword(&list, DIRECT_OBJECT);
    assert!(paths(&event).is_err());
}
