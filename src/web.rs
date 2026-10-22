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
}

#[derive(Default)]
struct Host {
    requests: Option<mpsc::UnboundedSender<Request>>,
    /// Until the app's subscription takes it.
    receiver: Option<mpsc::UnboundedReceiver<Request>>,
    replies: HashMap<u64, oneshot::Sender<Result<Document, String>>>,
    next_reply: u64,
    on_change: Option<js_sys::Function>,
    /// Called once, the first time a caption has Japanese (CJK) text that no
    /// loaded font draws.
    on_missing_font: Option<js_sys::Function>,
    revision: Option<(u64, u64)>,
    /// winit runs one event loop per page, even after it exits.
    mounted: bool,
    /// The listener that trades ⌘ and Control on a Mac, until `destroy`.
    command_key: Option<Closure<dyn FnMut(web_sys::Event)>>,
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
/// `format` when given. `on_missing_font` is called once, the first time a
/// caption has Japanese (CJK) text that no loaded font draws. A page can mount one editor; put further editors in
/// iframes.
#[wasm_bindgen]
pub async fn mount(
    element: web_sys::Element,
    data: Option<Vec<u8>>,
    format: Option<String>,
    on_missing_font: Option<js_sys::Function>,
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
            ..Host::default()
        }
    });
    if crate::platform::mac() {
        let swap = command_key(&element);
        HOST.with_borrow_mut(|host| host.command_key = swap);
    }
    // iced's multisampling leaves most canvases blank on WebGL2, the fallback
    // when the browser offers no WebGPU adapter.
    let antialiasing = webgpu().await;
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

/// Add a TrueType or OpenType font (or collection) for drawing, measuring and
/// export, before or after `mount`. Text whose own font lacks a glyph falls
/// back to it. Returns how many faces it holds.
#[wasm_bindgen(js_name = addFont)]
pub fn add_font(bytes: Vec<u8>) -> Result<usize, JsError> {
    let faces = reshiki::style::add_font(bytes).map_err(js)?;
    // Before mount the first frame has it anyway.
    let _ = send(Request::Redraw);
    Ok(faces)
}

/// Tell the page when `text`, about to be drawn, needs a Japanese font.
pub(crate) fn text_drawn(text: &str) {
    let cjk = |c: char| {
        matches!(c,
            '\u{3000}'..='\u{30FF}' // CJK punctuation, hiragana, katakana
            | '\u{3400}'..='\u{4DBF}' // CJK extension A
            | '\u{4E00}'..='\u{9FFF}' // CJK unified ideographs
            | '\u{F900}'..='\u{FAFF}' // CJK compatibility ideographs
            | '\u{FF00}'..='\u{FFEF}') // full- and half-width forms
    };
    if HOST.with_borrow(|host| host.on_missing_font.is_none())
        || !text
            .chars()
            .any(|c| cjk(c) && !reshiki::style::has_glyph(c))
    {
        return;
    }
    let callback = HOST.with_borrow_mut(|host| host.on_missing_font.take());
    if let Some(callback) = callback
        && let Err(thrown) = callback.call0(&JsValue::NULL)
    {
        error(&format!("The missing font callback failed: {thrown:?}"));
    }
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
    HOST.with_borrow_mut(|host| {
        host.requests = None;
        host.on_change = None;
        host.on_missing_font = None;
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

/// The app's update, then the page's change callback when the drawing in
/// front changed.
pub(crate) fn update(app: &mut App, message: Message) -> Task<Message> {
    let task = app.update(message);
    let revision = app.revision();
    let callback = HOST.with_borrow_mut(|host| {
        let previous = host.revision.replace(revision);
        previous
            .is_some_and(|previous| previous != revision)
            .then(|| host.on_change.clone())
            .flatten()
    });
    if let Some(callback) = callback
        && let Err(thrown) = callback.call0(&JsValue::NULL)
    {
        error(&format!("The change callback failed: {thrown:?}"));
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
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).map_err(|_| FAILED)?;
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

/// Ask with the browser's OK/Cancel prompt; true for OK.
pub(crate) fn confirm(message: &str) -> bool {
    web_sys::window().is_some_and(|window| window.confirm_with_message(message).unwrap_or(false))
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

/// The system clipboard's text, when the page may read it.
pub(crate) async fn read_text() -> Option<String> {
    wasm_bindgen_futures::JsFuture::from(clipboard()?.read_text())
        .await
        .ok()?
        .as_string()
}
