#![feature(type_alias_impl_trait)]
#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

use core::sync::atomic::Ordering;
use defmt::trace;
use heapless::spsc::Queue;
use nrf51_hal::pac::Interrupt;
use rtic::export::atomic::AtomicU32;

use crate::pend::Pend;
use crate::radio::Radio;
use crate::radio_protocol::RadioProtocol;
use crate::rtc::Rtc;
use crate::uart::Uart;

mod pend;
mod radio;
mod radio_protocol;
mod rtc;
mod uart;

// USB UART pins
// const TX_PIN: u32 = 24;
// const RX_PIN: u32 = 25;

// Edge connector rings 0 and 1
const TX_PIN: u32 = 2;
const RX_PIN: u32 = 3;

#[rtic::app(device = nrf51_hal::pac, peripherals = true, dispatchers = [SWI0])]
mod app {
    use super::*;

    struct Queues {
        uart_tx: uart::QueueType,
        uart_rx: uart::QueueType,
        radio_tx: radio::QueueType,
        radio_rx: radio::QueueType,
    }

    impl Queues {
        const fn new() -> Self {
            Self {
                uart_tx: Queue::new(),
                uart_rx: Queue::new(),
                radio_tx: Queue::new(),
                radio_rx: Queue::new(),
            }
        }
    }

    #[shared]
    struct Shared {
        now: AtomicU32,
    }

    #[local]
    struct Local {
        uart: Uart,
        radio: Radio,
        radio_protocol: RadioProtocol,
        rtc: Rtc,
    }

    #[init(local = [queues: Queues = Queues::new()])]
    fn init(cx: init::Context) -> (Shared, Local) {
        let rtc = Rtc::new(cx.device.RTC0);
        rtc.init(&cx.device.CLOCK);

        let (uart_tx_producer, uart_tx_consumer) = cx.local.queues.uart_tx.split();
        let (uart_rx_producer, uart_rx_consumer) = cx.local.queues.uart_rx.split();
        let uart = Uart::new(cx.device.UART0, uart_tx_consumer, uart_rx_producer);
        uart.init(&cx.device.GPIO, TX_PIN, RX_PIN);

        let (radio_tx_producer, radio_tx_consumer) = cx.local.queues.radio_tx.split();
        let (radio_rx_producer, radio_rx_consumer) = cx.local.queues.radio_rx.split();
        let radio = Radio::new(cx.device.RADIO, radio_tx_consumer, radio_rx_producer);
        radio.init(&cx.device.CLOCK);

        let radio_protocol = RadioProtocol::new(
            radio_rx_consumer,
            radio_tx_producer,
            uart_rx_consumer,
            uart_tx_producer,
        );

        (
            Shared {
                now: AtomicU32::new(0),
            },
            Local {
                rtc,
                uart,
                radio,
                radio_protocol,
                // echo,
            },
        )
    }

    #[task(binds = UART0, priority = 1, local = [uart])]
    fn uart_task(cx: uart_task::Context) {
        let uart = cx.local.uart;
        if uart.handle_interrupt() {
            // echo_task::spawn().unwrap();
            let _ = radio_protocol_task::spawn();
        }
    }

    #[task(binds = RADIO, priority = 2, local = [radio])]
    fn radio_task(cx: radio_task::Context) {
        let radio = cx.local.radio;
        if radio.handle_interrupt() {
            let _ = radio_protocol_task::spawn();
        }
    }

    #[task(binds = RTC0, priority = 2, local = [rtc], shared = [&now])]
    fn rtc0_task(cx: rtc0_task::Context) {
        let rtc = cx.local.rtc;
        let now = rtc.run();
        if (now % 1000) == 0 {
            trace!("RTC0 tick {=u32}", now);
        }
        cx.shared.now.store(now, Ordering::Relaxed);
        let _ = radio_protocol_task::spawn();
    }

    #[task(priority = 3, local = [radio_protocol], shared = [&now])]
    async fn radio_protocol_task(cx: radio_protocol_task::Context) {
        let radio_protocol = cx.local.radio_protocol;
        let now = cx.shared.now.load(Ordering::Relaxed);
        match radio_protocol.run(now) {
            Pend::Nothing => {}
            Pend::Radio => rtic::pend(Interrupt::RADIO),
            Pend::Uart => rtic::pend(Interrupt::UART0),
            Pend::Both => {
                rtic::pend(Interrupt::UART0);
                rtic::pend(Interrupt::RADIO);
            }
        }
    }
}
