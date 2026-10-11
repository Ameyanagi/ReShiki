//! Optional Assistant setup and a local, known-reference image exercise.
use super::{Action, card};
use crate::app::{App, InspectorTab, Message};
use iced::widget::{column, container, row, text};
use iced::{Element, Length, Task};
use reshiki::{assistant::codex, document::Document, pictures::Picture};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) enum Connection {
    #[default]
    Unchecked,
    Checking,
    SignInRequired,
    Failed(codex::ConnectionError),
    Ready,
    Cancelled,
}

#[derive(Debug, Clone, Copy)]
pub enum Help {
    Setup,
    Install,
    SignIn,
}
impl Help {
    fn url(self) -> &'static str {
        match self {
            Self::Setup => "https://reshiki.com/guide/assistant-setup/",
            Self::Install => "https://learn.chatgpt.com/docs/cli",
            Self::SignIn => "https://learn.chatgpt.com/docs/auth",
        }
    }
}

const EXAMPLE_PROMPT: &str = "Reconstruct the attached image as an editable molecule. Preserve the visible atoms and bonds; report any uncertainty. Do not add a title.";
const TRANSFER: &str = "Send shares your request, recent conversation, relevant drawing data and attached image with the model through Codex. Review also sends rendered draft images and the source. Account access and usage limits apply; API-key usage has separate billing.";

fn setup_action<'a>(
    id: &'static str,
    label: &'static str,
    value: Action,
) -> reshiki::accessibility::Button<'a, Message> {
    reshiki::accessibility::button(id, label, text(label).size(12))
        .padding([7, 10])
        .on_press(Message::Assistant(value))
        .style(super::super::workspace::control(false))
}

fn reference() -> Result<Document, String> {
    Document::from_json(include_bytes!("../../../assets/assistant/ethanol.rsk"))
}

impl App {
    pub(super) fn assistant_setup_help(&mut self, help: Help) -> Task<Message> {
        if open::that(help.url()).is_err() {
            self.assistant.error = true;
            self.assistant.status = format!("Could not open the browser. Visit {}", help.url());
        }
        Task::none()
    }

    pub(super) fn assistant_image_example(&mut self) {
        if self.assistant.busy || self.assistant.reading_image || self.assistant.draft.is_some() {
            return;
        }
        match Picture::import(include_bytes!("../../../assets/assistant/ethanol.png")) {
            Ok(image) => {
                self.assistant.source_image = Some(image);
                self.assistant.input =
                    iced::widget::text_editor::Content::with_text(EXAMPLE_PROMPT);
                self.assistant.guided_example = true;
                self.assistant.requires_apply = true;
                self.assistant.example_reference = reference().ok();
                self.assistant.replace = false;
                self.assistant.status = "Example attached locally · Choose Send when ready".into();
                self.assistant.error = false;
                self.assistant.follow_chat = true;
            }
            Err(error) => {
                self.assistant.status = error;
                self.assistant.error = true;
            }
        }
    }

