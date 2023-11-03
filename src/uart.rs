use defmt::debug;
use heapless::spsc::Queue;
use microbit::pac::{GPIO, UART0};

#[derive(PartialEq, Eq)]
enum TxState {
    Idle,
    Tx,
}
use TxState::*;

pub struct Uart {
    uart0: UART0,
    tx_state: TxState,
}

impl Uart {
    pub fn new(uart0: UART0) -> Self {
        Self {
            uart0,
            tx_state: Idle,
        }
    }

    pub fn init(&self, gpio: &GPIO, tx_pin: u32, rx_pin: u32) {
        gpio.pin_cnf[tx_pin as usize].write(|w| w.pull().pullup().dir().output());
        gpio.pin_cnf[rx_pin as usize].write(|w| w.pull().disabled().dir().input());

        self.uart0.pseltxd.write(|w| unsafe { w.bits(tx_pin) });
        self.uart0.pselrxd.write(|w| unsafe { w.bits(rx_pin) });
        self.uart0.baudrate.write(|w| w.baudrate().baud9600());
        self.uart0.enable.write(|w| w.enable().enabled());

        self.uart0.tasks_startrx.write(|w| unsafe { w.bits(1) });
        self.uart0.tasks_starttx.write(|w| unsafe { w.bits(1) });

        debug!("UART initialized");
    }

    pub fn tick(
        &mut self,
        _now: u32,
        tx_queue: &mut Queue<u8, 1024>,
        rx_queue: &mut Queue<u8, 1024>,
    ) {
        self.tx_state = match self.tx_state {
            Idle => {
                if let Some(c) = tx_queue.dequeue() {
                    debug!(
                        "uart - first write {=u8:x}, queue size {=usize}",
                        c,
                        tx_queue.len()
                    );
                    self.uart0.txd.write(|w| unsafe { w.txd().bits(c) });
                    Tx
                } else {
                    Idle
                }
            }
            Tx => {
                if self.uart0.events_txdrdy.read().bits() != 0 {
                    if let Some(c) = tx_queue.dequeue() {
                        debug!(
                            "uart - write {=u8:x}, queue size {=usize}",
                            c,
                            tx_queue.len()
                        );
                        self.uart0.events_txdrdy.write(|w| unsafe { w.bits(0) });
                        self.uart0.txd.write(|w| unsafe { w.txd().bits(c) });
                    }
                }
                Tx
            }
        };

        while self.uart0.events_rxdrdy.read().bits() != 0 {
            self.uart0.events_rxdrdy.write(|w| unsafe { w.bits(0) });
            let byte = self.uart0.rxd.read().bits() as u8;
            rx_queue.enqueue(byte).unwrap();
            debug!(
                "uart - read {=u8:x}, queue size {=usize}",
                byte,
                rx_queue.len()
            );
        }
    }
}
