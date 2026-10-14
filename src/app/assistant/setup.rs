//! Optional Assistant setup and a local, known-reference image exercise.
use super::{Action, card};
use crate::app::workspace::{accessible_command, muted_text};
use crate::app::{App, Message};
use iced::widget::{Column, Row, column, container, rich_text, row, span, text};
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

impl Connection {
    pub(super) fn from_account(account: &codex::Account) -> Self {
        if account.connected {
            Self::Ready
        } else {
            Self::SignInRequired
        }
    }

    pub(super) fn from_error(error: codex::ConnectionError) -> Self {
        if error == codex::ConnectionError::Cancelled {
            Self::Cancelled
        } else {
            Self::Failed(error)
        }
    }

    fn copy(self) -> SetupCopy {
        let (title, detail, help) = match self {
            Self::Unchecked => (UNCHECKED_TITLE, UNCHECKED_DETAIL.into(), Help::Setup),
            Self::Checking => (CHECKING_TITLE, CHECKING_DETAIL.into(), Help::Setup),
            Self::SignInRequired => (SIGN_IN_TITLE, SIGN_IN_DETAIL.into(), Help::SignIn),
            Self::Failed(codex::ConnectionError::MissingInstallation) => {
                (INSTALL_TITLE, INSTALL_DETAIL.into(), Help::Install)
            }
            Self::Failed(error) => (FAILED_TITLE, error.to_string(), Help::Setup),
            Self::Ready => (READY_TITLE, READY_DETAIL.into(), Help::Setup),
            Self::Cancelled => (CANCELLED_TITLE, CANCELLED_DETAIL.into(), Help::Setup),
        };
        SetupCopy {
            title,
            detail,
            help,
        }
    }

    /// Whether `status` only repeats the problem this card already shows.
    pub(super) fn repeats(self, status: &str) -> bool {
        match self {
            Self::SignInRequired => status == SIGN_IN_STATUS,
            Self::Failed(error) => status == error.to_string(),
            _ => false,
        }
    }

    pub(super) fn cancel_check(&mut self) -> bool {
        let checking = *self == Self::Checking;
        if checking {
            *self = Self::Cancelled;
        }
        checking
    }
}

#[derive(Debug, Clone)]
struct SetupCopy {
    title: &'static str,
    detail: String,
    help: Help,
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

const UNCHECKED_TITLE: &str = "Set up Assistant";
const UNCHECKED_DETAIL: &str = concat!(
    "Check whether Codex is installed and signed in. Assistant is optional; drawing, ",
    "imports and exports work without it.",
);
const CHECKING_TITLE: &str = "Checking Codex…";
const CHECKING_DETAIL: &str = concat!(
    "ReShiki is checking the local Codex executable, saved sign-in and model catalog. You ",
    "can cancel and keep drawing.",
);
/// The status after a check that found Codex installed but signed out.
pub(super) const SIGN_IN_STATUS: &str = "Run `codex login` to sign in, then reconnect.";
const SIGN_IN_TITLE: &str = "Sign in to Codex";
const SIGN_IN_DETAIL: &str = concat!(
    "Codex is installed. In a terminal, run `codex login` and complete the browser ",
    "sign-in. Check with `codex login status`, then test the connection.",
);
const INSTALL_TITLE: &str = "Install Codex";
const INSTALL_DETAIL: &str = concat!(
    "ReShiki could not find Codex. Follow the official installation instructions for your ",
    "operating system, restart ReShiki, then test the connection. On Windows, install the ",
    "native codex.exe rather than only a WSL copy.",
);
const FAILED_TITLE: &str = "Connection needs attention";
const READY_TITLE: &str = "Ready to try an image";
const READY_DETAIL: &str = concat!(
    "Codex is installed and signed in, and its model catalog is available. Sending a ",
    "request checks access to the selected model; this connection check does not use an ",
    "inference turn.",
);
const CANCELLED_TITLE: &str = "Connection check cancelled";
const CANCELLED_DETAIL: &str = "You can keep drawing. Test the connection again when ready.";
const CREDENTIALS: &str = concat!(
    "Use your existing Codex sign-in. Credentials are managed by Codex; ReShiki does not ",
    "ask for passwords or API keys here.",
);
const EXAMPLE_PROMPT: &str = concat!(
    "Reconstruct the attached image as an editable molecule. Preserve the visible atoms ",
    "and bonds; report any uncertainty. Do not add a title.",
);
const TRANSFER: &str = concat!(
    "Send shares your request, recent conversation, relevant drawing data and attached ",
    "image with the model through Codex. Review also sends rendered draft images and the ",
    "source. Account access and usage limits apply; API-key usage has separate billing.",
);
const EXAMPLE_TITLE: &str = "Image exercise · ethanol";
const EXAMPLE_INSTRUCTIONS: &str = concat!(
    "1. Choose Send to reconstruct the attached image.\n2. Compare the editable draft ",
    "with this known reference: three heavy atoms (C–C–O), two single bonds, neutral, no ",
    "specified stereochemistry.\n3. Inspect or edit the draft, then choose Apply. The ",
    "exercise always waits for your Apply; Undo can remove it.",
);
const EXAMPLE_CAUTION: &str = concat!(
    "A completed generation or visual review does not prove chemical identity. Check ",
    "labels, bond orders and stereochemistry in your own images; unreadable or ambiguous ",
    "input needs your decision.",
);
const IMAGE_TRANSFER: &str = concat!(
    "Attached locally · Send uploads this image, request and drawing context through ",
    "Codex. Review also shares source and rendered drafts.",
);
const DRAWING_TRANSFER: &str = concat!(
    "Send shares the request, conversation and drawing context through Codex. Review ",
    "shares rendered drafts. Account usage limits apply.",
);

/// A setup command; `primary` marks the card's next step.
fn setup_action<'a>(
    id: &'static str,
    label: &'static str,
    value: Action,
    primary: bool,
) -> reshiki::accessibility::Button<'a, Message> {
    let button = accessible_command(id, label, label).on_press(Message::Assistant(value));
    if primary {
        button.style(crate::appearance::primary)
    } else {
        button
    }
}

