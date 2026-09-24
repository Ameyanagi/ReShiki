//! macOS document-open events, isolated from the safe editor and chemistry code.
#![cfg(target_os = "macos")]
#![deny(unsafe_op_in_unsafe_fn)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_app_kit::{NSApplication, NSApplicationWillFinishLaunchingNotification};
use objc2_foundation::{
    MainThreadMarker, NSAppleEventDescriptor, NSAppleEventManager, NSNotification,
    NSNotificationCenter, NSObject,
};
use std::path::PathBuf;
use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

pub type OpenRequest = Result<Vec<PathBuf>, String>;
const CORE: u32 = u32::from_be_bytes(*b"aevt");
const OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

define_class!(
    // SAFETY: NSObject has no additional subclassing invariants; ivars contain
    // an owned sender, and the class does not implement Drop.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = UnboundedSender<OpenRequest>]
    struct DocumentEventHandler;

    impl DocumentEventHandler {
        // SAFETY: Notification selectors receive one NSNotification object.
        #[unsafe(method(willFinishLaunching:))]
        fn will_finish_launching(&self, _notification: &NSNotification) {
            register(self);
        }

        // SAFETY: This is the documented NSAppleEventManager callback signature.
        // Optional references also tolerate a nil event or reply descriptor.
        #[unsafe(method(openDocuments:withReplyEvent:))]
        fn open_documents(&self, event: Option<&NSAppleEventDescriptor>, _reply: Option<&NSAppleEventDescriptor>) {
            let request = event.ok_or_else(|| "The document-open event was empty".to_owned())
                .and_then(paths);
            let _ = self.ivars().send(request);
        }
    }
);

fn paths(event: &NSAppleEventDescriptor) -> OpenRequest {
    let list = event
        .paramDescriptorForKeyword(DIRECT_OBJECT)
        .ok_or("The document-open event did not contain files")?;
    let count = list.numberOfItems();
    if !(1..=1024).contains(&count) {
        return Err("Open between 1 and 1024 documents at a time".into());
    }
    let mut paths = Vec::new();
    for index in 1..=count {
        let url = list
            .descriptorAtIndex(index)
            .and_then(|d| d.fileURLValue())
            .ok_or("The document-open event contained an invalid file URL")?;
        if !url.isFileURL() {
            return Err("Only local files can be opened as drawings".into());
        }
        let path = url.path().ok_or("The file URL has no path")?;
        let path = PathBuf::from(path.to_string());
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    Ok(paths)
}

/// Keep this guard alive on the main thread until the application exits.
pub struct FileOpenHandler {
    _handler: Retained<DocumentEventHandler>,
}

impl Drop for FileOpenHandler {
    fn drop(&mut self) {
        // SAFETY: This retained observer is still alive and was registered below.
        unsafe {
            NSNotificationCenter::defaultCenter().removeObserver(&self._handler);
        }
        NSAppleEventManager::sharedAppleEventManager()
            .removeEventHandlerForEventClass_andEventID(CORE, OPEN_DOCUMENTS);
    }
}

pub fn install_document_events() -> Result<(FileOpenHandler, UnboundedReceiver<OpenRequest>), String>
{
    let main =
        MainThreadMarker::new().ok_or("Document events must be installed on the main thread")?;
    // Initialize AppKit's standard handlers first, then replace only Open Documents.
    let _app = NSApplication::sharedApplication(main);
    let (sender, receiver) = mpsc::unbounded_channel();
    let allocated = DocumentEventHandler::alloc(main).set_ivars(sender);
    // SAFETY: The allocated NSObject subclass has initialized ivars and the
    // inherited init method takes no arguments and returns this object retained.
    let handler: Retained<DocumentEventHandler> = unsafe { msg_send![super(allocated), init] };
    // AppKit replaces its default Apple event handlers during finishLaunching.
    // Register at WillFinishLaunching, before the first Open Documents event.
    // SAFETY: The observer implements this notification selector, remains alive
    // in the guard, and the framework exports this immutable notification name.
    unsafe {
        NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
            &handler,
            sel!(willFinishLaunching:),
            Some(NSApplicationWillFinishLaunchingNotification),
            Some(&_app),
        );
    }
    register(&handler);
    Ok((FileOpenHandler { _handler: handler }, receiver))
}

fn register(handler: &DocumentEventHandler) {
    // SAFETY: The receiver implements this exact two-descriptor selector and
    // remains retained until removal. Both entry points run on the main thread.
    unsafe {
        NSAppleEventManager::sharedAppleEventManager()
            .setEventHandler_andSelector_forEventClass_andEventID(
                handler,
                sel!(openDocuments:withReplyEvent:),
                CORE,
                OPEN_DOCUMENTS,
            );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2_foundation::{NSString, NSURL};

    fn event() -> Retained<NSAppleEventDescriptor> {
        NSAppleEventDescriptor::appleEventWithEventClass_eventID_targetDescriptor_returnID_transactionID(
            CORE, OPEN_DOCUMENTS, None, -1, 0,
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
}
