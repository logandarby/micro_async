use embedded_hal::digital::{OutputPin, PinState, StatefulOutputPin};
use nrf52833_hal::gpio::{Output, Pin, PushPull};

use crate::utils::InfallibleExt;

#[derive(Copy, Clone)]
pub enum LedState {
    On,
    Off,
}

#[derive(Copy, Clone)]
pub enum LedAxis {
    Col,
    Row,
}

pub type LedPin = Pin<Output<PushPull>>;

pub struct LedMatrix {
    pub pin_rows: [LedPin; LedMatrix::ROWS],
    pub pin_cols: [LedPin; LedMatrix::COLS],
}

impl LedMatrix {
    pub const ROWS: usize = 5;
    pub const COLS: usize = 5;

    pub fn get(&mut self, axis: LedAxis, col_or_row: usize) -> &mut LedPin {
        match axis {
            LedAxis::Col => {
                let col = col_or_row;
                assert!(col < Self::COLS, "Column index {col} out of bounds");
                &mut self.pin_cols[col]
            }
            LedAxis::Row => {
                let row = col_or_row;
                assert!(row < Self::ROWS, "Row index {row} out of bounds");
                &mut self.pin_rows[row]
            }
        }
    }

    pub fn set(&mut self, axis: LedAxis, col_or_row: usize, state: LedState) {
        self.get(axis, col_or_row)
            .set_state(match (axis, state) {
                (LedAxis::Row, LedState::On) | (LedAxis::Col, LedState::Off) => PinState::High,
                _ => PinState::Low,
            })
            .unwrap_infallible();
    }

    pub fn toggle(&mut self, axis: LedAxis, col_or_row: usize) {
        self.get(axis, col_or_row).toggle().unwrap_infallible();
    }
}

#[derive(Copy, Clone)]
pub enum Direction {
    Left,
    Right,
}

pub struct LedBlinker<'a> {
    leds: &'a mut LedMatrix,
    row: usize,
    col: usize,
}

const INITIAL_COL: usize = 0;

impl<'a> LedBlinker<'a> {
    pub fn new(leds: &'a mut LedMatrix, row: usize) -> Option<Self> {
        if row >= LedMatrix::ROWS {
            return None;
        }
        leds.set(LedAxis::Col, INITIAL_COL, LedState::On);
        Some(Self {
            row,
            leds,
            col: INITIAL_COL,
        })
    }

    pub fn toggle(&mut self) {
        self.leds.toggle(LedAxis::Row, self.row);
    }

    pub fn shift(&mut self, direction: Direction) {
        let new_col = match direction {
            Direction::Left => (self.col + LedMatrix::COLS - 1) % LedMatrix::COLS,
            Direction::Right => (self.col + 1) % LedMatrix::COLS,
        };
        self.leds.set(LedAxis::Col, self.col, LedState::Off);
        self.col = new_col;
        self.leds.set(LedAxis::Col, self.col, LedState::On);
        self.leds.set(LedAxis::Row, self.row, LedState::On);
    }
}
