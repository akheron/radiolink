#![no_std]
#![no_main]

use cortex_m_rt::entry;
use defmt_rtt as _;
use microbit::pac::UART0;
use panic_halt as _;

const TX_PIN: u32 = 2;
const RX_PIN: u32 = 3;

#[entry]
fn main() -> ! {
    let p = microbit::Peripherals::take().unwrap();
    p.GPIO.pin_cnf[TX_PIN as usize].write(|w| w.pull().pullup().dir().output());
    p.GPIO.pin_cnf[RX_PIN as usize].write(|w| w.pull().disabled().dir().input());

    let uart0 = p.UART0;
    uart0.pseltxd.write(|w| unsafe { w.bits(TX_PIN) });
    uart0.pselrxd.write(|w| unsafe { w.bits(RX_PIN) });
    uart0.baudrate.write(|w| w.baudrate().baud115200());
    uart0.enable.write(|w| w.enable().enabled());

    let _ = write_uart0(&uart0, "Type something:\r\n");

    uart0.tasks_startrx.write(|w| unsafe { w.bits(1) });
    loop {
        while uart0.events_rxdrdy.read().bits() == 0 {}
        uart0.events_rxdrdy.write(|w| unsafe { w.bits(0) });
        let c = uart0.rxd.read().bits() as u8;
        let _ = write_uart0(&uart0, &[c; 1]);
    }
}

fn write_uart0<B: AsRef<[u8]>>(uart0: &UART0, s: B) {
    write_uart0_data(uart0, s.as_ref());
}

fn write_uart0_data(uart0: &UART0, data: &[u8]) {
    uart0.tasks_starttx.write(|w| unsafe { w.bits(1) });

    for c in data.as_ref() {
        uart0.txd.write(|w| unsafe { w.bits(u32::from(*c)) });
        while uart0.events_txdrdy.read().bits() == 0 {}
        uart0.events_txdrdy.write(|w| unsafe { w.bits(0) });
    }

    uart0.tasks_stoptx.write(|w| unsafe { w.bits(1) });
}
