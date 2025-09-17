use core::{
    cell::{RefCell, RefMut},
    pin::Pin,
    sync::atomic::{AtomicBool, AtomicU32, Ordering},
    task::{Context, Poll},
};

use critical_section::{CriticalSection, Mutex};
use defmt::info;
use fugit::{Duration, Instant};
pub type TickInstant = Instant<u64, 1, 32768>;
pub type TickDuration = Duration<u64, 1, 32768>;
use heapless::{BinaryHeap, binary_heap::Min};
use nrf52833_hal::{
    Rtc,
    pac::{NVIC, RTC0, interrupt},
    rtc::{RtcCompareReg, RtcInterrupt},
};
use snafu::prelude::*;

use crate::executor::{Executor, ExtWaker};

pub struct Timer {
    end_time: TickInstant,
    state: TimerState,
}

impl Timer {
    pub fn new(duration: TickDuration) -> Self {
        let end_time = Ticker::now() + duration;
        Self {
            end_time,
            state: TimerState::Init,
        }
    }

    fn is_ready(&self) -> bool {
        Ticker::now() >= self.end_time
    }

    pub async fn delay(duration: TickDuration) {
        Self::new(duration).await;
    }
}

enum TimerState {
    Wait,
    Init,
}

impl Future for Timer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        match self.state {
            TimerState::Init => {
                Ticker::add_deadline(self.end_time, cx.waker().task_id()).unwrap();
                self.state = TimerState::Wait;
                Poll::Pending
            }
            TimerState::Wait => {
                if self.is_ready() {
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

static DEADLINES: DeadlinePQ = Mutex::new(RefCell::new(BinaryHeap::new()));

static TICKER: Ticker = Ticker {
    rtc0: Mutex::new(RefCell::new(Option::None)),
    overflow_count: AtomicU32::new(0),
    initialized: AtomicBool::new(false),
};

#[derive(Eq, PartialEq, PartialOrd, Ord, Debug)]
struct Deadline {
    value: u32,
    task_id: usize,
}

const DEADLINE_MAX_ITEMS: usize = 10;

type DeadlinePQ = Mutex<RefCell<BinaryHeap<Deadline, Min, DEADLINE_MAX_ITEMS>>>;

pub struct Ticker {
    rtc0: Mutex<RefCell<Option<Rtc<RTC0>>>>,
    overflow_count: AtomicU32,
    initialized: AtomicBool,
}

#[derive(Debug, Snafu)]
pub enum TimerError {
    #[snafu(display("Ticker class has not been initialized"))]
    TickerUninitialized,
    #[snafu(display("The deadline {ticks} is too big for the internal counter"))]
    DeadlineTooLarge { ticks: u64 },
    #[snafu(display("The deadline queue is full. It has a max of {DEADLINE_MAX_ITEMS} items."))]
    TimerQueueFull,
}

impl Ticker {
    pub fn init(rtc0: RTC0, nvic: &mut NVIC) {
        // Prevent init if already initialized
        if TICKER.initialized.load(Ordering::Relaxed) {
            return;
        }
        // SAFETY: Can never return an error since prescalar 0 never returns an error
        #[allow(clippy::unwrap_used)]
        let mut rtc0 = Rtc::new(rtc0, 0).unwrap();
        rtc0.enable_counter();

        // Enable overflow interrupt
        rtc0.enable_event(RtcInterrupt::Overflow);
        rtc0.enable_interrupt(RtcInterrupt::Overflow, Some(nvic));

        // Enable compare interrupt
        rtc0.enable_event(RtcInterrupt::Compare0);
        rtc0.enable_interrupt(RtcInterrupt::Compare0, Some(nvic));

        // Init
        critical_section::with(|cs| {
            TICKER.rtc0.replace(cs, Some(rtc0));
        });
        TICKER.initialized.swap(true, Ordering::Relaxed);
    }

    pub fn now() -> TickInstant {
        let ticks = critical_section::with(|cs| {
            let counter = Self::acquire(cs)
                .expect("Ticker uninitialized")
                .get_counter();
            let overflow = TICKER.overflow_count.load(Ordering::Relaxed);
            (u64::from(overflow) << 24) | u64::from(counter)
        });
        TickInstant::from_ticks(ticks)
    }

    fn acquire(cs: CriticalSection<'_>) -> Result<RefMut<'_, Rtc<RTC0>>, TimerError> {
        let guard = TICKER.rtc0.borrow_ref_mut(cs);
        if guard.is_some() {
            // Safety: Garunteed to be some
            #[allow(clippy::unwrap_used)]
            Ok(RefMut::map(guard, |opt| opt.as_mut().unwrap()))
        } else {
            Err(TimerError::TickerUninitialized)
        }
    }

    // TODO: Implement overflow counter
    fn add_deadline(deadline: TickInstant, task_id: usize) -> Result<(), TimerError> {
        let deadline_ticks = deadline.ticks();
        let deadline_ticks =
            deadline_ticks
                .try_into()
                .map_err(|_| TimerError::DeadlineTooLarge {
                    ticks: deadline_ticks,
                })?;
        critical_section::with(|cs| {
            let mut deadlines = DEADLINES.borrow_ref_mut(cs);
            deadlines
                .push(Deadline {
                    task_id,
                    value: deadline_ticks,
                })
                .map_err(|_| TimerError::TimerQueueFull)?;
            // Always set compare register to earliest deadline
            // SAFETY: Deadline always has a value in it, so this can't panic
            #[allow(clippy::unwrap_used)]
            Self::acquire(cs)?
                .set_compare(RtcCompareReg::Compare0, deadlines.peek().unwrap().value)
                .map_err(|_| TimerError::DeadlineTooLarge {
                    ticks: deadline_ticks.into(),
                })
        })
    }
}

#[interrupt]
fn RTC0() {
    let result = critical_section::with(|cs| -> Result<(), TimerError> {
        let mut rtc0 = Ticker::acquire(cs)?;
        if rtc0.is_event_triggered(RtcInterrupt::Overflow) {
            rtc0.reset_event(RtcInterrupt::Overflow);
            TICKER
                .overflow_count
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        if rtc0.is_event_triggered(RtcInterrupt::Compare0) {
            // Handle the event, and if there are more deadlines, set the compare register
            info!("COMPARE TRIGGERED");
            let mut deadlines = DEADLINES.borrow_ref_mut(cs);
            // Safety: Deadline always added before interrupt
            #[allow(clippy::expect_used)]
            let latest = deadlines.pop().expect("No deadline available on interrupt");
            if let Some(pending_deadline) = deadlines.peek() {
                rtc0.set_compare(RtcCompareReg::Compare0, pending_deadline.value)
                    .map_err(|_| TimerError::DeadlineTooLarge {
                        ticks: pending_deadline.value.into(),
                    })?;
            }
            Executor::wake_task(latest.task_id);
            rtc0.reset_event(RtcInterrupt::Compare0);
        }
        // Read needed for clock cycles
        let _ = rtc0.is_event_triggered(RtcInterrupt::Overflow);
        Ok(())
    });
    // Since it occurs in interrupt, it is unrecoverable
    #[allow(clippy::expect_used)]
    result.unwrap();
}
