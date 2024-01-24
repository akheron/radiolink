use defmt::debug;
use microbit::pac::RNG;

pub struct Rng {
    rng: RNG,
}

impl Rng {
    pub fn new(rng: RNG) -> Self {
        Self { rng }
    }

    pub fn init(&self) {
        // Enable digital corrector
        self.rng.config.write(|w| unsafe { w.bits(1) });
        debug!("RNG initialized");
    }

    pub fn random(&self) -> u32 {
        let mut result: u32 = 0;

        self.rng.tasks_start.write(|w| unsafe { w.bits(1) });
        let mut i = 0;
        while i < 4 {
            while self.rng.events_valrdy.read().bits() == 0 {}
            self.rng.events_valrdy.write(|w| unsafe { w.bits(0) });

            let next = self.rng.value.read().bits();
            result = (result << 8) | (next & 0xff);

            i += 1;
        }
        self.rng.tasks_stop.write(|w| unsafe { w.bits(1) });

        result
    }
}
