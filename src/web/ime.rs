//! Typing with an input method in the browser editor: Japanese, Chinese and
//! Korean composition, dead keys, the emoji picker and dictation.
//!
//! A canvas cannot take composed text, and winit's web backend has no input
//! method, so a hidden textarea takes it instead. While an Iced text widget
//! wants text, the textarea holds the focus at the widget's cursor, where the
//! browser places its candidate window. Its composition reaches Iced as the
//! input method events winit sends on the desktop (vendor/iced_winit), so the
//! widget shows the text being composed and inserts it when committed.
//!
//! Each key comes from one place only, to never type text twice: keys typed
//! outside a composition go on to the canvas as pressed and are cancelled in
//! the textarea, and keys that belong to a composition stay with the input
//! method.
use iced::Rectangle;
use iced_winit::web_ime;
use iced_winit::winit::event::Ime;
use std::cell::Cell;
use wasm_bindgen::{JsCast, prelude::*};

type Listener = Closure<dyn FnMut(web_sys::Event)>;

/// The textarea and its listeners, until `stop`.
pub(super) struct InputMethod {
    textarea: web_sys::HtmlTextAreaElement,
    textarea_listeners: Vec<(&'static str, Listener)>,
    window_listener: Listener,
}

/// The textarea's events that `start` handles.
const TEXTAREA_EVENTS: [&str; 8] = [
    "keydown",
    "keyup",
    "compositionstart",
    "compositionupdate",
    "compositionend",
    "beforeinput",
    "input",
    "blur",
];

thread_local! {
    /// Where the text widget that wants text has its cursor, if one does.
    static AREA: Cell<Option<Rectangle>> = const { Cell::new(None) };
}

fn canvas(element: &web_sys::Element) -> Option<web_sys::HtmlElement> {
    element
        .query_selector("canvas")
        .ok()
        .flatten()
        .and_then(|canvas| canvas.dyn_into().ok())
}

fn active_element() -> Option<web_sys::Element> {
    web_sys::window()?.document()?.active_element()
}

/// Whether `target` is `element`.
fn is(target: &web_sys::EventTarget, element: &impl AsRef<web_sys::EventTarget>) -> bool {
    target == element.as_ref()
}

/// Whether `element` has the page's focus.
fn has_focus(element: &impl AsRef<web_sys::EventTarget>) -> bool {
    active_element().is_some_and(|active| is(active.as_ref(), element))
}

/// Add the textarea to `element`, whose canvas the editor draws in.
pub(super) fn start(element: &web_sys::Element) -> Option<InputMethod> {
    let window = web_sys::window()?;
    let textarea: web_sys::HtmlTextAreaElement = window
        .document()?
        .create_element("textarea")
        .ok()?
        .dyn_into()
        .ok()?;
    for (name, value) in [
        ("autocomplete", "off"),
        ("autocorrect", "off"),
        ("autocapitalize", "off"),
        ("spellcheck", "false"),
        ("tabindex", "-1"),
        ("aria-hidden", "true"),
        // The editor's own key target, which its other key listeners serve.
        ("data-reshiki-keys", ""),
    ] {
        let _ = textarea.set_attribute(name, value);
    }
    // Through CSSOM, which a strict Content-Security-Policy allows, unlike a
    // style attribute.
    textarea.style().set_css_text(
        "position: fixed; left: 0; top: 0; width: 1px; height: 1px; margin: 0; \
         padding: 0; border: 0; outline: none; resize: none; overflow: hidden; \
         opacity: 0; pointer-events: none; color: transparent; \
         background: transparent; caret-color: transparent; white-space: pre",
    );
    element.append_child(&textarea).ok()?;

    // Iced says where text goes; the textarea follows, and leaves when no
    // widget wants text.
    let (hook_textarea, hook_element) = (textarea.clone(), element.clone());
    web_ime::on_input_method(move |area| {
        AREA.set(area);
        let Some(canvas) = canvas(&hook_element) else {
            return;
        };
        let focused = has_focus(&hook_textarea);
        match area {
            Some(area) => {
                place(&hook_textarea, &canvas, area);
                // Only when the editor has the focus, not another part of
                // the page.
                if !focused && has_focus(&canvas) {
                    let _ = hook_textarea.focus();
                }
            }
            None if focused => {
                let _ = canvas.focus();
            }
            None => {}
        }
    });

    // One closure per event: a key passed on to the canvas can move the focus
    // and fire the textarea's blur while the key's closure still runs.
    let mac = crate::platform::mac();
    let textarea_listeners: Vec<_> = TEXTAREA_EVENTS
        .into_iter()
        .map(|name| {
            let (listening, element) = (textarea.clone(), element.clone());
            let listener = Listener::new(move |event: web_sys::Event| {
                if let Some(canvas) = canvas(&element) {
                    textarea_event(&event, &listening, &canvas, mac);
                }
            });
            let _ =
                textarea.add_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
            (name, listener)
        })
        .collect();

    // Moving the focus from the canvas to the textarea is not leaving the
    // editor: winit must not hear the canvas's blur.
    let (suppressing, element_for_window) = (textarea.clone(), element.clone());
    let window_listener = Listener::new(move |event: web_sys::Event| {
        let Some(focus) = event.dyn_ref::<web_sys::FocusEvent>() else {
            return;
        };
        let from_canvas = canvas(&element_for_window)
            .zip(event.target())
            .is_some_and(|(canvas, target)| is(&target, &canvas));
        let to_textarea = focus
            .related_target()
            .is_some_and(|target| is(&target, &suppressing));
        if from_canvas && to_textarea {
            event.stop_immediate_propagation();
        }
    });
    let _ = window.add_event_listener_with_callback_and_bool(
        "blur",
        window_listener.as_ref().unchecked_ref(),
        true,
    );
    Some(InputMethod {
        textarea,
        textarea_listeners,
        window_listener,
    })
}

/// Handle one of the textarea's `TEXTAREA_EVENTS`.
fn textarea_event(
    event: &web_sys::Event,
    listening: &web_sys::HtmlTextAreaElement,
    canvas: &web_sys::HtmlElement,
    mac: bool,
) {
    match event.type_().as_str() {
        "keydown" | "keyup" => {
            let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
                return;
            };
            // The input method's keys, the key that ends a composition (229
            // in Chrome and Safari) and dead keys, which start one, stay
            // with it.
            if key.is_composing() || key.key_code() == 229 || key.key() == "Dead" {
                return;
            }
            key.prevent_default();
            forward(key, canvas, mac);
        }
        "compositionstart" => web_ime::send(Ime::Enabled),
        "compositionupdate" => {
            let text = composition_text(event);
            // The font arrives while the text is still being composed.
            super::text_drawn(&text);
            let end = text.len();
            web_ime::send(Ime::Preedit(text, Some((end, end))));
        }
        "compositionend" => {
            let text = composition_text(event);
            web_ime::send(Ime::Preedit(String::new(), None));
            if !text.is_empty() {
                super::text_drawn(&text);
                web_ime::send(Ime::Commit(text));
            }
            listening.set_value("");
        }
        // Text that comes without a key or a composition, such as from
        // the emoji picker or dictation.
        "beforeinput" => {
            let Some(input) = event.dyn_ref::<web_sys::InputEvent>() else {
                return;
            };
            if input.is_composing() {
                return;
            }
            input.prevent_default();
            if input.input_type() == "insertText"
                && let Some(text) = input.data().filter(|text| !text.is_empty())
            {
                super::text_drawn(&text);
                web_ime::send(Ime::Commit(text));
            }
        }
        "input" => {
            let composing = event
                .dyn_ref::<web_sys::InputEvent>()
                .is_some_and(web_sys::InputEvent::is_composing);
            if !composing {
                listening.set_value("");
            }
        }
        "blur" => {
            let to = event
                .dyn_ref::<web_sys::FocusEvent>()
                .and_then(web_sys::FocusEvent::related_target);
            if to.as_ref().is_some_and(|to| is(to, canvas)) {
                // A click on the canvas: once Iced has seen it, take
                // the focus back if the widget still wants text.
                let (textarea, canvas) = (listening.clone(), canvas.clone());
                let refocus = Closure::once_into_js(move || {
                    if AREA.get().is_some() && has_focus(&canvas) {
                        let _ = textarea.focus();
                    }
                });
                if let Some(window) = web_sys::window() {
                    let _ = window.set_timeout_with_callback(refocus.unchecked_ref());
                }
            } else {
                // The focus left the editor, which winit hears from
                // the canvas.
                let init = web_sys::FocusEventInit::new();
                init.set_related_target(to.as_ref());
                if let Ok(blur) = web_sys::FocusEvent::new_with_focus_event_init_dict("blur", &init)
                {
                    let _ = canvas.dispatch_event(&blur);
                }
            }
        }
        _ => {}
    }
}

