use core::{
    cell::{RefCell, RefMut},
    future::poll_fn,
    sync::atomic::{AtomicUsize, Ordering},
    task::Poll,
};

use critical_section::{CriticalSection, Mutex};
use defmt::error;
use embedded_hal::digital::{InputPin, PinState};
use nrf52833_hal::{
    gpio::{Floating, Input, Pin},
    gpiote::{Gpiote, GpioteChannel},
    pac::{GPIOTE, Interrupt, NVIC, interrupt},
};

use crate::{atomic_waker::AtomicWaker, infalliable::InfallibleExt};

pub struct GpioteManager {
    gpiote: Mutex<RefCell<Option<Gpiote>>>,
}

use snafu::prelude::*;

type ChannelId = usize;

#[derive(Snafu, Debug)]
pub enum GpioteError {
    #[snafu(display(
        "Too many InputChannels have been initialized, only {MAX_CHANNELS} are permitted."
    ))]
    OutOfChannels,
    #[snafu(display("Please initialize GpioteManager first"))]
    GpioteManagerUninitialized,
}

impl GpioteManager {
    pub fn init(gpiote: GPIOTE) {
        critical_section::with(|cs| GPIOTE_MANAGER.gpiote.replace(cs, Some(Gpiote::new(gpiote))));
    }

    pub fn acquire(cs: CriticalSection<'_>) -> Option<RefMut<'_, Gpiote>> {
        let rm = GPIOTE_MANAGER.gpiote.borrow_ref_mut(cs);
        if rm.is_some() {
            Some(RefMut::map(rm, |option| option.as_mut().unwrap()))
        } else {
            None
        }
    }

    pub fn get_channel(
        gpiote: &Gpiote,
        channel: ChannelId,
    ) -> Result<GpioteChannel<'_>, GpioteError> {
        Ok(match channel {
            0 => gpiote.channel0(),
            1 => gpiote.channel1(),
            2 => gpiote.channel2(),
            3 => gpiote.channel3(),
            4 => gpiote.channel4(),
            5 => gpiote.channel5(),
            6 => gpiote.channel6(),
            7 => gpiote.channel7(),
            _ => return Err(GpioteError::OutOfChannels),
        })
    }
}

static GPIOTE_MANAGER: GpioteManager = GpioteManager {
    gpiote: Mutex::new(RefCell::new(Option::None)),
};

const MAX_CHANNELS: usize = 8;
static WAKE_TASKS: [AtomicWaker; MAX_CHANNELS] = [const { AtomicWaker::new() }; MAX_CHANNELS];
static NEXT_CHANNEL: AtomicUsize = AtomicUsize::new(0);

type InputChannelPin = Pin<Input<Floating>>;

// Essentially registers an interrupt with a GPIO pin and a channel
// When the pin transitions to the ready state, then an interrupt is fired
pub struct InputChannel {
    pin: InputChannelPin,
    channel_id: ChannelId,
}

impl InputChannel {
    pub fn new(pin: InputChannelPin) -> Result<Self, GpioteError> {
        let channel_id = NEXT_CHANNEL.fetch_add(1, Ordering::Relaxed);
        critical_section::with(|cs| {
            let gpiote =
                GpioteManager::acquire(cs).ok_or(GpioteError::GpioteManagerUninitialized)?;
            let channel = GpioteManager::get_channel(&gpiote, channel_id)?;
            channel.input_pin(&pin).toggle().enable_interrupt();
            unsafe { NVIC::unmask(Interrupt::GPIOTE) }
            Ok(())
        })?;
        Ok(Self { pin, channel_id })
    }

    pub async fn wait_for(&mut self, ready_state: PinState) -> () {
        poll_fn(move |cx| {
            if ready_state == PinState::from(self.pin.is_high().unwrap_infallible()) {
                Poll::Ready(())
            } else {
                critical_section::with(|cs| WAKE_TASKS[self.channel_id].register(cs, cx.waker()));
                Poll::Pending
            }
        })
        .await;
    }
}

#[interrupt]
fn GPIOTE() {
    critical_section::with(|cs| {
        let Some(gpiote) = GpioteManager::acquire(cs) else {
            error!("GpioteManager is uninitialized");
            return;
        };
        WAKE_TASKS
            .iter()
            .enumerate()
            .filter(|(channel, _)| {
                GpioteManager::get_channel(&gpiote, *channel)
                    .is_ok_and(|channel| channel.is_event_triggered())
            })
            .for_each(|(_, task)| task.wake(cs));
        gpiote.reset_events();
        // Dummy read for clock cycles
        let _ = gpiote.channel0().is_event_triggered();
    });
}
