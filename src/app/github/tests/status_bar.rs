//! The GitHub button at the left end of the status bar, and the ↗ and
//! right-click menu of the picker's repository rows, laid out and clicked
//! with the real renderer.
use super::*;
use iced::advanced::renderer::Headless;
use iced::advanced::widget::{Id, Operation, operation};
use iced::mouse::{Button, Cursor, Event as Mouse};
use iced::{Point, Rectangle, Size};
use reshiki::accessibility::Node;

/// The keyboard stops in Tab order, and the texts drawn.
#[derive(Default)]
struct Stops {
    stops: Vec<Option<Id>>,
    texts: Vec<(String, Rectangle)>,
}
impl Operation for Stops {
    fn focusable(&mut self, id: Option<&Id>, _: Rectangle, _: &mut dyn operation::Focusable) {
        self.stops.push(id.cloned());
    }
    fn text(&mut self, _: Option<&Id>, bounds: Rectangle, text: &str) {
        self.texts.push((text.to_owned(), bounds));
    }
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation)) {
        operate(self);
    }
}

/// The window as laid out once.
struct Pass {
    nodes: Vec<Node>,
    messages: Vec<Message>,
    found: Stops,
}
impl Pass {
    fn node(&self, id: &str) -> &Node {
        (self.nodes.iter())
            .find(|node| node.id == id)
            .unwrap_or_else(|| panic!("{id} is in the accessibility tree"))
    }
    fn has(&self, id: &str) -> bool {
        self.nodes.iter().any(|node| node.id == id)
    }
    /// Where `id` comes in Tab order.
    fn stop(&self, id: &str) -> usize {
        (self.found.stops.iter())
            .position(|stop| *stop == Some(Id::from(id.to_owned())))
            .unwrap_or_else(|| panic!("{id} takes keyboard focus"))
    }
    fn text(&self, text: &str) -> Rectangle {
        (self.found.texts.iter())
            .find(|(found, _)| found == text)
            .unwrap_or_else(|| panic!("{text:?} is drawn"))
            .1
    }
}

struct Ui {
    renderer: iced::Renderer,
    size: Size,
}
impl Ui {
    async fn new(size: Size) -> Self {
        let renderer = <iced::Renderer as Headless>::new(
            iced::Font::with_name(reshiki::style::ui_font_family()),
            iced::Pixels(16.),
            None,
        )
        .await
        .expect("headless renderer");
        Self { renderer, size }
    }

    /// Sends `events` with the pointer at `at`, then reads the window.
    fn pass(&mut self, app: &App, at: Option<Point>, events: &[Mouse]) -> Pass {
        let mut ui = iced_runtime::UserInterface::build(
            app.view(),
            self.size,
            iced_runtime::user_interface::Cache::new(),
            &mut self.renderer,
        );
        let mut messages = Vec::new();
        let events: Vec<_> = events.iter().cloned().map(iced::Event::Mouse).collect();
        ui.update(
            &events,
            at.map_or(Cursor::Unavailable, Cursor::Available),
            &mut self.renderer,
            &mut iced::advanced::clipboard::Null,
            &mut messages,
        );
        let mut collect = reshiki::accessibility::Collect::new(Rectangle::with_size(self.size));
        ui.operate(&self.renderer, &mut operation::black_box(&mut collect));
        let mut found = Stops::default();
        ui.operate(&self.renderer, &mut found);
        Pass {
            nodes: collect.snapshot().nodes.clone(),
            messages,
            found,
        }
    }

    /// A left click at `at`: the messages it sends.
    fn click(&mut self, app: &App, at: Point) -> Vec<Message> {
        self.pass(
            app,
            Some(at),
            &[
                Mouse::CursorMoved { position: at },
                Mouse::ButtonPressed(Button::Left),
                Mouse::ButtonReleased(Button::Left),
            ],
        )
        .messages
    }

    fn right_click(&mut self, app: &App, at: Point) -> Vec<Message> {
        self.pass(
            app,
            Some(at),
            &[
                Mouse::CursorMoved { position: at },
                Mouse::ButtonPressed(Button::Right),
                Mouse::ButtonReleased(Button::Right),
            ],
        )
        .messages
    }
}

fn apply(app: &mut App, messages: Vec<Message>) {
    for message in messages {
        let _ = app.update(message);
    }
}

