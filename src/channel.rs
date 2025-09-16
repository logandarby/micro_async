use core::{
    cell::{Cell, RefCell},
    future::poll_fn,
    task::{Poll, Waker},
};

pub struct Sender<'a, T> {
    channel: &'a Channel<T>,
}

impl<'a, T> Sender<'a, T> {
    fn new(channel: &'a Channel<T>) -> Self {
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
    fn new(channel: &'a Channel<T>) -> Self {
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
            RecvState::Wait => {
                if let Some(val) = self.channel.recv() {
                    Poll::Ready(val)
                } else {
                    Poll::Pending
                }
            }
        })
        .await
    }
}

pub struct Channel<T> {
    item: Cell<Option<T>>,
    waker: RefCell<Option<Waker>>,
}

impl<T> Channel<T> {
    pub fn new() -> Self {
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

    pub fn get_sender<'a>(&'a self) -> Sender<'a, T> {
        Sender::new(self)
    }

    pub fn get_recv<'a>(&'a self) -> Receiver<'a, T> {
        Receiver::new(self)
    }
}