/// Remove the textarea and its listeners.
pub(super) fn stop(input: InputMethod) {
    web_ime::clear_input_method();
    AREA.set(None);
    for (name, listener) in &input.textarea_listeners {
        let _ = input
            .textarea
            .remove_event_listener_with_callback(name, listener.as_ref().unchecked_ref());
    }
    if let Some(window) = web_sys::window() {
        let _ = window.remove_event_listener_with_callback_and_bool(
            "blur",
            input.window_listener.as_ref().unchecked_ref(),
            true,
        );
    }
    input.textarea.remove();
}

/// Put the textarea over the cursor, `area` in the canvas's logical pixels,
/// so the browser shows its candidate window there.
fn place(textarea: &web_sys::HtmlTextAreaElement, canvas: &web_sys::HtmlElement, area: Rectangle) {
    let canvas = canvas.get_bounding_client_rect();
    let style = textarea.style();
    let height = area.height.max(1.);
    for (name, value) in [
        ("left", canvas.left() + f64::from(area.x)),
        ("top", canvas.top() + f64::from(area.y)),
        ("height", f64::from(height)),
        ("font-size", f64::from(height * 0.8)),
        ("line-height", f64::from(height)),
    ] {
        let _ = style.set_property(name, &format!("{value}px"));
    }
}

fn composition_text(event: &web_sys::Event) -> String {
    event
        .dyn_ref::<web_sys::CompositionEvent>()
        .and_then(web_sys::CompositionEvent::data)
        .unwrap_or_default()
}

/// Press `key` on the canvas as it was pressed in the textarea. On a Mac the
/// ⌘ swap (see `command_key`) already traded its ⌘ and Control, and trades
/// them again on the canvas, so they go as pressed.
fn forward(key: &web_sys::KeyboardEvent, canvas: &web_sys::HtmlElement, mac: bool) {
    let (control, command) = if mac {
        (key.meta_key(), key.ctrl_key())
    } else {
        (key.ctrl_key(), key.meta_key())
    };
    let init = web_sys::KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_key(&key.key());
    init.set_code(&key.code());
    init.set_location(key.location());
    init.set_repeat(key.repeat());
    init.set_ctrl_key(control);
    init.set_meta_key(command);
    init.set_shift_key(key.shift_key());
    init.set_alt_key(key.alt_key());
    if let Ok(forwarded) =
        web_sys::KeyboardEvent::new_with_keyboard_event_init_dict(&key.type_(), &init)
    {
        let _ = canvas.dispatch_event(&forwarded);
    }
}
