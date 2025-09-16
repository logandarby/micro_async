use core::cell::Cell;

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
}

impl<'a, T> Receiver<'a, T> {
    fn new(channel: &'a Channel<T>) -> Self {
        Self { channel }
    }

    pub fn recv(&self) -> Option<T> {
        self.channel.recv()
    }
}

pub struct Channel<T> {
    item: Cell<Option<T>>,
}

impl<T> Channel<T> {
    pub fn new() -> Self {
        Self {
            item: Cell::new(Option::None),
        }
    }

    pub fn send(&self, item: T) {
        self.item.replace(Option::Some(item));
    }

    pub fn recv(&self) -> Option<T> {
        self.item.take()
    }

    pub fn get_sender<'a>(&'a self) -> Sender<'a, T> {
        Sender::new(self)
    }

    pub fn get_recv<'a>(&'a self) -> Receiver<'a, T> {
        Receiver::new(self)
    }
}
