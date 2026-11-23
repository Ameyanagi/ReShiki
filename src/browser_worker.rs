//! One bounded module worker per job kind. Requests are posted only after the
//! ready handshake, so an unavailable worker can fall back before work starts.
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, VecDeque},
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
use wasm_bindgen::{JsCast, JsValue, closure::Closure};
use web_time::{Duration, Instant};

const LOAD_DEADLINE: Duration = Duration::from_secs(60);
const QUEUE_DEADLINE: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(25);
const MAX_PENDING: usize = 8;

#[derive(Debug, Clone, Default)]
pub struct Cancel(Arc<AtomicBool>);
impl Cancel {
    pub fn stop(&self) {
        self.0.store(true, Ordering::Release);
    }
    pub(crate) fn stopped(&self) -> bool {
        self.0.load(Ordering::Acquire)
    }
}

#[derive(Debug)]
pub enum Failure {
    /// No operation was posted. A caller may use its local implementation.
    Unavailable(String),
    /// Work may have started; retrying on the caller could duplicate it.
    Failed(String),
    Cancelled,
}

#[derive(Default)]
struct Pool {
    idle: RefCell<Option<web_sys::Worker>>,
    active: RefCell<Option<web_sys::Worker>>,
    busy: Cell<bool>,
    loading: Cell<bool>,
    waiters: RefCell<VecDeque<u64>>,
    next: Cell<u64>,
}
struct Turn {
    pool: Rc<Pool>,
    ticket: u64,
}
impl Drop for Turn {
    fn drop(&mut self) {
        self.pool
            .waiters
            .borrow_mut()
            .retain(|ticket| *ticket != self.ticket);
    }
}
thread_local! {
    static FACTORY: RefCell<Option<js_sys::Function>> = const { RefCell::new(None) };
    static POOLS: RefCell<BTreeMap<&'static str, Rc<Pool>>> = const { RefCell::new(BTreeMap::new()) };
    static GENERATION: Cell<u64> = const { Cell::new(0) };
}

pub fn set_worker(factory: js_sys::Function) {
    shutdown();
    FACTORY.set(Some(factory));
}

/// Free retained worker memories and invalidate active and queued requests.
pub fn shutdown() {
    FACTORY.set(None);
    GENERATION.set(GENERATION.get().wrapping_add(1));
    POOLS.with_borrow(|pools| {
        for pool in pools.values() {
            for worker in [pool.idle.take(), pool.active.take()].into_iter().flatten() {
                worker.terminate();
            }
        }
    });
}

pub fn loading(job: &'static str) -> bool {
    POOLS.with_borrow(|pools| pools.get(job).is_some_and(|pool| pool.loading.get()))
}

pub fn available() -> bool {
    FACTORY.with_borrow(|factory| factory.is_some())
}

enum Event {
    Ready,
    Reply(Vec<u8>),
    Failed(String),
}
struct Request {
    worker: Option<web_sys::Worker>,
    pool: Rc<Pool>,
    events: Rc<RefCell<Vec<Event>>>,
    _message: Closure<dyn FnMut(web_sys::MessageEvent)>,
    _error: Closure<dyn FnMut(web_sys::Event)>,
}
impl Drop for Request {
    fn drop(&mut self) {
        if let Some(worker) = self.worker.take() {
            worker.set_onmessage(None);
            worker.set_onerror(None);
            worker.set_onmessageerror(None);
            worker.terminate();
        }
        self.pool.active.take();
        self.pool.loading.set(false);
        self.pool.busy.set(false);
    }
}