    pub(super) fn assistant_setup_card(&self) -> Element<'_, Message> {
        let state = &self.assistant;
        let failure_detail = if let Connection::Failed(error) = state.connection {
            error.to_string()
        } else {
            String::new()
        };
        let (title, detail, help) = match state.connection {
            Connection::Unchecked => (
                "Set up Assistant",
                "Check whether Codex is installed and signed in. Assistant is optional; drawing, imports and exports work without it.",
                Help::Setup,
            ),
            Connection::Checking => (
                "Checking Codex…",
                "ReShiki is checking the local Codex executable, saved sign-in and model catalog. You can cancel and keep drawing.",
                Help::Setup,
            ),
            Connection::SignInRequired => (
                "Sign in to Codex",
                "Codex is installed. In a terminal, run `codex login` and complete the browser sign-in. Check with `codex login status`, then test the connection.",
                Help::SignIn,
            ),
            Connection::Failed(codex::ConnectionError::MissingInstallation) => (
                "Install Codex",
                "ReShiki could not find Codex. Follow the official installation instructions for your operating system, restart ReShiki, then test the connection. On Windows, install the native codex.exe rather than only a WSL copy.",
                Help::Install,
            ),
            Connection::Failed(_) => (
                "Connection needs attention",
                failure_detail.as_str(),
                Help::Setup,
            ),
            Connection::Ready => (
                "Ready to try an image",
                "Codex is installed and signed in, and its model catalog is available. Sending a request checks access to the selected model; this connection check does not use an inference turn.",
                Help::Setup,
            ),
            Connection::Cancelled => (
                "Connection check cancelled",
                "You can keep drawing. Test the connection again when ready.",
                Help::Setup,
            ),
        };
        let mut content = column![text(title).size(16), text(detail.to_string()).size(12)]
            .spacing(9)
            .width(Length::Fill);
        let mut actions = row![setup_action(
            "assistant.setup.help",
            match help {
                Help::Install => "Install instructions",
                Help::SignIn => "Sign-in instructions",
                Help::Setup => "Setup guide",
            },
            Action::SetupHelp(help)
        )]
        .spacing(6);
        if state.connection == Connection::Checking {
            actions = actions.push(setup_action(
                "assistant.setup.cancel",
                "Cancel check",
                Action::Stop,
            ));
        } else {
            actions = actions.push(
                setup_action("assistant.setup.test", "Test connection", Action::Connect)
                    .value(format!("{title}. {detail}. {TRANSFER}"))
                    .on_press_maybe((!state.busy).then_some(Message::Assistant(Action::Connect))),
            );
        }
        content = content.push(actions)
            .push(text("Use your existing Codex sign-in. Credentials are managed by Codex; ReShiki does not ask for passwords or API keys here.").size(11).style(super::super::workspace::muted_text));
        if state.connection == Connection::Ready && !state.guided_example {
            content = content.push(
                setup_action(
                    "assistant.setup.example",
                    "Try example image · ethanol",
                    Action::ImageExample,
                )
                .value(
                    "Attach original ethanol image locally; Send and Apply require your actions.",
                )
                .on_press_maybe(
                    (!state.busy && !state.reading_image && state.draft.is_none())
                        .then_some(Message::Assistant(Action::ImageExample)),
                ),
            );
        }
        content = content
            .push(
                text(TRANSFER)
                    .size(11)
                    .style(super::super::workspace::muted_text),
            )
            .push(
                reshiki::accessibility::button(
                    "assistant.setup.drawing",
                    "Back to drawing",
                    text("Back to drawing").size(12),
                )
                .on_press(Message::Inspector(InspectorTab::Properties))
                .style(super::super::workspace::control(false)),
            );
        container(content)
            .padding(12)
            .width(Length::Fill)
            .style(|theme| crate::appearance::container(theme, card()))
            .into()
    }

    pub(super) fn assistant_example_card(&self) -> Element<'_, Message> {
        let mut content = column![
            text("Image exercise · ethanol").size(14),
            text("1. Choose Send to reconstruct the attached image.\n2. Compare the editable draft with this known reference: three heavy atoms (C–C–O), two single bonds, neutral, no specified stereochemistry.\n3. Inspect or edit the draft, then choose Apply. The exercise always waits for your Apply; Undo can remove it.").size(12),
        ].spacing(8);
        if let Some(source) = &self.assistant.source_image
            && let Some(handle) = source.handle(false)
        {
            content = content.push(text("Attached source image").size(11)).push(
                iced::widget::image(handle)
                    .height(76)
                    .content_fit(iced::ContentFit::Contain),
            );
        }
        if let Some(reference) = &self.assistant.example_reference {
            content = content.push(text("Known reference graph").size(11)).push(
                crate::canvas::layered::canvas(crate::canvas::DrawingPreview(reference))
                    .width(Length::Fill)
                    .height(100),
            );
        }
        content = content.push(text("A completed generation or visual review does not prove chemical identity. Check labels, bond orders and stereochemistry in your own images; unreadable or ambiguous input needs your decision.").size(11).style(super::super::workspace::muted_text));
        container(content)
            .padding(12)
            .width(Length::Fill)
            .style(|theme| crate::appearance::container(theme, card()))
            .into()
    }

    pub(super) fn assistant_transfer_note(&self) -> Element<'_, Message> {
        text(if self.assistant.source_image.is_some() {
            "Attached locally · Send uploads this image, request and drawing context through Codex. Review also shares source and rendered drafts."
        } else {
            "Send shares the request, conversation and drawing context through Codex. Review shares rendered drafts. Account usage limits apply."
        }).size(10).style(super::super::workspace::muted_text).into()
    }
}

#[cfg(test)]
mod tests;
