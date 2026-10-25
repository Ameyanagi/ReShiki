//! The editor in the browser. A page calls `mount` with an element to start
//! the editor in it, then loads, saves and exports the drawing in front and
//! hears about its edits through the functions below. The npm package
//! (packages/editor) wraps them as an editor handle.
//!
//! The page and the editor share one thread. Requests reach the app through
//! a channel its subscription drains; drawings come back through one-shot
//! replies, and edits are reported after each update.
use crate::app::{App, Message};
use iced::futures::{
    StreamExt,
    channel::{mpsc, oneshot},
    stream,
};
use iced::{Subscription, Task};
use reshiki::{document::Document, formats};
use std::{cell::RefCell, collections::HashMap, path::Path};
use wasm_bindgen::{JsCast, prelude::*};

mod ime;
mod storage;

/// What the page asks of the running editor.
#[derive(Debug, Clone)]
pub(crate) enum Request {
    /// Replace the drawing in front.
    Load(Box<Document>),
    /// Hand the drawing in front to the waiting reply with this number.
    Snapshot(u64),
    /// Stop the editor.
    Close,
    /// Draw again, as after a font arrives.
    Redraw,
    /// Add fonts kept from earlier visits, by SHA-256, which text needs.
    AddFonts(Vec<(String, Vec<u8>)>),
    /// A file in browser storage changed under the app: another tab kept a
    /// change, or one of this tab's own was not kept, with why.
    Stored {
        key: String,
        failure: Option<String>,
    },
    /// This many files are dragged over the editor, or none once they leave.
    /// Browsers name dragged files only when they are dropped.
    Drag(usize),
    /// Files dropped on the editor, before they are read, so the import
    /// belongs to the drawing they were dropped on.
    Drop(Vec<web_sys::File>),
}

type DragListener = Closure<dyn FnMut(web_sys::DragEvent)>;

