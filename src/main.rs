#![no_std]
#![no_main]

use core::panic::PanicInfo;
use core::sync::atomic::{self, Ordering};

use cortex_m_rt::entry;
use defmt::debug;
use defmt_rtt as _; // global logger
use microbit::pac::Peripherals;

use crate::queue::Queue;
use crate::radio::Radio;
use crate::rtc::Rtc;
use crate::uart::Uart;

mod queue;
mod radio;
mod rtc;
mod uart;

// USB UART pins
// const TX_PIN: u32 = 24;
// const RX_PIN: u32 = 25;

// Edge connector rings 0 and 1
const TX_PIN: u32 = 2;
const RX_PIN: u32 = 3;

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();

    let mut rtc = Rtc::new(p.RTC0);
    rtc.init(&p.CLOCK);

    let mut uart = Uart::new(p.UART0);
    uart.init(&p.GPIO, TX_PIN, RX_PIN);

    let mut radio = Radio::new(p.RADIO);
    radio.init(&p.CLOCK);

    let mut uart_to_radio = Queue::new();
    let mut radio_to_uart = Queue::new();
    loop {
        let now = rtc.tick();
        uart.tick(now, &mut radio_to_uart, &mut uart_to_radio);
        radio.tick(now, &mut uart_to_radio, &mut radio_to_uart);

        radio_to_uart.flow_control(&mut uart_to_radio);
        uart_to_radio.flow_control(&mut radio_to_uart);
    }
}

#[inline(never)]
#[panic_handler]
fn panic(panic_info: &PanicInfo) -> ! {
    if let Some(s) = panic_info.payload().downcast_ref::<&str>() {
        debug!("panic: {=str}", *s);
    } else {
        debug!("panic");
    }
    loop {
        atomic::compiler_fence(Ordering::SeqCst);
    }
}
