#![no_std]
#![no_main]

mod radio;
mod rtc;
mod uart;

use crate::radio::Radio;
use crate::rtc::Rtc;
use crate::uart::Uart;
use core::panic::PanicInfo;
use core::sync::atomic::{self, Ordering};
use cortex_m_rt::entry;
use defmt::debug;
use defmt_rtt as _;
use heapless::spsc::Queue;
use microbit::Peripherals;

//use panic_halt as _;

// USB UART pins
// const TX_PIN: u32 = 24;
// const RX_PIN: u32 = 25;

// Edge connector rings 0 and 1
const TX_PIN: u32 = 2;
const RX_PIN: u32 = 3;

const XON: u8 = 0x11;
const XOFF: u8 = 0x13;

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();

    let mut rtc = Rtc::new(p.RTC0);
    rtc.init(&p.CLOCK);

    let mut uart = Uart::new(p.UART0);
    uart.init(&p.GPIO, TX_PIN, RX_PIN);

    let mut radio = Radio::new(p.RADIO);
    radio.init(&p.CLOCK);

    let mut uart_to_radio: Queue<u8, 1024> = Queue::new();
    let mut uart_to_radio_control = Control::new();

    let mut radio_to_uart: Queue<u8, 1024> = Queue::new();
    let mut radio_to_uart_control = Control::new();

    loop {
        let now = rtc.tick();
        uart.tick(now, &mut radio_to_uart, &mut uart_to_radio);
        radio.tick(now, &mut uart_to_radio, &mut radio_to_uart);

        if let Some(c) = radio_to_uart_control.control(&radio_to_uart) {
            uart_to_radio.enqueue(c).unwrap();
        }
        if let Some(c) = uart_to_radio_control.control(&uart_to_radio) {
            radio_to_uart.enqueue(c).unwrap();
        }
    }
}

#[inline(never)]
#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    debug!("panic");
    loop {
        atomic::compiler_fence(Ordering::SeqCst);
    }
}

struct Control {
    xoff: bool,
}

impl Control {
    fn new() -> Self {
        Self { xoff: false }
    }

    fn control(&mut self, queue: &Queue<u8, 1024>) -> Option<u8> {
        if queue.len() > 512 && !self.xoff {
            self.xoff = true;
            Some(XOFF)
        } else if queue.len() < 256 && self.xoff {
            self.xoff = false;
            Some(XON)
        } else {
            None
        }
    }
}
