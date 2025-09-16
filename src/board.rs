use embedded_hal::digital::{OutputPin, PinState};
use nrf52833_hal::{self as hal, gpio::*};

use crate::{gpiote::GpioteManager, time::Ticker};

pub struct LedMatrix {
    pub pin_rows: [Pin<Output<PushPull>>; LedMatrix::ROWS],
    pub pin_cols: [Pin<Output<PushPull>>; LedMatrix::COLS],
}

impl LedMatrix {
    pub const ROWS: usize = 5;
    pub const COLS: usize = 5;

    pub fn clear(&mut self) {
        for irow in 0..Self::ROWS {
            let _ = self.pin_rows[irow].set_state(PinState::Low);
        }
        for icol in 0..Self::COLS {
            let _ = self.pin_cols[icol].set_state(PinState::High);
        }
    }
}

pub type Button = Pin<Input<Floating>>;

pub struct Board {
    pub leds: LedMatrix,
    pub btn_l: Button,
    pub btn_r: Button,
}

impl Board {
    pub fn new() -> Self {
        let p = hal::pac::Peripherals::take().unwrap();
        let mut core_p = hal::pac::CorePeripherals::take().unwrap();
        Ticker::init(p.RTC0, &mut core_p.NVIC);
        GpioteManager::init(p.GPIOTE);
        let p0parts = p0::Parts::new(p.P0);
        let p1parts = p1::Parts::new(p.P1);

        let rows = [
            p0parts.p0_21.into_push_pull_output(Level::Low).degrade(),
            p0parts.p0_22.into_push_pull_output(Level::Low).degrade(),
            p0parts.p0_15.into_push_pull_output(Level::Low).degrade(),
            p0parts.p0_24.into_push_pull_output(Level::Low).degrade(),
            p0parts.p0_19.into_push_pull_output(Level::Low).degrade(),
        ];
        let cols = [
            p0parts.p0_28.into_push_pull_output(Level::High).degrade(),
            p0parts.p0_11.into_push_pull_output(Level::High).degrade(),
            p0parts.p0_31.into_push_pull_output(Level::High).degrade(),
            p1parts.p1_05.into_push_pull_output(Level::High).degrade(),
            p0parts.p0_30.into_push_pull_output(Level::High).degrade(),
        ];
        Self {
            leds: LedMatrix {
                pin_rows: rows,
                pin_cols: cols,
            },
            btn_l: p0parts.p0_14.into_floating_input().degrade(),
            btn_r: p0parts.p0_23.into_floating_input().degrade(),
        }
    }
}
