use core::cell::{Cell, RefCell};

use critical_section::Mutex;

pub struct LockCell<T> {
    inner: Mutex<Cell<T>>,
}

impl<T> LockCell<T> {
    pub const fn new(t: T) -> Self {
        Self {
            inner: Mutex::new(Cell::new(t)),
        }
    }

    pub fn with_lock<R>(&self, f: impl FnOnce(&Cell<T>) -> R) -> R {
        critical_section::with(|cs| f(self.inner.borrow(cs)))
    }
}

pub struct LockMut<T> {
    inner: Mutex<RefCell<Option<T>>>,
}

impl<T> LockMut<T> {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(RefCell::new(None)),
        }
    }

    pub fn init(&self, val: T) {
        critical_section::with(|cs| self.inner.replace(cs, Some(val)));
    }

    pub fn with_lock<R>(&self, f: impl FnOnce(&mut T) -> R) -> R {
        critical_section::with(|cs| {
            f(self
                .inner
                .borrow_ref_mut(cs)
                .as_mut()
                .expect("Please initialize the LockMut first"))
        })
    }
}
