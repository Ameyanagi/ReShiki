//! A sign-in that lasts until the app quits. Each test thread has its own.
use super::FAILED;

#[derive(Default)]
struct Slot {
    bytes: Option<Vec<u8>>,
    failing: bool,
}

#[cfg(not(test))]
fn with<T>(work: impl FnOnce(&mut Slot) -> T) -> T {
    static SLOT: std::sync::Mutex<Slot> = std::sync::Mutex::new(Slot {
        bytes: None,
        failing: false,
    });
    work(&mut SLOT.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
}

#[cfg(test)]
fn with<T>(work: impl FnOnce(&mut Slot) -> T) -> T {
    thread_local!(static SLOT: std::cell::RefCell<Slot> = std::cell::RefCell::default());
    SLOT.with_borrow_mut(work)
}

fn checked<T>(work: impl FnOnce(&mut Slot) -> T) -> Result<T, String> {
    with(|slot| {
        if slot.failing {
            Err(FAILED.into())
        } else {
            Ok(work(slot))
        }
    })
}

pub async fn read() -> Result<Option<Vec<u8>>, String> {
    checked(|slot| slot.bytes.clone())
}

pub async fn write(bytes: Vec<u8>) -> Result<(), String> {
    checked(|slot| slot.bytes = Some(bytes))
}

pub async fn delete() -> Result<(), String> {
    checked(|slot| slot.bytes = None)
}

/// Make this thread's store fail like a locked or missing keyring.
#[cfg(test)]
pub fn fail(failing: bool) {
    with(|slot| slot.failing = failing);
}