#[derive(Default)]
struct Host {
    requests: Option<mpsc::UnboundedSender<Request>>,
    /// Until the app's subscription takes it.
    receiver: Option<mpsc::UnboundedReceiver<Request>>,
    replies: HashMap<u64, oneshot::Sender<Result<Document, String>>>,
    next_reply: u64,
    on_change: Option<js_sys::Function>,
    /// Called with "ja", "ko" or "zh", once for each, the first time text in
    /// that language has characters no loaded font draws.
    on_missing_font: Option<js_sys::Function>,
    /// The languages `on_missing_font` was called with, and whether the
    /// page has finished answering each.
    fonts_asked: Vec<(&'static str, bool)>,
    /// The CJK characters `text_drawn` has looked up, with the font count and
    /// answered requests they were looked up with.
    checked: ((usize, usize), std::collections::HashSet<char>),
    revision: Option<(u64, u64)>,
    /// Whether any tab has unsaved changes, as of the last update.
    dirty: bool,
    on_dirty_change: Option<js_sys::Function>,
    /// The window title, `Name • — ReShiki`, as of the last update.
    title: Option<String>,
    on_title_change: Option<js_sys::Function>,
    /// Whether the title becomes the page's `document.title`.
    sync_title: bool,
    /// winit runs one event loop per page, even after it exits.
    mounted: bool,
    /// The listener that trades ⌘ and Control on a Mac, until `destroy`.
    command_key: Option<Closure<dyn FnMut(web_sys::Event)>>,
    /// The clipboard handoff for canvas drawing shortcuts, until `destroy`.
    canvas_paste: Option<PasteListeners>,
    /// The element's file drop listener, until `destroy`.
    drops: Option<(web_sys::Element, DragListener)>,
    /// The textarea that takes input method text, until `destroy`.
    input_method: Option<ime::InputMethod>,
}

thread_local! {
    static HOST: RefCell<Host> = RefCell::default();
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn error(message: &str);
}

/// Report panics on the console; the browser build aborts on them.
pub(crate) fn start() {
    std::panic::set_hook(Box::new(|info| error(&info.to_string())));
}

fn js(message: impl std::fmt::Display) -> JsError {
    JsError::new(&message.to_string())
}

/// Start the editor in `element`, which it fills, showing `data` read as
/// `format` when given. `on_missing_font` is called with "ja", "ko" or "zh",
/// once for each, the first time text in that language has characters no
/// loaded font draws. With `sync_title`, the page's title follows the
/// document's, as the desktop window's does. A page can mount one editor; put
/// further editors in iframes.
#[wasm_bindgen]
pub async fn mount(
    element: web_sys::Element,
    data: Option<Vec<u8>>,
    format: Option<String>,
    on_missing_font: Option<js_sys::Function>,
    sync_title: bool,
) -> Result<(), JsError> {
    const TARGET: &str = "reshiki-editor-canvas";
    if HOST.with_borrow(|host| host.mounted) {
        return Err(js(
            "ReShiki is already mounted on this page; use an iframe for another editor",
        ));
    }
    // Read the drawing first, so data that cannot be read leaves the page free
    // to mount again.
    let opened = match data {
        Some(data) => Some(
            formats::read(data, format.as_deref().unwrap_or("auto"))
                .await
                .map_err(js)?,
        ),
        None => None,
    };
    // iced replaces the element with this id by its canvas.
    let document = element
        .owner_document()
        .ok_or_else(|| js("The element has no page"))?;
    let target = document
        .create_element("div")
        .map_err(|_| js("Could not create the editor canvas"))?;
    target.set_id(TARGET);
    element
        .append_child(&target)
        .map_err(|_| js("Could not add the editor canvas"))?;
    // iced looks the placeholder up from the page's body, which cannot see
    // into shadow roots or detached elements.
    let found = document
        .body()
        .and_then(|body| body.query_selector(&format!("#{TARGET}")).ok().flatten());
    if found.as_ref() != Some(&target) {
        target.remove();
        return Err(js(
            "Mount the editor on an element in the page's body, outside shadow roots",
        ));
    }
    let (sender, receiver) = mpsc::unbounded();
    if let Some(doc) = opened {
        let _ = sender.unbounded_send(Request::Load(Box::new(doc)));
    }
    HOST.with_borrow_mut(|host| {
        *host = Host {
            requests: Some(sender),
            receiver: Some(receiver),
            mounted: true,
            on_missing_font,
            sync_title,
            ..Host::default()
        }
    });
    ask_before_unload();
    // Before the ⌘ swap, so it sees the keys as pressed.
    let paste = canvas_paste(&element);
    HOST.with_borrow_mut(|host| host.canvas_paste = paste);
    if crate::platform::mac() {
        let swap = command_key(&element);
        HOST.with_borrow_mut(|host| host.command_key = swap);
    }
    let drops = file_drops(&element);
    let input_method = ime::start(&element);
    HOST.with_borrow_mut(|host| {
        host.drops = Some((element, drops));
        host.input_method = input_method;
    });
    // iced's multisampling leaves most canvases blank on WebGL2, the fallback
    // when the browser offers no WebGPU adapter.
    let antialiasing = webgpu().await;
    // Settings, templates, themes and fonts kept from earlier visits, before
    // the app reads them.
    storage::open().await;
    crate::run(
        crate::session::Arguments::default(),
        iced::window::Settings {
            platform_specific: iced::window::settings::PlatformSpecific {
                target: Some(TARGET.into()),
            },
            ..Default::default()
        },
        antialiasing,
    )
    .map_err(js)
}

/// Ask before the page unloads with unsaved changes. The browser never sends
/// the window a close request, so the desktop's save dialog cannot run. The
/// listener lives as long as the page; after `destroy` nothing is dirty.
fn ask_before_unload() {
    let ask = Closure::<dyn FnMut(web_sys::Event)>::new(|event: web_sys::Event| {
        if HOST.with_borrow(|host| host.dirty) {
            // Browsers show their own "Leave site?" text.
            event.prevent_default();
            if let Some(event) = event.dyn_ref::<web_sys::BeforeUnloadEvent>() {
                event.set_return_value("");
            }
        }
    });
    if let Some(window) = web_sys::window() {
        let _ =
            window.add_event_listener_with_callback("beforeunload", ask.as_ref().unchecked_ref());
    }
    ask.forget();
}

/// The system the browser runs on, as `navigator.userAgentData.platform`
/// ("macOS", "Windows") or else `navigator.platform` ("MacIntel", "Win32")
/// names it.
pub(crate) fn platform_name() -> String {
    let Some(navigator) = web_sys::window().map(|window| window.navigator()) else {
        return String::new();
    };
    js_sys::Reflect::get(&navigator, &"userAgentData".into())
        .ok()
        .filter(|data| data.is_object())
        .and_then(|data| js_sys::Reflect::get(&data, &"platform".into()).ok())
        .and_then(|platform| platform.as_string())
        .filter(|platform| !platform.is_empty())
        .or_else(|| navigator.platform().ok())
        .unwrap_or_default()
}

/// `canvas_paste`'s listeners, one per event, as the paste listener's key
/// reaches the key listener while it runs.
type PasteListeners = [(&'static str, Closure<dyn FnMut(web_sys::Event)>); 2];

thread_local! {
    /// A Ctrl/⌘+V press held back until the page has the clipboard's text.
    static HELD_PASTE: RefCell<Option<web_sys::KeyboardEvent>> = const { RefCell::new(None) };
    /// While `relay_key` hands a held key to the editor.
    static RELAYING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Keep the canvas's Ctrl/⌘+V clipboard handoff. The native paste event
/// supplies clipboard text where available; otherwise a browser API read
/// precedes the drawing shortcut. Text widgets use the input-method textarea's
/// native paste event instead and never replay a key (see `ime`).
fn canvas_paste(element: &web_sys::Element) -> Option<PasteListeners> {
    let window = web_sys::window()?;
    let element = element.clone();
    let mac = crate::platform::mac();
    let paste = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        let Some(paste) = event.dyn_ref::<web_sys::ClipboardEvent>() else {
            return;
        };
        let Some(key) = HELD_PASTE.take() else {
            return;
        };
        // The canvas handles the drawing shortcut, not native text insertion.
        paste.prevent_default();
        let text = paste
            .clipboard_data()
            .and_then(|data| data.get_data("text/plain").ok())
            .filter(|text| !text.is_empty());
        if let Some(text) = &text {
            text_drawn(text);
        }
        window_clipboard::set_contents(text);
        relay_key(&key);
    });
    let keys = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        if RELAYING.get() {
            return;
        }
        let Some(key) = event.dyn_ref::<web_sys::KeyboardEvent>() else {
            return;
        };
        let command = if mac { key.meta_key() } else { key.ctrl_key() };
        // Shift+V is the drawing's flip, which must not read the clipboard.
        if !command
            || key.alt_key()
            || key.shift_key()
            || !is_v(key)
            || !editor_keys(&event, &element)
        {
            return;
        }
        // A real editable target can receive the browser's native paste.
        // Replaying V after an asynchronous clipboard permission prompt can
        // reach Iced before its modifier update and type a literal "v".
        if key
            .target()
            .is_some_and(|target| target.is_instance_of::<web_sys::HtmlTextAreaElement>())
        {
            return;
        }
        // winit would cancel the key, and with it the paste event.
        key.stop_immediate_propagation();
        HELD_PASTE.set(Some(key.clone()));
        // Browsers that send no paste event here get the key next turn,
        // with the text read through the Clipboard API where allowed.
        let fallback = Closure::once_into_js(|| {
            if let Some(key) = HELD_PASTE.take() {
                wasm_bindgen_futures::spawn_local(async move {
                    window_clipboard::set_contents(read_clipboard_text().await);
                    relay_key(&key);
                });
            }
        });
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback(fallback.unchecked_ref());
        }
    });
    let listeners = [("keydown", keys), ("paste", paste)];
    for (name, listener) in &listeners {
        // Capture runs before winit's listeners and the ⌘ swap.
        let _ = window.add_event_listener_with_callback_and_bool(
            name,
            listener.as_ref().unchecked_ref(),
            true,
        );
    }
    Some(listeners)
}

