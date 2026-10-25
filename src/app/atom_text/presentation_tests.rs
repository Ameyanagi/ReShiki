//! Real pointer, keyboard, IME and deadline verification.
use super::qa_ui::Ui;
use super::*;
use iced::advanced::{mouse, widget::Tree};
use iced::{Event, Point, Rectangle, Size};

fn key(character: char) -> Event {
    use iced::keyboard::{
        self, Key, Modifiers,
        key::{NativeCode, Physical},
    };
    Event::Keyboard(keyboard::Event::KeyPressed {
        key: Key::Character(character.to_string().into()),
        modified_key: Key::Character(character.to_string().into()),
        physical_key: Physical::Unidentified(NativeCode::Unidentified),
        location: keyboard::Location::Standard,
        modifiers: Modifiers::empty(),
        text: Some(character.to_string().into()),
        repeat: false,
    })
}

fn cropped(pixels: &[u8], size: Size, bounds: Rectangle) -> Vec<u8> {
    let mut result = Vec::new();
    for y in bounds.y.ceil() as usize..(bounds.y + bounds.height).floor() as usize {
        let start = (y * size.width as usize + bounds.x.ceil() as usize) * 4;
        let end = (y * size.width as usize + (bounds.x + bounds.width).floor() as usize) * 4;
        result.extend_from_slice(&pixels[start..end]);
    }
    result
}

fn prepare(ui: &mut Ui) -> (App, u64, Point, Vec<u8>) {
    let (mut app, _, target) = super::tests::remembered_methoxy();
    app.grid = false;
    app.appearance.mode = crate::appearance::Mode::Light;
    app.tab.fit_to_view = false;
    app.tab.camera.zoom = 2.;
    for _ in 0..4 {
        ui.redraw(
            &mut app,
            iced::time::Instant::now(),
            mouse::Cursor::Unavailable,
            false,
        );
    }
    let point = Point::new(ui.size.width / 2., ui.size.height / 2.);
    let bounds = ui.canvas.unwrap();
    assert!(bounds.contains(point));
    let world = app.tab.camera.center.offset(
        (point.x - bounds.x - app.viewport.width / 2.) / app.tab.camera.zoom,
        (point.y - bounds.y - app.viewport.height / 2.) / app.tab.camera.zoom,
    );
    app.tab.doc.atom_mut(target).unwrap().position = world;
    let pixels = ui.frame(
        &mut app,
        Event::Mouse(mouse::Event::CursorMoved { position: point }),
        mouse::Cursor::Available(point),
        false,
    );
    (app, target, point, pixels)
}

#[tokio::test]
#[ignore = "Opt-in real renderer pointer/focus/IME and 450 ms frame checks"]
async fn delayed_editor_real_pointer_focus_keyboard_and_ime() {
    let mut ui = Ui::new(Size::new(1040., 680.)).await;
    let (mut app, target, point, closed) = prepare(&mut ui);
    let before = app.tab.doc.clone();
    ui.click(&mut app, point);
    let deadline = app.tab.atom_text.as_ref().unwrap().present_after.unwrap();
    ui.redraw(
        &mut app,
        deadline - DOUBLE_CLICK_INTERVAL,
        mouse::Cursor::Available(point),
        true,
    );
    let editor = ui.editor.unwrap();
    assert!(
        Rectangle::new(
            Point::new(editor.x - 22., editor.y - 60.),
            Size::new(430., 500.)
        )
        .contains(point),
        "Target must lie under the hidden dialog"
    );
    for frame in 0..27 {
        let now = deadline - DOUBLE_CLICK_INTERVAL
            + std::time::Duration::from_nanos(frame * 1_000_000_000 / 60);
        let pixels = ui.redraw(&mut app, now, mouse::Cursor::Available(point), false);
        assert_eq!(
            cropped(&pixels, ui.size, editor),
            cropped(&closed, ui.size, editor),
            "No dialog ink at frame {frame}/60 s"
        );
        assert!(app.tab.atom_text.as_ref().unwrap().present_after.is_some());
    }
    // Keep gesture time deterministic despite real GPU frame costs. Both
    // clicks themselves go through the actual canvas and popup widgets.
    app.text_click = Some((iced::time::Instant::now(), target));
    ui.click(&mut app, point);
    assert!(
        app.tab.atom_text.is_none(),
        "Invisible dialog must not capture the second click"
    );
    assert_eq!(app.tab.doc.abbreviation(target).unwrap().label, "OMe");
    let _ = app.update(Message::Undo);
    assert_eq!(app.tab.doc, before);

    ui.tree = Tree::empty();
    let (mut app, _, point, _) = prepare(&mut ui);
    ui.click(&mut app, point);
    let deadline = app.tab.atom_text.as_ref().unwrap().present_after.unwrap();
    ui.redraw(
        &mut app,
        deadline - std::time::Duration::from_nanos(1),
        mouse::Cursor::Available(point),
        true,
    );
    assert!(app.tab.atom_text.as_ref().unwrap().present_after.is_some());
    ui.redraw(&mut app, deadline, mouse::Cursor::Available(point), false);
    assert!(app.tab.atom_text.as_ref().unwrap().present_after.is_none());
    ui.frame(&mut app, key('N'), mouse::Cursor::Unavailable, false);
    assert_eq!(
        app.tab.atom_text.as_ref().unwrap().input,
        "N",
        "Deadline reveal retains editor focus"
    );

    ui.tree = Tree::empty();
    let (mut app, _, point, _) = prepare(&mut ui);
    ui.click(&mut app, point);
    let deadline = app.tab.atom_text.as_ref().unwrap().present_after.unwrap();
    ui.redraw(
        &mut app,
        deadline - DOUBLE_CLICK_INTERVAL,
        mouse::Cursor::Available(point),
        true,
    );
    for character in ['O', 'M', 'e'] {
        ui.frame(&mut app, key(character), mouse::Cursor::Unavailable, false);
    }
    assert_eq!(
        app.tab.atom_text.as_ref().unwrap().input,
        "OMe",
        "First key and focus must survive reveal"
    );
    assert!(app.tab.atom_text.as_ref().unwrap().present_after.is_none());

    ui.tree = Tree::empty();
    let (mut app, _, point, _) = prepare(&mut ui);
    ui.click(&mut app, point);
    let deadline = app.tab.atom_text.as_ref().unwrap().present_after.unwrap();
    ui.redraw(
        &mut app,
        deadline - DOUBLE_CLICK_INTERVAL,
        mouse::Cursor::Available(point),
        true,
    );
    use iced::advanced::input_method::Event as Ime;
    ui.frame(
        &mut app,
        Event::InputMethod(Ime::Opened),
        mouse::Cursor::Unavailable,
        false,
    );
    assert!(app.tab.atom_text.as_ref().unwrap().present_after.is_some());
    ui.frame(
        &mut app,
        Event::InputMethod(Ime::Preedit("触".into(), Some(0..3))),
        mouse::Cursor::Unavailable,
        false,
    );
    assert!(app.tab.atom_text.as_ref().unwrap().present_after.is_none());
    ui.frame(
        &mut app,
        Event::InputMethod(Ime::Commit("触媒".into())),
        mouse::Cursor::Unavailable,
        false,
    );
    ui.frame(&mut app, key('a'), mouse::Cursor::Unavailable, false);
    assert_eq!(app.tab.atom_text.as_ref().unwrap().input, "触媒a");
}
