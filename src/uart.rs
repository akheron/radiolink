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
    tx_queue: Queue<u8, 1024>, // TODO: these can probably be much smaller, do measurements
    rx_queue: Queue<u8, 1024>,
}

impl Uart {
    pub fn new(uart0: UART0) -> Self {
        Self {
            uart0,
            tx_state: Idle,
            tx_queue: Queue::new(),
            rx_queue: Queue::new(),
        }
    }

    pub fn init(&self, gpio: &GPIO, tx_pin: u32, rx_pin: u32) {
        gpio.pin_cnf[tx_pin as usize].write(|w| w.pull().pullup().dir().output());
        gpio.pin_cnf[rx_pin as usize].write(|w| w.pull().disabled().dir().input());

        self.uart0.pseltxd.write(|w| unsafe { w.bits(tx_pin) });
        self.uart0.pselrxd.write(|w| unsafe { w.bits(rx_pin) });
        self.uart0.baudrate.write(|w| w.baudrate().baud115200());
        self.uart0.enable.write(|w| w.enable().enabled());

        self.uart0.tasks_startrx.write(|w| unsafe { w.bits(1) });
        debug!("UART initialized");
    }

    pub fn tick(&mut self) {
        self.tx_state = match self.tx_state {
            Idle => {
                if let Some(c) = self.tx_queue.dequeue() {
                    debug!("uart - Sending {:x}", c);
                    self.uart0.tasks_starttx.write(|w| unsafe { w.bits(1) });
                    self.uart0.txd.write(|w| unsafe { w.txd().bits(c) });
                    Tx
                } else {
                    Idle
                }
            }
            Tx => {
                if self.uart0.events_txdrdy.read().bits() != 0 {
                    debug!("uart - Send finished");
                    if let Some(c) = self.tx_queue.dequeue() {
                        debug!(
                            "uart - Sending {:x} right away, queue size {}",
                            c,
                            self.tx_queue.len()
                        );
                        self.uart0.txd.write(|w| unsafe { w.txd().bits(c) });
                        Tx
                    } else {
                        self.uart0.tasks_stoptx.write(|w| unsafe { w.bits(1) });
                        debug!("uart - Nothing to send");
                        Idle
                    }
                } else {
                    Tx
                }
            }
        };

        // Receive all available bytes
        while self.uart0.events_rxdrdy.read().bits() != 0 {
            self.uart0.events_rxdrdy.write(|w| unsafe { w.bits(0) });
            self.rx_queue
                .enqueue(self.uart0.rxd.read().bits() as u8)
                .unwrap();
            debug!("uart - Received {:x}", self.rx_queue.peek().unwrap());
        }
    }

    pub fn read(&mut self) -> Option<u8> {
        self.rx_queue.dequeue()
    }

    pub fn write(&mut self, c: u8) {
        self.tx_queue.enqueue(c).unwrap();
    }
}
