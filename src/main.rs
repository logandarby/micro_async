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
use embedded_hal::digital::{InputPin, OutputPin, PinState};
use futures::{FutureExt, select_biased};
use nrf52833_hal::gpio::Level;

use crate::{
    board::{Board, Button, TouchSensor},
    channel::*,
    executor::Executor,
    gpiote::*,
    led::{Direction, LedBlinker, LedMatrix},
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
    mut btn_recv: Receiver<'_, ButtonDirection>,
) {
    let mut blinky = LedBlinker::new(leds, 0).unwrap();
    loop {
        select_biased! {
            direction = btn_recv.recv().fuse() => {
                blinky.shift(match direction {
                    ButtonDirection::Left => Direction::Left,
                    ButtonDirection::Right => Direction::Right,
                    ButtonDirection::Down => Direction::Down,
                });
                blinky.toggle();
            }
            _ = Timer::delay(blink_duration).fuse() => { blinky.toggle(); }
        }
    }
}

#[derive(Debug, Copy, Clone)]
pub enum ButtonDirection {
    Left,
    Right,
    Down,
}

pub async fn button_task(
    button: Button,
    direction: ButtonDirection,
    sender: Sender<'_, ButtonDirection>,
) {
    let mut input = InputChannel::new(button);
    loop {
        input.wait_for(PinState::Low).await;
        info!("Pressed");
        sender.send(direction);
        input.wait_for(PinState::High).await;
        info!("Released");
    }
}

async fn read_capacitance(pin: TouchSensor) -> (TouchSensor, u32) {
    let mut output_pin = pin.into_push_pull_output(Level::High);
    output_pin.set_high().ok();
    Timer::delay(TickDuration::micros(10)).await;
    let pin = output_pin.into_floating_input();
    let mut discharge_count = 0u32;
    const MAX_COUNT: u32 = 1000;
    while pin.is_high().unwrap() && discharge_count < MAX_COUNT {
        discharge_count += 1;
        Timer::delay(TickDuration::micros(1)).await;
    }
    (pin, discharge_count)
}

pub async fn touch_task(mut touch: TouchSensor) {
    let mut baseline = 0u32;
    for _ in 0..10 {
        let result = read_capacitance(touch).await;
    }
    loop {}
}

#[entry]
fn main() -> ! {
    info!("Starting");
    let mut b = Board::new();
    let touch_task = pin!(touch_task(b.touch_sensor));
    Executor::run_tasks(&mut [touch_task]);
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