/// Whether `event` is for the editor's own keys: its canvas, or an element
/// of the editor marked `data-reshiki-keys`, and not a page control placed
/// inside the editor's element.
fn editor_keys(event: &web_sys::Event, element: &web_sys::Element) -> bool {
    let Some(target) = event
        .target()
        .and_then(|target| target.dyn_into::<web_sys::Element>().ok())
    else {
        return false;
    };
    let canvas = element.query_selector("canvas").ok().flatten();
    canvas.as_ref() == Some(&target)
        || (target.has_attribute("data-reshiki-keys") && element.contains(Some(&target)))
}

/// Whether `key` is V, by the character it types on a Latin layout or else
/// where it sits, as iced reads shortcuts.
fn is_v(key: &web_sys::KeyboardEvent) -> bool {
    let typed = key.key();
    let mut chars = typed.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphabetic() => c.eq_ignore_ascii_case(&'v'),
        _ => key.code() == "KeyV",
    }
}

/// The clipboard's text through the Clipboard API, when the page may read it.
async fn read_clipboard_text() -> Option<String> {
    let clipboard = js_sys::Reflect::get(&web_sys::window()?.navigator(), &"clipboard".into())
        .ok()?
        .dyn_into::<web_sys::Clipboard>()
        .ok()?;
    wasm_bindgen_futures::JsFuture::from(clipboard.read_text())
        .await
        .ok()?
        .as_string()
        .filter(|text| !text.is_empty())
}

/// Hand a held key press to the editor's canvas as pressed.
fn relay_key(key: &web_sys::KeyboardEvent) {
    let canvas = HOST
        .with_borrow(|host| host.drops.as_ref().map(|(element, _)| element.clone()))
        .and_then(|element| element.query_selector("canvas").ok().flatten());
    let Some(canvas) = canvas else {
        return;
    };
    // The read may have waited for browser permission. Do not deliver its
    // drawing shortcut to a text widget or another page control focused since.
    let focus = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.active_element());
    if focus.as_ref() != Some(&canvas) {
        return;
    }
    let init = web_sys::KeyboardEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_key(&key.key());
    init.set_code(&key.code());
    init.set_location(key.location());
    init.set_repeat(key.repeat());
    init.set_ctrl_key(key.ctrl_key());
    init.set_meta_key(key.meta_key());
    init.set_shift_key(key.shift_key());
    init.set_alt_key(key.alt_key());
    let Ok(relayed) = web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init)
    else {
        return;
    };
    RELAYING.set(true);
    let _ = canvas.dispatch_event(&relayed);
    RELAYING.set(false);
}

/// The events whose ⌘ and Control `command_key` trades.
const SWAPPED_EVENTS: [&str; 15] = [
    "keydown",
    "keyup",
    "pointerdown",
    "pointermove",
    "pointerup",
    "pointercancel",
    "pointerover",
    "pointerout",
    "wheel",
    "mousedown",
    "mousemove",
    "mouseup",
    "click",
    "dblclick",
    "contextmenu",
];

