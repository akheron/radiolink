use defmt::{info, trace, warn};
use heapless::spsc::{Consumer, Producer, Queue};
use nrf51_hal::pac::{GPIO, UART0};

const QUEUE_SIZE: usize = 2048;

pub type QueueType = Queue<u8, QUEUE_SIZE>;
pub type ProducerType = Producer<'static, u8, QUEUE_SIZE>;
pub type ConsumerType = Consumer<'static, u8, QUEUE_SIZE>;

pub struct Uart {
    uart0: UART0,
    txing: bool,
    tx: ConsumerType,
    rx: ProducerType,
}

impl Uart {
    pub fn new(uart0: UART0, tx: ConsumerType, rx: ProducerType) -> Self {
        Self {
            uart0,
            tx,
            rx,
            txing: false,
        }
    }

    pub fn init(&self, gpio: &GPIO, tx_pin: u32, rx_pin: u32) {
        gpio.pin_cnf[tx_pin as usize].write(|w| w.pull().pullup().dir().output());
        gpio.pin_cnf[rx_pin as usize].write(|w| w.pull().disabled().dir().input());

        let uart0 = &self.uart0;
        uart0.pseltxd.write(|w| unsafe { w.bits(tx_pin) });
        uart0.pselrxd.write(|w| unsafe { w.bits(rx_pin) });
        uart0.baudrate.write(|w| w.baudrate().baud38400());
        uart0
            .intenset
            .write(|w| w.txdrdy().bit(true).rxdrdy().bit(true));
        uart0.enable.write(|w| w.enable().enabled());

        uart0.tasks_startrx.write(|w| unsafe { w.bits(1) });
        uart0.tasks_starttx.write(|w| unsafe { w.bits(1) });

        info!("UART initialized");
    }

    /// Returns true if data was written to the RX queue
    pub fn handle_interrupt(&mut self) -> bool {
        let uart0 = &self.uart0;
        if uart0.events_txdrdy.read().bits() != 0 {
            uart0.events_txdrdy.write(|w| unsafe { w.bits(0) });
            trace!("uart: txdrdy");
            self.txing = false;
        }
        if !self.txing {
            trace!("uart: try_recv");
            if let Some(c) = self.tx.dequeue() {
                trace!("tx: {=u8:x}", c);
                uart0.txd.write(|w| unsafe { w.txd().bits(c) });
                self.txing = true;
            }
        }
        if uart0.events_rxdrdy.read().bits() != 0 {
            uart0.events_rxdrdy.write(|w| unsafe { w.bits(0) });
            trace!("uart: rxdrdy");
            let byte = uart0.rxd.read().bits() as u8;
            trace!("uart: rx {=u8:x}", byte);
            if self.rx.enqueue(byte).is_err() {
                warn!("uart: rx queue full");
            }
            trace!("uart: enqueued {=u8:x}", byte);
            true
        } else {
            false
        }
    }
}
