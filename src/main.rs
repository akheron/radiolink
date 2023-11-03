#![no_std]
#![no_main]

mod radio;
mod uart;

use crate::radio::Radio;
use cortex_m_rt::entry;
use defmt_rtt as _;
use microbit::Peripherals;
use panic_halt as _;

const TX_PIN: u32 = 2;
const RX_PIN: u32 = 3;

#[entry]
fn main() -> ! {
    let p = Peripherals::take().unwrap();

    let mut uart = uart::Uart::new(p.UART0);
    uart.init(&p.GPIO, TX_PIN, RX_PIN);

    let mut radio = Radio::new(p.RADIO);
    radio.init(&p.CLOCK);

    loop {
        uart.tick();
        radio.tick();

        if let Some(c) = uart.read() {
            radio.write(c);
        }
        if let Some(msg) = radio.read() {
            uart.write(msg);
        }
    }
}