/// `detail` with each `command` in backticks set in a monospace font.
fn detail_text(detail: &str) -> Element<'static, Message> {
    let spans = detail.split('`').enumerate().map(|(index, part)| {
        let part: iced::widget::text::Span<'static> = span(part.to_owned());
        if index % 2 == 1 {
            part.font(iced::Font::MONOSPACE)
        } else {
            part
        }
    });
    rich_text(spans.collect::<Vec<_>>())
        .size(12)
        .width(Length::Fill)
        .into()
}

fn reference() -> Result<Document, String> {
    Document::from_json(include_bytes!("../../../assets/assistant/ethanol.rsk"))
}

impl App {
    pub(super) fn assistant_connection_failed(&mut self, error: codex::ConnectionError) {
        self.assistant.account = None;
        self.assistant.connection = Connection::from_error(error);
        self.assistant.status = error.to_string();
        self.assistant.error = error != codex::ConnectionError::Cancelled;
    }

    pub(super) fn assistant_setup_help(&mut self, help: Help) -> Task<Message> {
        Task::perform(
            reshiki::updates::open_url(help.url().into()),
            move |result| Message::Assistant(Action::SetupHelpOpened(help, result)),
        )
    }

    pub(super) fn assistant_setup_help_opened(&mut self, help: Help, result: Result<(), String>) {
        if result.is_err() {
            self.assistant.error = true;
            self.assistant.status = format!("Could not open the browser. Visit {}", help.url());
        }
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
        let copy = self.assistant.connection.copy();
        let actions = self.setup_actions(&copy);
        let content = column![text(copy.title).size(16), detail_text(&copy.detail)]
            .spacing(9)
            .width(Length::Fill)
            .push(actions);
        let content = self.setup_footer(content);
        container(content)
            .padding(12)
            .width(Length::Fill)
            .style(|theme| crate::appearance::container(theme, card()))
            .into()
    }

    fn setup_actions(&self, copy: &SetupCopy) -> Row<'_, Message> {
        let state = &self.assistant;
        // Instructions are the next step while Codex is missing or signed
        // out; otherwise it is the connection test, or the example once ready.
        let instructions = !matches!(copy.help, Help::Setup);
        let mut actions = row![setup_action(
            "assistant.setup.help",
            match copy.help {
                Help::Install => "Install instructions",
                Help::SignIn => "Sign-in instructions",
                Help::Setup => "Setup guide",
            },
            Action::SetupHelp(copy.help),
            instructions,
        )]
        .spacing(6);
        if state.connection == Connection::Checking {
            actions = actions.push(setup_action(
                "assistant.setup.cancel",
                "Cancel check",
                Action::Stop,
                false,
            ));
        } else {
            let title = copy.title;
            let detail = &copy.detail;
            let value = format!("{title}. {detail}. {TRANSFER}");
            let next = !instructions && state.connection != Connection::Ready;
            actions = actions.push(
                setup_action(
                    "assistant.setup.test",
                    "Test connection",
                    Action::Connect,
                    next,
                )
                .value(value)
                .on_press_maybe((!state.busy).then_some(Message::Assistant(Action::Connect))),
            );
        }
        actions
    }

    /// The credential note and, once connected, the example. The composer
    /// note below the input says what Send shares.
    fn setup_footer<'a>(&self, mut content: Column<'a, Message>) -> Column<'a, Message> {
        let state = &self.assistant;
        content = content.push(text(CREDENTIALS).size(11).style(muted_text));
        if state.connection == Connection::Ready && !state.guided_example {
            content = content.push(
                setup_action(
                    "assistant.setup.example",
                    "Try example image · ethanol",
                    Action::ImageExample,
                    true,
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
        content
    }

    pub(super) fn assistant_example_card(&self) -> Element<'_, Message> {
        let mut content = column![
            text(EXAMPLE_TITLE).size(14),
            text(EXAMPLE_INSTRUCTIONS).size(12),
        ]
        .spacing(8);
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
        content = content.push(text(EXAMPLE_CAUTION).size(11).style(muted_text));
        container(content)
            .padding(12)
            .width(Length::Fill)
            .style(|theme| crate::appearance::container(theme, card()))
            .into()
    }

    pub(super) fn assistant_transfer_note(&self) -> Element<'_, Message> {
        let note = if self.assistant.source_image.is_some() {
            IMAGE_TRANSFER
        } else {
            DRAWING_TRANSFER
        };
        text(note).size(10).style(muted_text).into()
    }
}

#[cfg(test)]
mod tests;
