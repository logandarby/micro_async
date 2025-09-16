#![no_std]
#![no_main]

use core::{
    panic::PanicInfo,
    pin::pin,
    sync::atomic::{AtomicBool, Ordering},
};

use cortex_m::{self as _, asm, interrupt};
use cortex_m_rt::entry;
use defmt::{self as _, info};
use defmt_rtt as _;
use embedded_hal::digital::{PinState, StatefulOutputPin};

use crate::{
    board::{Board, Button},
    channel::*,
    executor::Executor,
    gpiote::*,
    led::{LedBlinker, LedMatrix},
    time::{TickDuration, Timer},
};

mod board;
mod channel;
mod executor;
mod gpiote;
mod led;
mod time;

async fn led_task(
    leds: &mut LedMatrix,
    blink_duration: TickDuration,
    _btn_recv: Receiver<'_, ButtonDirection>,
) {
    let mut blinky = LedBlinker::new(leds, 0).unwrap();
    loop {
        blinky.toggle();
        Timer::delay(blink_duration).await;
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ButtonDirection {
    Left,
    Right,
}

pub async fn button_task(
    button: Button,
    direction: ButtonDirection,
    sender: Sender<'_, ButtonDirection>,
) {
    let mut input = InputChannel::new(button);
    loop {
        input.wait_for(PinState::Low).await;
        info!(
            "{} Button Pressed",
            match direction {
                ButtonDirection::Left => "Left",
                ButtonDirection::Right => "Right",
            }
        );
        sender.send(direction);
        input.wait_for(PinState::High).await;
        info!(
            "{} Button Released",
            match direction {
                ButtonDirection::Left => "Left",
                ButtonDirection::Right => "Right",
            }
        );
    }
}

#[entry]
fn main() -> ! {
    info!("Starting");
    let mut b = Board::new();
    let btn_channel = Channel::<ButtonDirection>::new();
    let led_task = pin!(led_task(
        &mut b.leds,
        TickDuration::millis(500),
        btn_channel.get_recv()
    ));
    let button_task_r = pin!(button_task(
        b.btn_r,
        ButtonDirection::Right,
        btn_channel.get_sender()
    ));
    let button_task_l = pin!(button_task(
        b.btn_l,
        ButtonDirection::Left,
        btn_channel.get_sender()
    ));
    Executor::run_tasks(&mut [button_task_l, button_task_r, led_task]);
}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    static PANICKED: AtomicBool = AtomicBool::new(false);
    interrupt::disable();
    if !PANICKED.load(Ordering::Relaxed) {
        PANICKED.store(true, Ordering::Relaxed);
        defmt::error!("{}", defmt::Display2Format(info));
    }
    asm::bkpt();
    asm::udf();
}
