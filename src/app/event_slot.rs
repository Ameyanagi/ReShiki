//! Native event receivers are installed before their subscription takes ownership.
use super::Message;
use iced::{Subscription, futures::SinkExt};
use std::{
    future::Future,
    hash::{Hash, Hasher},
    sync::Mutex,
};
use tokio::sync::mpsc;

pub(super) struct Slot<R>(Mutex<Option<R>>);

impl<R> Slot<R> {
    pub(super) const fn new() -> Self {
        Self(Mutex::new(None))
    }

    pub(super) fn install(&self, receiver: R) {
        if let Ok(mut slot) = self.0.lock() {
            *slot = Some(receiver);
        }
    }

    pub(super) fn take(&self) -> Option<R> {
        self.0.lock().ok().and_then(|mut slot| slot.take())
    }
}

pub(super) trait Receiver: Send + 'static {
    type Item: Send + 'static;

    fn recv(&mut self) -> impl Future<Output = Option<Self::Item>> + Send;
}

impl<T: Send + 'static> Receiver for mpsc::Receiver<T> {
    type Item = T;

    fn recv(&mut self) -> impl Future<Output = Option<T>> + Send {
        mpsc::Receiver::recv(self)
    }
}

impl<T: Send + 'static> Receiver for mpsc::UnboundedReceiver<T> {
    type Item = T;

    fn recv(&mut self) -> impl Future<Output = Option<T>> + Send {
        mpsc::UnboundedReceiver::recv(self)
    }
}

struct Source<R: Receiver> {
    slot: &'static Slot<R>,
    capacity: usize,
    forward: fn(R::Item) -> Message,
}

impl<R: Receiver> Hash for Source<R> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::ptr::from_ref(self.slot).hash(state);
        self.capacity.hash(state);
        self.forward.hash(state);
    }
}

pub(super) fn subscription<R: Receiver>(
    slot: &'static Slot<R>,
    capacity: usize,
    forward: fn(R::Item) -> Message,
) -> Subscription<Message> {
    Subscription::run_with(
        Source {
            slot,
            capacity,
            forward,
        },
        stream,
    )
}

fn stream<R: Receiver>(source: &Source<R>) -> impl iced::futures::Stream<Item = Message> + use<R> {
    let slot = source.slot;
    let forward = source.forward;
    iced::stream::channel(source.capacity, async move |mut output| {
        if let Some(mut receiver) = slot.take() {
            while let Some(item) = receiver.recv().await {
                if output.send(forward(item)).await.is_err() {
                    break;
                }
            }
        }
    })
}

#[cfg(test)]
mod tests;