/// Make ⌘ the Command key on a Mac, as in the desktop app. iced decides at
/// build time that Command is Control everywhere but macOS, and a browser
/// build is never macOS, so the editor's key and pointer events trade ⌘ and
/// Control before winit reads them (see platform.rs). Control-click is the
/// Mac's secondary click, so it reaches the editor as a right-click.
fn command_key(element: &web_sys::Element) -> Option<Closure<dyn FnMut(web_sys::Event)>> {
    use std::cell::Cell;
    let window = web_sys::window()?;
    let element = element.clone();
    // The pointer whose Control-click became a right-click, until it lifts.
    let secondary: Cell<Option<JsValue>> = Cell::new(None);
    let swap = Closure::<dyn FnMut(web_sys::Event)>::new(move |event: web_sys::Event| {
        let inside = event
            .target()
            .and_then(|target| target.dyn_into::<web_sys::Node>().ok())
            .is_some_and(|target| element.contains(Some(&target)));
        let get = |name: &str| js_sys::Reflect::get(&event, &name.into()).ok();
        let flag = |name: &str| get(name).and_then(|value| value.as_bool());
        let (true, Some(control), Some(command)) = (inside, flag("ctrlKey"), flag("metaKey"))
        else {
            return;
        };
        let set = |name: &str, value: JsValue| {
            let descriptor = js_sys::Object::new();
            let _ = js_sys::Reflect::set(&descriptor, &"value".into(), &value);
            js_sys::Object::define_property(&event, &name.into(), &descriptor);
        };
        let kind = event.type_();
        if kind.starts_with("pointer") {
            let pointer = get("pointerId").unwrap_or(JsValue::NULL);
            let button = get("button").and_then(|value| value.as_f64());
            let buttons = get("buttons")
                .and_then(|value| value.as_f64())
                .unwrap_or(0.) as u32;
            let mouse = get("pointerType").and_then(|value| value.as_string());
            let held = secondary.take();
            let ours = held.as_ref() == Some(&pointer);
            if kind == "pointerdown"
                && control
                && button == Some(0.)
                && mouse.as_deref() == Some("mouse")
            {
                set("button", 2.into());
                set("buttons", ((buttons & !1) | 2).into());
                secondary.set(Some(pointer));
            } else if ours {
                if button == Some(0.) {
                    set("button", 2.into());
                }
                if buttons & 1 != 0 {
                    set("buttons", ((buttons & !1) | 2).into());
                }
                if !matches!(kind.as_str(), "pointerup" | "pointercancel") {
                    secondary.set(held);
                }
            } else {
                secondary.set(held);
            }
        }
        set("ctrlKey", command.into());
        set("metaKey", control.into());
    });
    for name in SWAPPED_EVENTS {
        // Capture runs before winit's listeners on the canvas.
        let _ = window.add_event_listener_with_callback_and_bool(
            name,
            swap.as_ref().unchecked_ref(),
            true,
        );
    }
    Some(swap)
}

/// The drag events `file_drops` handles.
const DRAG_EVENTS: [&str; 4] = ["dragenter", "dragover", "dragleave", "drop"];

/// Take files dropped on `element` into the editor, which opens or inserts
/// them as the desktop app does. The browser would otherwise show or
/// download the file in place of the page.
fn file_drops(element: &web_sys::Element) -> DragListener {
    use std::cell::Cell;
    // dragenter and dragleave also fire on moving between the element's
    // children; the files leave when the count is back at zero.
    let inside = Cell::new(0u32);
    let drops = DragListener::new(move |event: web_sys::DragEvent| {
        let Some(transfer) = event.data_transfer() else {
            return;
        };
        if !transfer.types().includes(&"Files".into(), 0) {
            return;
        }
        event.prevent_default();
        match event.type_().as_str() {
            "dragenter" => {
                if inside.get() == 0 {
                    let files = transfer.items().length() as usize;
                    let _ = send(Request::Drag(files.max(1)));
                }
                inside.set(inside.get() + 1);
            }
            "dragover" => transfer.set_drop_effect("copy"),
            "dragleave" => {
                inside.set(inside.get().saturating_sub(1));
                if inside.get() == 0 {
                    let _ = send(Request::Drag(0));
                }
            }
            "drop" => {
                inside.set(0);
                let _ = send(Request::Drag(0));
                let files = transfer
                    .files()
                    .map(|list| files(&list))
                    .unwrap_or_default();
                let _ = send(Request::Drop(files));
            }
            _ => {}
        }
    });
    for name in DRAG_EVENTS {
        let _ = element.add_event_listener_with_callback(name, drops.as_ref().unchecked_ref());
    }
    drops
}

fn files(list: &web_sys::FileList) -> Vec<web_sys::File> {
    (0..list.length()).filter_map(|i| list.get(i)).collect()
}

/// The files' names and contents, or which one the browser could not read.
/// Each is read up to `limit`, as on the desktop: a larger file is cut one
/// byte past it, so its loader refuses it with the usual size message.
pub(crate) async fn read_files(
    files: Vec<web_sys::File>,
    limit: fn(&Path) -> u64,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut read = Vec::with_capacity(files.len());
    for file in files {
        let failed = || format!("Could not read {}", file.name());
        let size = file.size() as u64;
        let length = crate::app::read_length(size, limit(Path::new(&file.name())));
        let blob: web_sys::Blob = if length < size {
            file.slice_with_f64_and_f64(0., length as f64)
                .map_err(|_| failed())?
        } else {
            file.clone().into()
        };
        let buffer = wasm_bindgen_futures::JsFuture::from(blob.array_buffer())
            .await
            .map_err(|_| failed())?;
        read.push((file.name(), js_sys::Uint8Array::new(&buffer).to_vec()));
    }
    Ok(read)
}

