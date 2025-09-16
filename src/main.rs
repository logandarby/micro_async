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
use embedded_hal::digital::{OutputPin, PinState, StatefulOutputPin};

use crate::{
    board::{Board, Button, LedMatrix},
    channel::*,
    executor::Executor,
    gpiote::*,
    time::{TickDuration, Timer},
};

mod board;
mod channel;
mod executor;
mod gpiote;
mod time;

enum LedState {
    Toggle,
    Wait(Timer),
}

pub struct LedTask<'a> {
    leds: &'a mut LedMatrix,
    active_col: usize,
    state: LedState,
    btn_recv: Receiver<'a, ButtonDirection>,
}

impl<'a> LedTask<'a> {
    pub fn new(leds: &'a mut LedMatrix, btn_recv: Receiver<'a, ButtonDirection>) -> Self {
        let _ = leds.pin_cols[0].set_state(PinState::Low);
        Self {
            leds,
            active_col: 0,
            state: LedState::Toggle,
            btn_recv,
        }
    }

    pub fn poll(&mut self) {
        match self.state {
            LedState::Toggle => {
                let _ = self.leds.pin_rows[0].toggle().unwrap();
                let timer = Timer::new(TickDuration::millis(500));
                self.state = LedState::Wait(timer);
            }
            LedState::Wait(ref timer) => {
                if timer.is_ready() {
                    self.state = LedState::Toggle;
                }
                if let Some(direction) = self.btn_recv.recv() {
                    self.shift(direction);
                    self.state = LedState::Toggle;
                }
            }
        }
    }

    fn shift(&mut self, direction: ButtonDirection) {
        let new_col = match direction {
            ButtonDirection::Left => (self.active_col + LedMatrix::COLS - 1) % LedMatrix::COLS,
            ButtonDirection::Right => (self.active_col + 1) % LedMatrix::COLS,
        };
        let _ = self.leds.pin_cols[self.active_col].set_high().unwrap();
        self.active_col = new_col;
        let _ = self.leds.pin_cols[self.active_col].set_low().unwrap();
        let _ = self.leds.pin_rows[0].set_low().unwrap();
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
    let mut _led_task = LedTask::new(&mut b.leds, btn_channel.get_recv());
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
    Executor::run_tasks(&mut [button_task_l, button_task_r]);
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