/// Signed in, with an unsent drawing in front: the cloud is the status
/// bar's first control, before the chip and the message, and the command
/// bar has none. Tab goes from it to the chip. Its menu opens upward and its
/// items work from there; it also fits a 1040 px window.
#[tokio::test]
#[ignore = "Opt-in real renderer layout check"]
async fn the_github_button_leads_the_status_bar_and_its_menu_opens_upward() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    app.status = "Status for the test".into();
    for width in [1280., 1040.] {
        let mut ui = Ui::new(Size::new(width, 800.)).await;
        let window = ui.pass(&app, None, &[]);
        let button = window.node("status-github");
        assert_eq!(
            button.name,
            "GitHub · Signed in as @octo · Not everything is sent"
        );
        let chip = window.node("github-chip-place").bounds;
        let message = window.text("Status for the test");
        let preset = window.node("journal-preset").bounds;
        let cloud = button.bounds;
        assert!(cloud.y > 800. - 50., "in the status bar: {cloud:?}");
        assert!(cloud.x < 20., "at its left end: {cloud:?}");
        assert!(cloud.x + cloud.width <= chip.x, "{cloud:?} {chip:?}");
        assert!(chip.x + chip.width <= message.x, "{chip:?} {message:?}");
        assert!(message.x < preset.x, "{message:?} {preset:?}");
        assert!((cloud.center_y() - chip.center_y()).abs() < 1.);
        // The command bar has no GitHub button.
        assert!(
            !(window.nodes.iter())
                .any(|node| node.name.starts_with("GitHub") && node.bounds.y < 60.),
            "{width}"
        );
        // Tab: the side panel, then the cloud, the chip and the rest.
        let at = window.stop("status-github");
        assert!(window.stop("header-inspector") < at);
        assert_eq!(window.stop("github-chip-place"), at + 1);
        assert!(window.stop("journal-preset") > at + 1);
    }
    // Controls wider than the room, as a long theme name makes them: the
    // message gives way, the cloud keeps its size.
    let mut ui = Ui::new(Size::new(700., 800.)).await;
    let cloud = ui.pass(&app, None, &[]).node("status-github").bounds;
    assert!(
        cloud.x < 20. && cloud.width == 32. && cloud.height == 32.,
        "{cloud:?}"
    );

    let mut ui = Ui::new(Size::new(1280., 800.)).await;
    let cloud = ui.pass(&app, None, &[]).node("status-github").bounds;
    let opened = ui.click(&app, cloud.center());
    assert!(
        matches!(opened.as_slice(), [Message::GitHub(Action::Menu(true))]),
        "{opened:?}"
    );
    apply(&mut app, opened);
    let menu = ui.pass(&app, None, &[]);
    for id in [
        "github-open",
        "github-send",
        "github-versions",
        "github-repositories",
        "github-sign-out",
    ] {
        let item = menu.node(id).bounds;
        assert!(item.y + item.height <= cloud.y, "{id} above: {item:?}");
        assert!(item.x >= cloud.x - 1., "{id} from the left end: {item:?}");
    }
    // History, from the menu.
    let history = menu.node("github-versions").bounds.center();
    let sent = ui.click(&app, history);
    apply(&mut app, sent);
    assert_eq!(app.inspector_tab, InspectorTab::Versions);
    assert!(!app.github.menu);
    // Open from GitHub…
    let sent = ui.click(&app, cloud.center());
    apply(&mut app, sent);
    let open = ui.pass(&app, None, &[]).node("github-open").bounds.center();
    let sent = ui.click(&app, open);
    apply(&mut app, sent);
    assert!(matches!(app.github.dialog, Some(Dialog::Picker(_))));
    assert!(!app.github.menu);
}

/// Signed out, the button stays at the left end, says so, and its menu
/// offers Open from GitHub… and Sign in….
#[tokio::test]
#[ignore = "Opt-in real renderer layout check"]
async fn signed_out_the_github_button_stays_in_the_status_bar() {
    let dir = tempfile::tempdir().unwrap();
    let mut app = signed_out(dir.path());
    let mut ui = Ui::new(Size::new(1280., 800.)).await;
    let window = ui.pass(&app, None, &[]);
    let button = window.node("status-github");
    assert_eq!(button.name, "GitHub · Not signed in");
    let cloud = button.bounds;
    assert!(cloud.y > 800. - 50. && cloud.x < 20., "{cloud:?}");
    let sent = ui.click(&app, cloud.center());
    apply(&mut app, sent);
    let menu = ui.pass(&app, None, &[]);
    for id in ["github-open", "github-sign-in"] {
        let item = menu.node(id).bounds;
        assert!(item.y + item.height <= cloud.y, "{id} above: {item:?}");
    }
}

/// The theme editor replaces the status bar, button and chip with it; the
/// command palette still offers GitHub, and Send opens as a dialog.
#[tokio::test]
#[ignore = "Opt-in real renderer layout check"]
async fn the_theme_editor_leaves_github_to_the_palette() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, path) = app_in(dir.path());
    save_here(&mut app, &path);
    let _ = app.theme_generator_action(crate::app::theme_generator::Action::Open);
    let mut ui = Ui::new(Size::new(1280., 800.)).await;
    let window = ui.pass(&app, None, &[]);
    assert!(!window.has("status-github"));
    assert!(!window.has("github-chip-place"));
    let rows: Vec<_> = (app.github_palette().into_iter())
        .map(|(name, _, action)| (name, action.is_some()))
        .collect();
    for name in [OPEN_FROM, "Send 1 drawing", "History", "Sign out of GitHub"] {
        assert!(rows.contains(&(name.to_owned(), true)), "{name}: {rows:?}");
    }
    github(&mut app, Action::ShowSend);
    assert!(!app.send_on_chip());
    assert!(ui.pass(&app, None, &[]).has("github-send-confirm"));
}

