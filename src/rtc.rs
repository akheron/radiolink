use defmt::info;
use nrf51_hal::pac::{CLOCK, RTC0};

pub struct Rtc {
    rtc0: RTC0,
    now: u32,
}

impl Rtc {
    pub fn new(rtc0: RTC0) -> Self {
        Self { rtc0, now: 0 }
    }

    pub fn init(&self, clock: &CLOCK) {
        clock.tasks_lfclkstart.write(|w| unsafe { w.bits(1) });

        // ~1 ms per tick
        self.rtc0.prescaler.write(|w| unsafe { w.bits(32) });
        self.rtc0.evtenset.write(|w| w.tick().set());
        self.rtc0.intenset.write(|w| w.tick().set());
        self.rtc0.tasks_start.write(|w| unsafe { w.bits(1) });

        info!("RTC initialized");
    }

    pub fn run(&mut self) -> u32 {
        if self.rtc0.events_tick.read().bits() != 0 {
            self.rtc0.events_tick.write(|w| unsafe { w.bits(0) });
            self.now = self.rtc0.counter.read().bits();
        }
        self.now
    }
}
