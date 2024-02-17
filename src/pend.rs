use core::ops::{Add, AddAssign};

#[derive(Clone, Copy)]
pub enum Pend {
    Nothing,
    Radio,
    Uart,
    Both,
}

impl Pend {
    pub fn plus(self, other: Self) -> Self {
        match (self, other) {
            (Pend::Nothing, Pend::Nothing) => Pend::Nothing,
            (Pend::Nothing, Pend::Radio) => Pend::Radio,
            (Pend::Nothing, Pend::Uart) => Pend::Uart,
            (Pend::Nothing, Pend::Both) => Pend::Both,
            (Pend::Radio, Pend::Nothing) => Pend::Radio,
            (Pend::Radio, Pend::Radio) => Pend::Radio,
            (Pend::Radio, Pend::Uart) => Pend::Both,
            (Pend::Radio, Pend::Both) => Pend::Both,
            (Pend::Uart, Pend::Nothing) => Pend::Uart,
            (Pend::Uart, Pend::Radio) => Pend::Both,
            (Pend::Uart, Pend::Uart) => Pend::Uart,
            (Pend::Uart, Pend::Both) => Pend::Both,
            (Pend::Both, Pend::Nothing) => Pend::Both,
            (Pend::Both, Pend::Radio) => Pend::Both,
            (Pend::Both, Pend::Uart) => Pend::Both,
            (Pend::Both, Pend::Both) => Pend::Both,
        }
    }
}

impl Add for Pend {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self.plus(other)
    }
}

impl AddAssign for Pend {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}
