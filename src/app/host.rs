//! Requests from the page that hosts the browser editor; see web.rs.
use super::{App, Message};
use crate::web::Request;
use iced::Task;

impl App {
    pub(super) fn host_request(&mut self, request: Request) -> Task<Message> {
        match request {
            // Start the front tab over, dropping text drafts and previews
            // that belong to the drawing it replaces.
            Request::Load(doc) => {
                self.reset_tab();
                self.show_opened(*doc, None);
            }
            // Include a text draft still being typed, as Save and Export do,
            // without ending the edit.
            Request::Snapshot(id) => crate::web::reply(id, self.inline_candidate()),
            Request::Close => return iced::exit(),
            // The canvas sees the new font when it draws after this update.
            Request::Redraw => {}
            Request::Drag(files) => self.browser_drag(files),
            // Open or insert them as chosen files, through the same checks.
            // The import keeps the tab and drawing the files were dropped
            // on, whichever is in front once they are read.
            Request::Drop(files) => {
                let (id, ticket) = (self.tab.id, self.import_ticket());
                let read = crate::web::read_files(files, super::import::limit);
                return Task::perform(read, move |files| {
                    let files = files.map(super::files::browser_files);
                    let read = super::import::Action::Read(ticket, files);
                    Message::Tab(id, Box::new(Message::Imports(read)))
                });
            }
        }
        Task::none()
    }

    /// The drawing in front and its revision, which changes with every edit.
    pub(crate) fn revision(&self) -> (u64, u64) {
        (self.tab.id.0, self.tab.revision)
    }
}
