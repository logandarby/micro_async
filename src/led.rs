use embedded_hal::digital::{OutputPin, StatefulOutputPin};
use nrf52833_hal::gpio::*;

pub struct LedMatrix {
    pub pin_rows: [Pin<Output<PushPull>>; LedMatrix::ROWS],
    pub pin_cols: [Pin<Output<PushPull>>; LedMatrix::COLS],
}

impl LedMatrix {
    pub const ROWS: usize = 5;
    pub const COLS: usize = 5;
}

pub enum Direction {
    Left,
    Right,
    Down,
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
        leds.pin_cols[0].set_low().unwrap();
        Some(Self { row, leds, col: 0 })
    }

    pub fn toggle(&mut self) {
        self.leds.pin_rows[self.row].toggle().unwrap();
    }

    pub fn shift(&mut self, direction: Direction) {
        match direction {
            Direction::Left | Direction::Right => self.shift_horizontal(direction),
            Direction::Down => self.shift_vertical(),
        }
    }

    fn shift_vertical(&mut self) {
        let new_row = (self.row + 1) % LedMatrix::ROWS;
        self.leds.pin_rows[self.row].set_low().unwrap();
        self.row = new_row;
        self.leds.pin_rows[self.row].set_high().unwrap();
        self.leds.pin_cols[self.col].set_high().unwrap();
    }

    fn shift_horizontal(&mut self, direction: Direction) {
        let new_col = match direction {
            Direction::Left => (self.col + LedMatrix::COLS - 1) % LedMatrix::COLS,
            Direction::Right => (self.col + 1) % LedMatrix::COLS,
            _ => return,
        };
        self.leds.pin_cols[self.col].set_high().unwrap();
        self.col = new_col;
        self.leds.pin_cols[self.col].set_low().unwrap();
        self.leds.pin_rows[self.row].set_low().unwrap();
    }
}
