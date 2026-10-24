//! The browser sends the desktop worker's requests, as the `naming` job, to
//! the editor's Web Worker (`packages/editor/src/worker.js`). It has its own
//! WebAssembly memory, bounded like the desktop worker process, so a runaway
//! parse cannot take the editor down. A request past its time limit,
//! cancelled or dropped terminates the worker, and the next request starts
//! another. Other jobs can share this worker once they move off the page.
use super::{Error, Identity, Record, local, reverse};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_time::{Duration, Instant};

/// The worker job that answers the desktop naming worker's requests.
const JOB: &str = "naming";
const DEADLINE: Duration = Duration::from_secs(15);
/// Downloading and compiling the worker's WebAssembly, before the first request.
const LOAD_DEADLINE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(25);
const PARSE_CANCELLED: &str = "Local name parsing was cancelled";
const GENERATE_CANCELLED: &str = "Local name generation was cancelled";

#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }
    fn stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

thread_local! {
    /// Makes the worker; the page supplies it, so bundlers see the script.
    static FACTORY: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    /// The loaded worker between requests.
    static IDLE: RefCell<Option<web_sys::Worker>> = const { RefCell::new(None) };
    /// One request at a time, as the desktop runs one worker per kind.
    static BUSY: Cell<bool> = const { Cell::new(false) };
}

/// Start workers with `factory`, which returns a new module `Worker`
/// running `worker.js`.
pub fn set_worker(factory: js_sys::Function) {
    FACTORY.set(Some(factory));
    if let Some(worker) = IDLE.take() {
        worker.terminate();
    }
}

pub async fn resolve_name(name: &str, cancel: Cancel) -> Result<Record, Error> {
    let name = local::checked_name(name)?;
    let output = exchange(&local::parse_request(name)?, &cancel, PARSE_CANCELLED).await?;
    local::parse_result(name, &output)
}

/// As on the desktop, a name is returned only after the parser exactly
/// reconstructs the structure from it.
pub async fn generate_name(identity: &Identity, cancel: Cancel) -> Result<Record, Error> {
    let expected = super::canonical_smiles(&identity.smiles).map_err(Error::Rejected)?;
    let request = reverse::generate_request(&expected)?;
    let output = exchange(&request, &cancel, GENERATE_CANCELLED).await?;
    let candidate = reverse::parse_response(&output, &expected)?;
    let reconstructed = resolve_name(&candidate, cancel)
        .await
        .map_err(|error| match error {
            Error::Cancelled(_) => Error::Cancelled(GENERATE_CANCELLED.into()),
            error => error,
        })?;
    reverse::generated_record(identity, expected, candidate, &reconstructed)
}

/// What the worker posted: `null` once loaded, then the response bytes or
/// the failure's text.
enum Event {
    Loaded,
    Reply(Vec<u8>),
    Failed(String),
}

/// One request on the worker, which stays loaded only if it answers.
struct Request {
    worker: Option<web_sys::Worker>,
    events: Rc<RefCell<Vec<Event>>>,
    _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _error: Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for Request {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.set_onmessage(None);
            worker.set_onerror(None);
            worker.terminate();
        }
        BUSY.set(false);
    }
}

async fn exchange(request: &[u8], cancel: &Cancel, cancelled: &str) -> Result<Vec<u8>, Error> {
    while BUSY.get() {
        if cancel.stopped() {
            return Err(Error::Cancelled(cancelled.into()));
        }
        sleep(POLL).await;
    }
    BUSY.set(true);
    let (mut request, mut loaded) = start(request).inspect_err(|_| BUSY.set(false))?;
    let mut started = Instant::now();
    loop {
        if cancel.stopped() {
            return Err(Error::Cancelled(cancelled.into()));
        }
        for event in request.events.take() {
            match event {
                Event::Loaded => {
                    loaded = true;
                    started = Instant::now();
                }
                Event::Reply(bytes) => {
                    // The worker answered, so it can take the next request.
                    if let Some(worker) = request.worker.take() {
                        worker.set_onmessage(None);
                        worker.set_onerror(None);
                        IDLE.set(Some(worker));
                    }
                    return Ok(bytes);
                }
                Event::Failed(message) => {
                    return Err(Error::Worker(format!(
                        "The bounded local naming worker stopped: {message}"
                    )));
                }
            }
        }
        if !loaded && started.elapsed() > LOAD_DEADLINE {
            return Err(Error::Worker(
                "The local naming worker did not load within 60 seconds".into(),
            ));
        }
        if loaded && started.elapsed() > DEADLINE {
            return Err(Error::Worker(
                "Local name parsing reached its 15-second time limit".into(),
            ));
        }
        sleep(POLL).await;
    }
}

/// Posts the request, to the idle worker if there is one, which has loaded.
fn start(request: &[u8]) -> Result<(Request, bool), Error> {
    let (worker, loaded) = match IDLE.take() {
        Some(worker) => (worker, true),
        None => (spawn()?, false),
    };
    let events = Rc::new(RefCell::new(Vec::new()));
    let replies = events.clone();
    let message = Closure::<dyn FnMut(_)>::new(move |event: web_sys::MessageEvent| {
        let data = event.data();
        replies.borrow_mut().push(if data.is_null() {
            Event::Loaded
        } else if let Some(bytes) = data.dyn_ref::<js_sys::Uint8Array>() {
            Event::Reply(bytes.to_vec())
        } else {
            Event::Failed(data.as_string().unwrap_or_else(|| format!("{data:?}")))
        });
    });
    let failures = events.clone();
    let error = Closure::<dyn FnMut(_)>::new(move |event: web_sys::Event| {
        let message = event
            .dyn_ref::<web_sys::ErrorEvent>()
            .map(web_sys::ErrorEvent::message)
            .filter(|message| !message.is_empty())
            .unwrap_or_else(|| "its script could not be loaded".into());
        failures.borrow_mut().push(Event::Failed(message));
    });
    worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
    worker.set_onerror(Some(error.as_ref().unchecked_ref()));
    let state = Request {
        worker: Some(worker),
        events,
        _message: message,
        _error: error,
    };
    if let Some(worker) = &state.worker {
        worker
            .post_message(&js_sys::Array::of2(
                &JsValue::from_str(JOB),
                &js_sys::Uint8Array::from(request),
            ))
            .map_err(|error| {
                Error::Worker(format!(
                    "Could not reach the local naming worker: {error:?}"
                ))
            })?;
    }
    Ok((state, loaded))
}

fn spawn() -> Result<web_sys::Worker, Error> {
    let factory = FACTORY
        .with_borrow(Clone::clone)
        .ok_or_else(|| Error::Worker("This page did not provide a local naming worker".into()))?;
    factory
        .call0(&JsValue::NULL)
        .and_then(JsCast::dyn_into::<web_sys::Worker>)
        .map_err(|error| {
            Error::Worker(format!(
                "Could not start the local naming worker: {}",
                js_message(&error)
            ))
        })
}

fn js_message(value: &JsValue) -> String {
    value
        .dyn_ref::<js_sys::Error>()
        .map(|error| String::from(error.message()))
        .or_else(|| value.as_string())
        .unwrap_or_else(|| format!("{value:?}"))
}

async fn sleep(duration: Duration) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                duration.as_millis() as i32,
            );
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
