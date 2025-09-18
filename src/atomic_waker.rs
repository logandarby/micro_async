use core::{cell::Cell, task::Waker};

use critical_section::{CriticalSection, Mutex};

pub struct AtomicWaker {
    inner: Mutex<Cell<Option<Waker>>>,
}

impl AtomicWaker {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(Cell::new(None)),
        }
    }

    pub fn register(&self, cs: CriticalSection, waker: &Waker) {
        let cell = self.inner.borrow(cs);
        let prev_value = cell.replace(None);
        cell.set(match prev_value {
            Some(prev_value) if prev_value.will_wake(waker) => Some(prev_value),
            _ => Some(waker.clone()),
        });
    }

    pub fn wake(&self, cs: CriticalSection) {
        if let Some(waker) = self.inner.borrow(cs).replace(None) {
            waker.wake_by_ref();
        }
    }
}