/// A successful worker is reused. Dropping, cancelling or failing a request
/// terminates it, so late replies cannot reach another request.
pub async fn exchange(
    job: &'static str,
    bytes: &[u8],
    cancel: &Cancel,
    deadline: Duration,
) -> Result<Vec<u8>, Failure> {
    let pool = POOLS.with_borrow_mut(|pools| pools.entry(job).or_default().clone());
    if pool.waiters.borrow().len() + usize::from(pool.busy.get()) >= MAX_PENDING {
        return Err(Failure::Failed(format!("The {job} worker queue is full")));
    }
    let generation = GENERATION.get();
    let ticket = pool.next.get();
    pool.next.set(ticket.wrapping_add(1));
    pool.waiters.borrow_mut().push_back(ticket);
    let turn = Turn {
        pool: pool.clone(),
        ticket,
    };
    let queued = Instant::now();
    while pool.busy.get() || pool.waiters.borrow().front().copied() != Some(ticket) {
        if cancel.stopped() || generation != GENERATION.get() {
            return Err(Failure::Cancelled);
        }
        if queued.elapsed() > QUEUE_DEADLINE {
            return Err(Failure::Failed(format!(
                "The {job} worker queue reached its 60-second time limit"
            )));
        }
        sleep().await;
    }
    if cancel.stopped() || generation != GENERATION.get() {
        return Err(Failure::Cancelled);
    }
    pool.busy.set(true);
    drop(turn);
    let (mut request, mut ready) = match start(pool.clone()) {
        Ok(request) => request,
        Err(error) => {
            pool.busy.set(false);
            return Err(Failure::Unavailable(error));
        }
    };
    pool.active.set(request.worker.clone());
    pool.loading.set(!ready);
    let mut sent = false;
    let mut started = Instant::now();
    loop {
        if cancel.stopped() || generation != GENERATION.get() {
            return Err(Failure::Cancelled);
        }
        for event in request.events.take() {
            match event {
                Event::Ready => {
                    ready = true;
                    pool.loading.set(false);
                }
                Event::Reply(bytes) if sent => {
                    if let Some(worker) = request.worker.take() {
                        worker.set_onmessage(None);
                        worker.set_onerror(None);
                        worker.set_onmessageerror(None);
                        pool.idle.set(Some(worker));
                    }
                    return Ok(bytes);
                }
                Event::Reply(_) => {
                    return Err(Failure::Unavailable(format!(
                        "The {job} worker sent an invalid ready handshake"
                    )));
                }
                Event::Failed(message) => {
                    let message = format!("The {job} worker stopped: {message}");
                    return Err(if sent {
                        Failure::Failed(message)
                    } else {
                        Failure::Unavailable(message)
                    });
                }
            }
        }
        if ready && !sent {
            let message = js_sys::Array::of2(&job.into(), &js_sys::Uint8Array::from(bytes));
            request
                .worker
                .as_ref()
                .ok_or_else(|| Failure::Failed("The worker stopped".into()))?
                .post_message(&message)
                .map_err(|error| {
                    Failure::Failed(format!(
                        "Could not reach the {job} worker: {}",
                        js_message(&error)
                    ))
                })?;
            sent = true;
            started = Instant::now();
        }
        if !ready && started.elapsed() > LOAD_DEADLINE {
            return Err(Failure::Unavailable(format!(
                "The {job} worker did not load within 60 seconds"
            )));
        }
        if sent && started.elapsed() > deadline {
            return Err(Failure::Failed(format!(
                "The {job} worker reached its {}-second time limit",
                deadline.as_secs()
            )));
        }
        sleep().await;
    }
}

fn start(pool: Rc<Pool>) -> Result<(Request, bool), String> {
    let (worker, ready) = match pool.idle.take() {
        Some(worker) => (worker, true),
        None => {
            let factory = FACTORY
                .with_borrow(Clone::clone)
                .ok_or("This page did not provide a local worker")?;
            let worker = factory
                .call0(&JsValue::NULL)
                .and_then(JsCast::dyn_into::<web_sys::Worker>)
                .map_err(|error| {
                    format!("Could not start the local worker: {}", js_message(&error))
                })?;
            (worker, false)
        }
    };
    let events = Rc::new(RefCell::new(Vec::new()));
    let replies = events.clone();
    let message = Closure::<dyn FnMut(_)>::new(move |event: web_sys::MessageEvent| {
        let data = event.data();
        replies.borrow_mut().push(if data.is_null() {
            Event::Ready
        } else if let Some(bytes) = data.dyn_ref::<js_sys::Uint8Array>() {
            if bytes.length() as usize > 513 * 1024 * 1024 {
                Event::Failed("worker response exceeds its size limit".into())
            } else {
                Event::Reply(bytes.to_vec())
            }
        } else {
            Event::Failed(data.as_string().unwrap_or_else(|| format!("{data:?}")))
        });
    });
    let failures = events.clone();
    let error = Closure::<dyn FnMut(_)>::new(move |event: web_sys::Event| {
        failures.borrow_mut().push(Event::Failed(
            event
                .dyn_ref::<web_sys::ErrorEvent>()
                .map(web_sys::ErrorEvent::message)
                .filter(|message| !message.is_empty())
                .unwrap_or_else(|| "its script could not be loaded".into()),
        ));
    });
    worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
    worker.set_onerror(Some(error.as_ref().unchecked_ref()));
    worker.set_onmessageerror(Some(error.as_ref().unchecked_ref()));
    Ok((
        Request {
            worker: Some(worker),
            pool,
            events,
            _message: message,
            _error: error,
        },
        ready,
    ))
}

fn js_message(value: &JsValue) -> String {
    value
        .dyn_ref::<js_sys::Error>()
        .map(|error| String::from(error.message()))
        .or_else(|| value.as_string())
        .unwrap_or_else(|| format!("{value:?}"))
}

async fn sleep() {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(
                &resolve,
                POLL.as_millis() as i32,
            );
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}
