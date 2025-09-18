use core::cell::{RefCell, RefMut};

use critical_section::{CriticalSection, Mutex};

pub struct LockMut<T> {
    inner: Mutex<RefCell<Option<T>>>,
}

impl<T> LockMut<T> {
    pub const fn new_with_value(t: T) -> Self {
        Self {
            inner: Mutex::new(RefCell::new(Some(t))),
        }
    }

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

    pub fn acquire<'a>(&'a self, cs: CriticalSection<'a>) -> RefMut<'a, T> {
        RefMut::map(self.inner.borrow_ref_mut(cs), |opt| {
            opt.as_mut().expect("Please initialize the LockMut first")
        })
    }
}