/// Files the user chooses with the browser's file chooser, by name with their
/// contents; none when they cancel, and an error when one cannot be read. `extensions`, without dots, limit the
/// choice when given. The File System Access picker is used where the browser
/// has it, and otherwise a hidden file input.
pub(crate) async fn pick(
    extensions: &[&str],
    multiple: bool,
    limit: fn(&Path) -> u64,
) -> Result<Vec<(String, Vec<u8>)>, String> {
    let Some(window) = web_sys::window() else {
        return Ok(vec![]);
    };
    let files = match open_file_picker(&window, extensions, multiple).await {
        Some(files) => files?,
        None => file_input(&window, extensions, multiple).await,
    };
    read_files(files, limit).await
}

/// `showOpenFilePicker`, or None when the page cannot use it, as in Firefox,
/// Safari and cross-origin frames.
async fn open_file_picker(
    window: &web_sys::Window,
    extensions: &[&str],
    multiple: bool,
) -> Option<Result<Vec<web_sys::File>, String>> {
    use js_sys::{Array, Function, Object, Promise, Reflect};
    use wasm_bindgen_futures::JsFuture;
    let picker = Reflect::get(window, &"showOpenFilePicker".into())
        .ok()?
        .dyn_into::<Function>()
        .ok()?;
    let options = Object::new();
    let _ = Reflect::set(&options, &"multiple".into(), &multiple.into());
    // The picker refuses extensions longer than 16 characters or with other
    // characters than letters, digits, + and .; the choice is then unfiltered.
    let valid = |extension: &&str| {
        extension.len() < 16
            && extension
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.'))
    };
    if !extensions.is_empty() && extensions.iter().all(valid) {
        let accept = Object::new();
        let suffixes: Array = extensions
            .iter()
            .map(|extension| JsValue::from(format!(".{extension}")))
            .collect();
        let _ = Reflect::set(&accept, &"application/octet-stream".into(), &suffixes);
        let kind = Object::new();
        let _ = Reflect::set(&kind, &"accept".into(), &accept);
        let _ = Reflect::set(&options, &"types".into(), &Array::of1(&kind));
    }
    let picked = picker
        .call1(window, &options)
        .ok()?
        .dyn_into::<Promise>()
        .ok()?;
    let handles = match JsFuture::from(picked).await {
        Ok(handles) => Array::from(&handles),
        // Dismissing the picker rejects with an AbortError; other errors
        // leave the choice to the file input.
        Err(thrown) => {
            let name = Reflect::get(&thrown, &"name".into()).ok()?.as_string();
            return (name.as_deref() == Some("AbortError")).then(|| Ok(Vec::new()));
        }
    };
    let mut files = vec![];
    for handle in handles.iter() {
        let name = Reflect::get(&handle, &"name".into())
            .ok()
            .and_then(|name| name.as_string())
            .unwrap_or_default();
        let file = Reflect::get(&handle, &"getFile".into())
            .ok()
            .and_then(|get| get.dyn_into::<Function>().ok())
            .and_then(|get| get.call0(&handle).ok())
            .and_then(|file| file.dyn_into::<Promise>().ok());
        let file = match file {
            Some(file) => JsFuture::from(file)
                .await
                .ok()
                .and_then(|file| file.dyn_into::<web_sys::File>().ok()),
            None => None,
        };
        match file {
            Some(file) => files.push(file),
            None => return Some(Err(format!("Could not read {name}"))),
        }
    }
    Some(Ok(files))
}

/// A hidden `<input type=file>`, answered by its change or cancel event.
async fn file_input(
    window: &web_sys::Window,
    extensions: &[&str],
    multiple: bool,
) -> Vec<web_sys::File> {
    let Some((document, body)) = window
        .document()
        .and_then(|document| Some((document.clone(), document.body()?)))
    else {
        return vec![];
    };
    let Some(input) = document
        .create_element("input")
        .ok()
        .and_then(|input| input.dyn_into::<web_sys::HtmlInputElement>().ok())
    else {
        return vec![];
    };
    input.set_type("file");
    input.set_multiple(multiple);
    input.set_hidden(true);
    if !extensions.is_empty() {
        let accept: Vec<_> = extensions.iter().map(|e| format!(".{e}")).collect();
        input.set_accept(&accept.join(","));
    }
    if body.append_child(&input).is_err() {
        return vec![];
    }
    let (sender, receiver) = oneshot::channel::<()>();
    let sender = std::rc::Rc::new(RefCell::new(Some(sender)));
    let answered = Closure::<dyn FnMut()>::new(move || {
        if let Some(sender) = sender.borrow_mut().take() {
            let _ = sender.send(());
        }
    });
    for name in ["change", "cancel"] {
        let _ = input.add_event_listener_with_callback(name, answered.as_ref().unchecked_ref());
    }
    input.click();
    let _ = receiver.await;
    let chosen = input.files().map(|list| files(&list)).unwrap_or_default();
    input.remove();
    drop(answered);
    chosen
}

