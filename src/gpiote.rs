use core::{
    cell::RefCell,
    future::poll_fn,
    sync::atomic::{AtomicUsize, Ordering},
    task::Poll,
};

use critical_section::Mutex;
use embedded_hal::digital::{InputPin, PinState};
use nrf52833_hal::{
    gpio::*,
    gpiote::{Gpiote, GpioteInputPin},
    pac::{GPIOTE, Interrupt, NVIC, interrupt},
};

use crate::executor::{Executor, ExtWaker};

pub struct GpioteManager {
    gpiote: Mutex<RefCell<Option<Gpiote>>>,
}

impl GpioteManager {
    pub fn init(gpiote: GPIOTE) {
        critical_section::with(|cs| GPIOTE_MANAGER.gpiote.replace(cs, Some(Gpiote::new(gpiote))));
    }
}

static GPIOTE_MANAGER: GpioteManager = GpioteManager {
    gpiote: Mutex::new(RefCell::new(Option::None)),
};

const INVALID_TASK_ID: usize = usize::MAX;
const MAX_CHANNELS: usize = 3;
static WAKE_TASKS: [AtomicUsize; MAX_CHANNELS] =
    [const { AtomicUsize::new(INVALID_TASK_ID) }; MAX_CHANNELS];
static NEXT_CHANNEL: AtomicUsize = AtomicUsize::new(0);

type InputChannelPin<MODE> = Pin<Input<MODE>>;

// Essentially registers an interrupt with a GPIO pin and a channel
// When the pin transitions to the ready state, then an interrupt is fired
pub struct InputChannel<MODE> {
    pin: InputChannelPin<MODE>,
    channel_id: usize,
}

impl<MODE> InputChannel<MODE>
where
    InputChannelPin<MODE>: GpioteInputPin,
{
    pub fn new(pin: InputChannelPin<MODE>) -> Self {
        let channel_id = NEXT_CHANNEL.fetch_add(1, Ordering::Relaxed);
        critical_section::with(|cs| {
            let mut rm = GPIOTE_MANAGER.gpiote.borrow_ref_mut(cs);
            let gpiote = rm.as_mut().unwrap();
            let channel = match channel_id {
                0 => gpiote.channel0(),
                1 => gpiote.channel1(),
                2 => gpiote.channel2(),
                _ => panic!("Too many channels"),
            };
            channel.input_pin(&pin).toggle().enable_interrupt();
            unsafe { NVIC::unmask(Interrupt::GPIOTE) }
        });
        Self { pin, channel_id }
    }

    pub async fn wait_for(&mut self, ready_state: PinState) -> () {
        poll_fn(move |cx| {
            if ready_state == self.pin.is_high().unwrap().into() {
                Poll::Ready(())
            } else {
                WAKE_TASKS[self.channel_id].store(cx.waker().task_id(), Ordering::Relaxed);
                Poll::Pending
            }
        })
        .await
    }
}

#[interrupt]
fn GPIOTE() {
    critical_section::with(|cs| {
        let mut rm = GPIOTE_MANAGER.gpiote.borrow_ref_mut(cs);
        let Some(gpiote) = rm.as_mut() else {
            return;
        };
        for (channel, task) in WAKE_TASKS.iter().enumerate() {
            let channel = match channel {
                0 => gpiote.channel0(),
                1 => gpiote.channel1(),
                2 => gpiote.channel2(),
                _ => panic!("Too many channels"),
            };
            if !channel.is_event_triggered() {
                continue;
            }
            let task_id = task.swap(INVALID_TASK_ID, Ordering::Relaxed);
            if task_id != INVALID_TASK_ID {
                Executor::wake_task(task_id);
            }
        }
        gpiote.reset_events();
        // Dummy read for clock cycles
        let _ = gpiote.channel0().is_event_triggered();
    });
}
