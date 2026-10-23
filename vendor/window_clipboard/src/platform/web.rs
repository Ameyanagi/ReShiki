// ReShiki patch: the browser's clipboard for wasm32, in place of the dummy
// provider whose read and write always fail.
//
// A page cannot read the clipboard while a key press is handled, so `read`
// answers with the text the page hands over from the paste event with
// `set_contents`, or that this clipboard last wrote. `write` starts
// `navigator.clipboard.writeText` inside the key press, where the browser
// allows it, and logs a refusal.
use crate::ClipboardProvider;

use raw_window_handle::HasDisplayHandle;
use std::cell::RefCell;
use std::error::Error;
use wasm_bindgen::JsCast;

thread_local! {
    static CONTENTS: RefCell<Option<String>> = const { RefCell::new(None) };
}

/// Hand `read` the clipboard's text, as a paste event gives it, or none.
pub fn set_contents(text: Option<String>) {
    CONTENTS.with_borrow_mut(|contents| *contents = text);
}

struct Browser;

pub fn connect<W: HasDisplayHandle>(
    _window: &W,
) -> Result<Box<dyn ClipboardProvider>, Box<dyn Error>> {
    Ok(Box::new(Browser))
}

impl ClipboardProvider for Browser {
    fn read(&self) -> Result<String, Box<dyn Error>> {
        CONTENTS
            .with_borrow(Clone::clone)
            .ok_or_else(|| "The clipboard has no text".into())
    }

    fn write(&mut self, contents: String) -> Result<(), Box<dyn Error>> {
        set_contents(Some(contents.clone()));
        // `navigator.clipboard` is missing outside secure pages.
        let clipboard = web_sys::window()
            .and_then(|window| {
                js_sys::Reflect::get(&window.navigator(), &"clipboard".into())
                    .ok()
            })
            .and_then(|clipboard| clipboard.dyn_into::<web_sys::Clipboard>().ok())
            .ok_or("This page has no clipboard access")?;
        let written = clipboard.write_text(&contents);
        wasm_bindgen_futures::spawn_local(async move {
            if let Err(refused) =
                wasm_bindgen_futures::JsFuture::from(written).await
            {
                web_sys::console::warn_2(
                    &"Could not copy to the clipboard:".into(),
                    &refused,
                );
            }
        });
        Ok(())
    }
}