/// Whether the browser offers a WebGPU adapter, which wgpu prefers to WebGL2.
async fn webgpu() -> bool {
    let Some(gpu) = web_sys::window()
        .and_then(|window| js_sys::Reflect::get(&window.navigator(), &"gpu".into()).ok())
        .filter(|gpu| gpu.is_object())
    else {
        return false;
    };
    let Some(adapter) = js_sys::Reflect::get(&gpu, &"requestAdapter".into())
        .ok()
        .and_then(|request| request.dyn_into::<js_sys::Function>().ok())
        .and_then(|request| request.call0(&gpu).ok())
        .and_then(|promise| promise.dyn_into::<js_sys::Promise>().ok())
    else {
        return false;
    };
    wasm_bindgen_futures::JsFuture::from(adapter)
        .await
        .is_ok_and(|adapter| adapter.is_object())
}

fn send(request: Request) -> Result<(), JsError> {
    HOST.with_borrow(|host| {
        host.requests
            .as_ref()
            .and_then(|requests| requests.unbounded_send(request).ok())
            .ok_or_else(|| js("The editor is not running"))
    })
}

/// Replace the drawing in front with `data` read as `format`: `rsk`, `mol`,
/// `smiles`, `inchi`, `cdxml`, `cdx`, `rxn`, `rsmi` or `auto`.
#[wasm_bindgen]
pub async fn load(data: Vec<u8>, format: String) -> Result<(), JsError> {
    let doc = formats::read(data, &format).await.map_err(js)?;
    send(Request::Load(Box::new(doc)))
}

/// The drawing in front as `format`: `rsk`, `mol`, `smiles`, `inchi`, `cdxml`,
/// `cdx`, `rxn`, `rsmi`, or a figure (`svg`, `png` or `pdf`) as Export makes it.
#[wasm_bindgen]
pub async fn save(format: String) -> Result<Vec<u8>, JsError> {
    let (sender, receiver) = oneshot::channel();
    let id = HOST.with_borrow_mut(|host| {
        host.next_reply += 1;
        host.replies.insert(host.next_reply, sender);
        host.next_reply
    });
    send(Request::Snapshot(id))?;
    let doc = receiver
        .await
        .map_err(|_| js("The editor stopped"))?
        .map_err(js)?;
    formats::write(doc, &format).await.map_err(js)
}

/// Call `callback` after each edit to the drawing in front, or stop with none.
#[wasm_bindgen(js_name = onChange)]
pub fn on_change(callback: Option<js_sys::Function>) {
    HOST.with_borrow_mut(|host| host.on_change = callback);
}

/// Whether any tab has unsaved changes.
#[wasm_bindgen(js_name = isDirty)]
pub fn is_dirty() -> bool {
    HOST.with_borrow(|host| host.dirty)
}

/// Call `callback` with `true` when a tab gains unsaved changes and `false`
/// when none has any left, or stop with none.
#[wasm_bindgen(js_name = onDirtyChange)]
pub fn on_dirty_change(callback: Option<js_sys::Function>) {
    HOST.with_borrow_mut(|host| host.on_dirty_change = callback);
}

/// Call `callback` with the title, `Name • — ReShiki` while it has unsaved
/// changes, whenever it changes, or stop with none.
#[wasm_bindgen(js_name = onTitleChange)]
pub fn on_title_change(callback: Option<js_sys::Function>) {
    HOST.with_borrow_mut(|host| host.on_title_change = callback);
}

/// Run heavy jobs, so far naming, in workers that `factory` returns: a new
/// module `Worker` running `worker.js` each time, made on first use.
#[wasm_bindgen(js_name = setWorker)]
pub fn set_worker(factory: js_sys::Function) {
    reshiki::naming::set_worker(factory);
}

fn call(callback: &js_sys::Function, argument: &JsValue, name: &str) {
    if let Err(thrown) = callback.call1(&JsValue::NULL, argument) {
        error(&format!("The {name} callback failed: {thrown:?}"));
    }
}

/// Add a TrueType or OpenType font (or collection) for drawing, measuring and
/// export, before or after `mount`. Text whose own font lacks a glyph falls
/// back to it. The font is kept in browser storage and added again on a later
/// visit once drawn text lacks a glyph; adding the same font again changes
/// nothing. Returns how many faces it holds.
#[wasm_bindgen(js_name = addFont)]
pub fn add_font(bytes: Vec<u8>) -> Result<usize, JsError> {
    let faces = storage::add_font(bytes).map_err(js)?;
    // Before mount the first frame has it anyway.
    let _ = send(Request::Redraw);
    Ok(faces)
}

/// The language whose font draws `c`, for the Chinese, Japanese and Korean
/// scripts; ideographs go to Japanese first.
fn cjk_language(c: char) -> Option<&'static str> {
    match c {
        '\u{1100}'..='\u{11FF}' // Hangul jamo
        | '\u{3130}'..='\u{318F}' // Hangul compatibility jamo
        | '\u{A960}'..='\u{A97F}' // Hangul jamo extended A
        | '\u{AC00}'..='\u{D7FF}' => Some("ko"), // Hangul syllables, jamo extended B
        '\u{3100}'..='\u{312F}' | '\u{31A0}'..='\u{31BF}' => Some("zh"), // Bopomofo
        '\u{3000}'..='\u{30FF}' // CJK punctuation, hiragana, katakana
        | '\u{31F0}'..='\u{31FF}' // katakana phonetic extensions
        | '\u{3400}'..='\u{4DBF}' // CJK extension A
        | '\u{4E00}'..='\u{9FFF}' // CJK unified ideographs
        | '\u{F900}'..='\u{FAFF}' // CJK compatibility ideographs
        | '\u{FF00}'..='\u{FFEF}' => Some("ja"), // full- and half-width forms
        _ => None,
    }
}

