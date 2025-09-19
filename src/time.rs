use core::{
    cell::Cell,
    marker::PhantomPinned,
    pin::Pin,
    task::{Context, Poll, Waker},
};

use critical_section::Mutex;
use fugit::{Duration, Instant};
pub type TickInstant = Instant<u64, 1, 32768>;
pub type TickDuration = Duration<u64, 1, 32768>;
use nrf52833_hal::{
    Rtc,
    pac::{NVIC, RTC0, interrupt},
    rtc::{RtcCompareReg, RtcInterrupt},
};
use snafu::prelude::*;

use intrusive_collections::{LinkedList, LinkedListAtomicLink, UnsafeRef, intrusive_adapter};

use crate::utils::{LockCell, LockMut};

intrusive_adapter!(TimerAdapter = UnsafeRef<TimerInner>: TimerInner { link: LinkedListAtomicLink } );

pub struct Timer {
    // SAFETY: Never access this through a mutable reference
    inner: TimerInner,
}

struct TimerQueue {
    timers: LinkedList<TimerAdapter>,
}

impl TimerQueue {
    fn new() -> Self {
        Self {
            timers: LinkedList::new(TimerAdapter::new()),
        }
    }

    fn insert_timer(&mut self, timer: &Timer) {
        /*
           SAFETY:
           UnsafeRef is safe if the object it is pointing to is not moved, dropped, or accessed through a mutable reference during the UnsafeRef's lifetime.
           - The inner value is pinned to avoid moving
           - The timer never exposes any functions to mutably alter the TimerInner, and the TimerInner is itself never accessed mutably
           - When the timer is dropped, it is first removed from the linked list
        */
        let timer_ref = unsafe { UnsafeRef::from_raw(&timer.inner) };
        let mut cursor = self.timers.front_mut();
        while let Some(current) = cursor.get() {
            if current.end_time > timer.inner.end_time {
                break;
            }
            cursor.move_next();
        }
        cursor.insert_before(timer_ref);
    }

    fn remove_timer(&mut self, timer: &Timer) {
        if timer.inner.link.is_linked() {
            // SAFETY
            // Since there is only one static timer queue in this module, then we know the timer must be a part of it
            let mut cursor = unsafe { self.timers.cursor_mut_from_ptr(&timer.inner) };
            cursor.remove();
        }
    }

    fn peek_earliest(&self) -> Option<&TimerInner> {
        self.timers.front().get()
    }

    fn pop_earliest(&mut self) -> Option<UnsafeRef<TimerInner>> {
        self.timers.pop_front()
    }
}

// SAFETY
// Must not be moved, dropped, or accessed through a mutable reference as long as at least one UnsafeRef is pointing to it
struct TimerInner {
    end_time: TickInstant,
    state: LockCell<TimerState>,
    waker: LockCell<Option<Waker>>,
    link: LinkedListAtomicLink,
    _pin: PhantomPinned,
}

impl Timer {
    pub fn new(duration: TickDuration) -> Self {
        let end_time = Ticker::now() + duration;
        Self {
            inner: TimerInner {
                end_time,
                state: LockCell::new(TimerState::Init),
                waker: LockCell::new(None),
                link: LinkedListAtomicLink::new(),
                _pin: PhantomPinned,
            },
        }
    }

    pub async fn delay(duration: TickDuration) {
        Self::new(duration).await;
    }

    fn is_ready(&self) -> bool {
        Ticker::now() >= self.inner.end_time
    }

    fn add_to_queue(&self, waker: &Waker) {
        TICKER.with_lock(|ticker| {
            // Only add if not already in the queue
            if !self.inner.link.is_linked() {
                ticker.deadlines.insert_timer(self);
                self.inner
                    .waker
                    .with_lock(|waker_cell| waker_cell.replace(Some(waker.clone())));
                // Update if this is now the earliest
                if let Some(latest) = ticker.deadlines.peek_earliest() {
                    set_deadline(&latest.end_time, &mut ticker.rtc0);
                }
            }
        });
    }

    fn remove_from_queue(&self) {
        TICKER.with_lock(|ticker| {
            if self.inner.link.is_linked() {
                ticker.deadlines.remove_timer(self);
                // Update in case we removed the first timer
                if let Some(earliest) = ticker.deadlines.peek_earliest() {
                    set_deadline(&earliest.end_time, &mut ticker.rtc0);
                }
            }
        })
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        // Remove from queue when dropped to adhere to UnsafeCell's safety requirements
        self.remove_from_queue();
    }
}

enum TimerState {
    Wait,
    Init,
}

impl Future for Timer {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let state = self
            .inner
            .state
            .with_lock(|cell| cell.replace(TimerState::Wait));
        match state {
            TimerState::Init => {
                self.add_to_queue(cx.waker());
                self.inner
                    .state
                    .with_lock(|cell| cell.set(TimerState::Wait));
                Poll::Pending
            }
            TimerState::Wait => {
                if self.is_ready() {
                    self.remove_from_queue();
                    Poll::Ready(())
                } else {
                    Poll::Pending
                }
            }
        }
    }
}

static TICKER: LockMut<Ticker> = LockMut::new();

pub struct Ticker {
    rtc0: Rtc<RTC0>,
    overflow_count: u32,
    deadlines: TimerQueue,
}

#[derive(Debug, Snafu)]
pub enum TimerError {
    #[snafu(display("The deadline {ticks} is too big for the internal counter"))]
    DeadlineTooLarge { ticks: u64 },
}

impl Ticker {
    pub fn init(rtc0: RTC0, nvic: &mut NVIC) {
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
        TICKER.init(Self {
            overflow_count: 0,
            rtc0,
            deadlines: TimerQueue::new(),
        });
    }

    pub fn now() -> TickInstant {
        let ticks = TICKER.with_lock(|ticker| {
            let counter = ticker.rtc0.get_counter();
            let overflow = ticker.overflow_count;
            (u64::from(overflow) << 24) | u64::from(counter)
        });
        TickInstant::from_ticks(ticks)
    }
}

fn set_deadline(deadline: &TickInstant, rtc0: &mut Rtc<RTC0>) {
    let deadline_low = (deadline.ticks() & 0x00FF_FFFF) as u32;
    rtc0.set_compare(RtcCompareReg::Compare0, deadline_low)
        .unwrap();
}

#[interrupt]
fn RTC0() {
    TICKER.with_lock(handle_rtc0_interrupt);
}

// TODO: I believe this is unsound, since it does not collect all the pending deadlines, only one.
fn handle_rtc0_interrupt(ticker: &mut Ticker) {
    let rtc0 = &mut ticker.rtc0;
    if rtc0.is_event_triggered(RtcInterrupt::Overflow) {
        rtc0.reset_event(RtcInterrupt::Overflow);
        ticker.overflow_count += 1;
    }
    if rtc0.is_event_triggered(RtcInterrupt::Compare0) {
        rtc0.reset_event(RtcInterrupt::Compare0);
        let latest = ticker
            .deadlines
            .pop_earliest()
            .expect("No deadline available on interrupt");
        if let Some(pending_deadline) = ticker.deadlines.peek_earliest() {
            set_deadline(&pending_deadline.end_time, rtc0);
        }
        latest
            .waker
            .with_lock(|cell| cell.replace(None))
            .expect("Timer does not have an associated waker")
            .wake();
    }
}