/// Each repository row ends in a ↗ to its page, in its own room at the
/// right end and next in Tab order; a click there opens the page and not
/// the repository. The right-click menu has Open on GitHub ↗, and Remove
/// from list for a shared one.
#[tokio::test]
#[ignore = "Opt-in real renderer layout check"]
async fn repository_rows_open_their_page_on_github() {
    let dir = tempfile::tempdir().unwrap();
    let (mut app, _) = app_in(dir.path());
    app.github.ends = config::Endpoints {
        api: "https://api.github.com".into(),
        web: "https://github.com".into(),
        overridden: false,
    };
    app.github.prefs.share(&alice());
    github(&mut app, Action::Picker(picker::Action::Show(Mode::Open)));
    let account = gh::Account {
        login: "octo".into(),
        repos: vec![repo()],
    };
    let generation = app.github.generation;
    github(&mut app, Action::Listed(generation, Ok(account)));
    let mut ui = Ui::new(Size::new(1280., 800.)).await;
    let window = ui.pass(&app, None, &[]);
    for (at, repo, page) in [
        (0, repo(), "https://github.com/octo/lab"),
        (1, alice(), LIGANDS),
    ] {
        let row = window.node(&format!("github-repo-{at}"));
        let arrow = window.node(&format!("github-repo-page-{at}"));
        let name = format!("Open {}/{} on GitHub", repo.key.owner, repo.key.repo);
        assert_eq!(arrow.name, name);
        let (row, arrow) = (row.bounds, arrow.bounds);
        let end = row.x + row.width;
        assert!(arrow.x + arrow.width <= end && arrow.x + arrow.width >= end - 8.);
        assert!((arrow.center_y() - row.center_y()).abs() < 1.);
        assert_eq!(
            window.stop(&format!("github-repo-page-{at}")),
            window.stop(&format!("github-repo-{at}")) + 1
        );
        assert_eq!(app.github.repo_page(&repo.key), page);

        // A click on the ↗ opens the page only.
        let clicked = ui.click(&app, arrow.center());
        assert!(
            matches!(clicked.as_slice(), [Message::GitHub(Action::OpenRepo(key))] if *key == repo.key),
            "{clicked:?}"
        );
        // Right-click: the menu.
        let pressed = ui.right_click(&app, Point::new(row.x + 40., row.center_y()));
        apply(&mut app, pressed);
        assert_eq!(the_picker(&app).menu, Some(repo.id));
        let menu = ui.pass(&app, None, &[]);
        let open = menu.node("github-repo-open-on");
        assert_eq!(open.name, "Open on GitHub ↗");
        assert_eq!(menu.has("github-forget"), at == 1, "{at}");
        let chosen = ui.click(&app, open.bounds.center());
        assert!(
            matches!(chosen.as_slice(), [Message::GitHub(Action::OpenRepo(key))] if *key == repo.key),
            "{chosen:?}"
        );
        apply(&mut app, chosen);
        assert_eq!(the_picker(&app).menu, None);
    }

    // A name longer than the list is wide is cut off; the lock and the ↗ of
    // the selected row keep their room at the end.
    let long = Repo {
        id: 9,
        key: RepoKey {
            owner: "octo".into(),
            repo: "a-repository-name-much-longer-than-the-list-is-wide".into(),
        },
        ..repo()
    };
    app.github.repos.push(long.clone());
    let repos = app.github_picker_repos();
    if let Some(Dialog::Picker(picker)) = &mut app.github.dialog {
        picker.listed(&repos, None);
    }
    github(
        &mut app,
        Action::Picker(picker::Action::PickRepo(long.key.clone())),
    );
    let window = ui.pass(&app, None, &[]);
    let at = (window.nodes.iter())
        .position(|node| {
            node.id.starts_with("github-repo-") && node.name.starts_with("a-repository")
        })
        .map(|at| window.nodes[at].id.clone())
        .expect("the long row");
    let row = window.node(&at).bounds;
    let shown = (window.found.texts.iter())
        .find(|(text, bounds)| text == "↗" && (bounds.center_y() - row.center_y()).abs() < 1.)
        .expect("the selected row's ↗")
        .1;
    assert!((shown.width - 18.).abs() < 0.5, "{shown:?}");
    assert!(
        shown.x + shown.width <= row.x + row.width,
        "{shown:?} {row:?}"
    );
}