/// Add the fonts kept from earlier visits when `text`, about to be drawn,
/// lacks a glyph; then tell the page when it needs a Chinese, Japanese
/// or Korean font. Ideographs that the Japanese font, once it has arrived,
/// still lacks ask for the Chinese one.
pub(crate) fn text_drawn(text: &str) {
    // Kept fonts are added between draws, not while text is measured, and
    // the page is asked only once they are in.
    if storage::adding_kept() {
        return;
    }
    if storage::has_kept() {
        let missing =
            |c: char| !c.is_whitespace() && !c.is_control() && !reshiki::style::has_glyph(c);
        if text.chars().any(missing) {
            storage::read_kept();
        }
        return;
    }
    if HOST.with_borrow(|host| host.on_missing_font.is_none()) {
        return;
    }
    // Lookups hold until a font is added or a request settles.
    let state = HOST.with_borrow(|host| {
        let settled = host.fonts_asked.iter().filter(|(_, done)| *done).count();
        (reshiki::style::font_generation(), settled)
    });
    for c in text.chars() {
        let Some(mut lang) = cjk_language(c) else {
            continue;
        };
        let known = HOST.with_borrow_mut(|host| {
            if host.checked.0 != state {
                host.checked = (state, Default::default());
            }
            !host.checked.1.insert(c)
        });
        if known {
            continue;
        }
        let asked = |lang: &str| {
            HOST.with_borrow(|host| {
                host.fonts_asked
                    .iter()
                    .find(|(asked, _)| *asked == lang)
                    .map(|(_, done)| *done)
            })
        };
        if lang == "ja"
            && let Some(done) = asked("ja")
        {
            let ideograph = matches!(c, '\u{3400}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}');
            // Until the Japanese request settles, its font may still draw
            // them.
            if !ideograph || !done {
                continue;
            }
            lang = "zh";
        }
        if asked(lang).is_some() || reshiki::style::has_glyph(c) {
            continue;
        }
        let callback = HOST.with_borrow_mut(|host| {
            host.fonts_asked.push((lang, false));
            host.on_missing_font.clone()
        });
        let Some(callback) = callback else {
            continue;
        };
        match callback.call1(&JsValue::NULL, &lang.into()) {
            // A promise settles once the font is added, or failed to load.
            Ok(result) => match result.dyn_into::<js_sys::Promise>() {
                Ok(loading) => wasm_bindgen_futures::spawn_local(async move {
                    let _ = wasm_bindgen_futures::JsFuture::from(loading).await;
                    font_request_settled(lang);
                }),
                Err(_) => font_request_settled(lang),
            },
            Err(thrown) => {
                error(&format!("The missing font callback failed: {thrown:?}"));
                font_request_settled(lang);
            }
        }
    }
}

/// Add fonts kept from earlier visits; see [`Request::AddFonts`].
pub(crate) fn add_kept_fonts(fonts: Vec<(String, Vec<u8>)>) {
    storage::add_kept(fonts);
}

/// The page finished answering the request for `lang`'s font.
fn font_request_settled(lang: &str) {
    HOST.with_borrow_mut(|host| {
        for (asked, done) in &mut host.fonts_asked {
            if *asked == lang {
                *done = true;
            }
        }
    });
    // Text drawn meanwhile is looked up again.
    let _ = send(Request::Redraw);
}

/// Stop the editor. Its canvas stays until the page removes it.
#[wasm_bindgen]
pub fn destroy() -> Result<(), JsError> {
    send(Request::Close)?;
    // The page may reuse the element; its events keep their own keys.
    let swap = HOST.with_borrow_mut(|host| host.command_key.take());
    if let (Some(swap), Some(window)) = (swap, web_sys::window()) {
        for name in SWAPPED_EVENTS {
            let _ = window.remove_event_listener_with_callback_and_bool(
                name,
                swap.as_ref().unchecked_ref(),
                true,
            );
        }
    }
    if let Some(input_method) = HOST.with_borrow_mut(|host| host.input_method.take()) {
        ime::stop(input_method);
    }
    let paste = HOST.with_borrow_mut(|host| host.canvas_paste.take());
    if let (Some(paste), Some(window)) = (paste, web_sys::window()) {
        for (name, listener) in &paste {
            let _ = window.remove_event_listener_with_callback_and_bool(
                name,
                listener.as_ref().unchecked_ref(),
                true,
            );
        }
    }
    let drops = HOST.with_borrow_mut(|host| host.drops.take());
    if let Some((element, drops)) = drops {
        for name in DRAG_EVENTS {
            let _ =
                element.remove_event_listener_with_callback(name, drops.as_ref().unchecked_ref());
        }
    }
    HOST.with_borrow_mut(|host| {
        host.requests = None;
        host.on_change = None;
        host.on_missing_font = None;
        host.on_dirty_change = None;
        host.on_title_change = None;
        // The drawings are gone, so leaving the page loses nothing more.
        host.dirty = false;
    });
    Ok(())
}

