use embedded_hal::digital::{OutputPin, PinState, StatefulOutputPin};
use nrf52833_hal::{self as hal, gpio::*};

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

pub enum Direction {
    Left,
    Right,
}

pub struct LedBlinker<'a> {
    leds: &'a mut LedMatrix,
    row: usize,
    col: usize,
}

impl<'a> LedBlinker<'a> {
    pub fn new(leds: &'a mut LedMatrix, row: usize) -> Option<Self> {
        if row >= LedMatrix::ROWS {
            return None;
        }
        let _ = leds.pin_cols[0].set_low().unwrap();
        Some(Self { row, leds, col: 0 })
    }

    pub fn toggle(&mut self) {
        let _ = self.leds.pin_rows[self.row].toggle().unwrap();
    }

    pub fn shift(&mut self, direction: Direction) {
        let new_col = match direction {
            Direction::Left => (self.col + LedMatrix::COLS - 1) % LedMatrix::COLS,
            Direction::Right => (self.col + 1) % LedMatrix::COLS,
        };
        let _ = self.leds.pin_cols[self.col].set_high().unwrap();
        self.col = new_col;
        let _ = self.leds.pin_cols[self.col].set_low().unwrap();
        let _ = self.leds.pin_rows[self.row].set_low().unwrap();
    }
}
