use core::{
    cell::{Cell, RefCell},
    future::poll_fn,
    task::{Poll, Waker},
};

pub struct Sender<'a, T> {
    channel: &'a Channel<T>,
}

impl<'a, T> Sender<'a, T> {
    const fn new(channel: &'a Channel<T>) -> Self {
        Self { channel }
    }

    pub fn send(&self, item: T) {
        self.channel.send(item);
    }
}

pub struct Receiver<'a, T> {
    channel: &'a Channel<T>,
    state: RecvState,
}

enum RecvState {
    Init,
    Wait,
}

impl<'a, T> Receiver<'a, T> {
    const fn new(channel: &'a Channel<T>) -> Self {
        Self {
            channel,
            state: RecvState::Init,
        }
    }

    pub async fn recv(&mut self) -> T {
        poll_fn(move |cx| match self.state {
            RecvState::Init => {
                self.channel.register(cx.waker().clone());
                self.state = RecvState::Wait;
                Poll::Pending
            }
            RecvState::Wait => self
                .channel
                .recv()
                .map_or_else(|| Poll::Pending, |val| Poll::Ready(val)),
        })
        .await
    }
}

pub struct Channel<T> {
    item: Cell<Option<T>>,
    waker: RefCell<Option<Waker>>,
}

impl<T> Channel<T> {
    pub const fn new() -> Self {
        Self {
            item: Cell::new(Option::None),
            waker: RefCell::new(None),
        }
    }

    pub fn send(&self, item: T) {
        self.item.replace(Option::Some(item));
        if let Some(waker) = self.waker.borrow().as_ref() {
            waker.wake_by_ref();
        }
    }

    pub fn recv(&self) -> Option<T> {
        self.item.take()
    }

    pub fn register(&self, waker: Waker) {
        self.waker.replace(Some(waker));
    }

    pub const fn get_sender(&self) -> Sender<'_, T> {
        Sender::new(self)
    }

    pub const fn get_recv(&self) -> Receiver<'_, T> {
        Receiver::new(self)
    }
}