/// The app's subscriptions and the page's requests.
pub(crate) fn subscription(app: &App) -> Subscription<Message> {
    Subscription::batch([app.subscription(), Subscription::run(requests)])
}

fn requests() -> impl iced::futures::Stream<Item = Message> {
    let receiver = HOST.with_borrow_mut(|host| host.receiver.take());
    stream::iter(receiver).flatten().map(Message::Host)
}

/// The app's update, then the page's callbacks for what changed: the drawing
/// in front, whether any tab has unsaved changes, and the title.
pub(crate) fn update(app: &mut App, message: Message) -> Task<Message> {
    let task = app.update(message);
    // Captions ask for fonts as they are drawn; the interface's names do here.
    for text in app.interface_text() {
        text_drawn(&text);
    }
    let revision = app.revision();
    let (dirty, title) = (app.unsaved(), app.title());
    let (changed, dirty_changed, title_changed, sync_title) = HOST.with_borrow_mut(|host| {
        let previous = host.revision.replace(revision);
        let changed = previous
            .is_some_and(|previous| previous != revision)
            .then(|| host.on_change.clone())
            .flatten();
        // After `destroy` the drawings cannot be saved, so leaving loses
        // nothing; its Close request still passes through here.
        let dirty = dirty && host.requests.is_some();
        let dirty_changed = (host.dirty != dirty)
            .then(|| host.on_dirty_change.clone())
            .flatten();
        host.dirty = dirty;
        let title_changed = host.title.as_ref() != Some(&title);
        if title_changed {
            host.title = Some(title.clone());
        }
        (
            changed,
            dirty_changed,
            title_changed.then(|| host.on_title_change.clone()),
            title_changed && host.sync_title,
        )
    });
    if let Some(callback) = changed
        && let Err(thrown) = callback.call0(&JsValue::NULL)
    {
        error(&format!("The change callback failed: {thrown:?}"));
    }
    if let Some(callback) = dirty_changed {
        call(&callback, &dirty.into(), "dirty change");
    }
    // winit's set_title only labels the canvas, so the page's title is set here.
    if sync_title && let Some(document) = web_sys::window().and_then(|window| window.document()) {
        document.set_title(&title);
    }
    if let Some(Some(callback)) = title_changed {
        call(&callback, &title.into(), "title change");
    }
    task
}

/// Answer the page's waiting `save` with the drawing in front.
pub(crate) fn reply(id: u64, doc: Result<Document, String>) {
    if let Some(reply) = HOST.with_borrow_mut(|host| host.replies.remove(&id)) {
        let _ = reply.send(doc);
    }
}

/// Save a file as a download named after `path`.
pub(crate) fn download(path: &Path, bytes: &[u8]) -> Result<(), String> {
    const FAILED: &str = "Could not download the file";
    let window = web_sys::window().ok_or(FAILED)?;
    let document = window.document().ok_or(FAILED)?;
    let mime = match path.extension().and_then(|extension| extension.to_str()) {
        Some("png") => "image/png",
        Some("svg") => "image/svg+xml",
        Some("pdf") => "application/pdf",
        Some("mol") => "chemical/x-mdl-molfile",
        Some("rxn") => "chemical/x-mdl-rxnfile",
        Some("cdxml") => "chemical/x-cdxml",
        Some("smiles" | "rsmi" | "inchi") => "text/plain",
        _ => "application/octet-stream",
    };
    let options = web_sys::BlobPropertyBag::new();
    options.set_type(mime);
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options)
        .map_err(|_| FAILED)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|_| FAILED)?;
    let link: web_sys::HtmlAnchorElement = document
        .create_element("a")
        .map_err(|_| FAILED)?
        .dyn_into()
        .map_err(|_| FAILED)?;
    link.set_href(&url);
    link.set_download(
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Drawing"),
    );
    link.click();
    // Revoke once the click has started the download.
    let revoke = Closure::once_into_js(move || {
        let _ = web_sys::Url::revoke_object_url(&url);
    });
    window
        .set_timeout_with_callback(revoke.unchecked_ref())
        .map_err(|_| FAILED)?;
    Ok(())
}

/// Open `url` in a new tab.
pub(crate) fn open_url(url: &str) -> Result<(), String> {
    web_sys::window()
        .ok_or("The page has no window")?
        .open_with_url_and_target(url, "_blank")
        .map(|_| ())
        .map_err(|_| "Could not open the link".into())
}

fn clipboard() -> Option<web_sys::Clipboard> {
    Some(web_sys::window()?.navigator().clipboard())
}

/// Put `text` on the system clipboard, where the page may write it. A refusal
/// is logged and returned, so Cut can keep what was not copied.
pub(crate) async fn write_text(text: String) -> Result<(), String> {
    const REFUSED: &str = "The browser did not allow clipboard access";
    let clipboard = clipboard().ok_or(REFUSED)?;
    if let Err(refused) = wasm_bindgen_futures::JsFuture::from(clipboard.write_text(&text)).await {
        error(&format!("Could not copy to the clipboard: {refused:?}"));
        return Err(REFUSED.into());
    }
    Ok(())
}
