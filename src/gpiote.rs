use core::{
    future::poll_fn,
    sync::atomic::{AtomicUsize, Ordering},
    task::Poll,
};

use embedded_hal::digital::{InputPin, PinState};
use nrf52833_hal::{
    gpio::{Floating, Input, Pin},
    gpiote::{Gpiote, GpioteChannel},
    pac::{GPIOTE, Interrupt, NVIC, interrupt},
};

use crate::utils::{AtomicWaker, InfallibleExt, LockMut};

use snafu::prelude::*;

type ChannelId = usize;

#[derive(Snafu, Debug)]
pub enum GpioteError {
    #[snafu(display(
        "Too many InputChannels have been initialized, only {MAX_CHANNELS} are permitted."
    ))]
    OutOfChannels,
}

fn get_channel(gpiote: &Gpiote, channel: ChannelId) -> Result<GpioteChannel<'_>, GpioteError> {
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

pub struct GpioteManager {}

impl GpioteManager {
    pub fn init(gpiote: GPIOTE) {
        GPIOTE_MANAGER.init(Gpiote::new(gpiote));
    }
}

static GPIOTE_MANAGER: LockMut<Gpiote> = LockMut::new();

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
        GPIOTE_MANAGER.with_lock(|gpiote| {
            let channel = get_channel(gpiote, channel_id)?;
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
    GPIOTE_MANAGER.with_lock(handle_gpiote_interrupt);
}

fn handle_gpiote_interrupt(gpiote: &mut Gpiote) {
    WAKE_TASKS
        .iter()
        .enumerate()
        .filter(|(channel, _)| {
            get_channel(gpiote, *channel).is_ok_and(|channel| channel.is_event_triggered())
        })
        .for_each(|(_, task)| task.wake());
    gpiote.reset_events();
    // Dummy read for clock cycles
    let _ = gpiote.channel0().is_event_triggered();
}
