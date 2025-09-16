use core::{
    cell::RefCell,
    sync::atomic::{AtomicU32, Ordering},
};

use critical_section::Mutex;
use fugit::{Duration, Instant};
pub type TickInstant = Instant<u64, 1, 32768>;
pub type TickDuration = Duration<u64, 1, 32768>;
use nrf52833_hal::{
    Rtc,
    pac::{NVIC, RTC0, interrupt},
    rtc::RtcInterrupt,
};

pub struct Timer {
    end_time: TickInstant,
}

impl Timer {
    pub fn new(duration: TickDuration) -> Self {
        let end_time = Ticker::now() + duration;
        Self { end_time }
    }

    pub fn is_ready(&self) -> bool {
        Ticker::now() >= self.end_time
    }
}

static TICKER: Ticker = Ticker {
    rtc0: Mutex::new(RefCell::new(Option::None)),
    overflow_count: AtomicU32::new(0),
};

pub struct Ticker {
    rtc0: Mutex<RefCell<Option<Rtc<RTC0>>>>,
    overflow_count: AtomicU32,
}

impl Ticker {
    pub fn init(rtc0: RTC0, nvic: &mut NVIC) {
        let mut rtc0 = Rtc::new(rtc0, 0).unwrap();
        rtc0.enable_counter();
        rtc0.enable_event(RtcInterrupt::Overflow);
        rtc0.enable_interrupt(RtcInterrupt::Overflow, Option::Some(nvic));
        critical_section::with(|cs| TICKER.rtc0.replace(cs, Option::Some(rtc0)));
    }

    pub fn now() -> TickInstant {
        let ticks = {
            critical_section::with(|cs| {
                let counter = TICKER
                    .rtc0
                    .borrow_ref_mut(cs)
                    .as_mut()
                    .expect("Timer has not been initialized")
                    .get_counter();
                let overflow = TICKER.overflow_count.load(Ordering::Relaxed);
                ((overflow as u64) << 24) | counter as u64
            })
        };
        TickInstant::from_ticks(ticks.into())
    }
}

#[interrupt]
fn RTC0() {
    critical_section::with(|cs| {
        let mut rtc0 = TICKER.rtc0.borrow_ref_mut(cs);
        let Some(rtc0) = rtc0.as_mut() else {
            return;
        };
        if rtc0.is_event_triggered(RtcInterrupt::Overflow) {
            rtc0.reset_event(RtcInterrupt::Overflow);
            TICKER
                .overflow_count
                .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
        }
        // Read needed for clock cycles
        let _ = rtc0.is_event_triggered(RtcInterrupt::Overflow);
    });
}
