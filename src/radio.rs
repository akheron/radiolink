use defmt::{debug, Format};
use heapless::spsc::Queue;
use microbit::pac::{CLOCK, RADIO};

const MAX_DATA_SIZE: usize = 64;
const MIN_PACKET_SIZE: usize = 3;
const MAX_PACKET_SIZE: usize = MAX_DATA_SIZE + 4;

#[derive(Clone, Copy, PartialEq, Eq, Format)]
enum RadioState {
    Uninitialized,
    RxIdle,
    Rx,
    RxDisable,
    Tx,
    TxDisable,
}

enum RxState {
    Initial,
    PendingAck { id: u8 },
    LastAck { id: u8 },
}

enum TxState {
    Idle,
    WaitingForAck {
        id: u8,
        data_len: u8,
        data: [u8; MAX_DATA_SIZE],
    },
}

impl TxState {
    fn waiting_for_ack(id: u8, data: &[u8]) -> Self {
        let mut data_len = data.len();
        if data_len > MAX_DATA_SIZE {
            data_len = MAX_DATA_SIZE;
        }
        let mut packet_data = [0; MAX_DATA_SIZE];
        packet_data[0..data_len].copy_from_slice(data);
        Self::WaitingForAck {
            id,
            data_len: data_len as u8,
            data: packet_data,
        }
    }
}

pub struct Radio {
    radio: RADIO,
    packet: [u8; MAX_PACKET_SIZE],
    next_packet_id: u8,
    radio_state: RadioState,
    rx_state: RxState,
    tx_state: TxState,
    tx_queue: Queue<u8, 1024>,
    rx_queue: Queue<u8, 1024>,
}

impl Radio {
    pub fn new(radio: RADIO) -> Self {
        Self {
            radio,
            packet: [0; MAX_PACKET_SIZE],
            next_packet_id: 0,
            radio_state: RadioState::Uninitialized,
            rx_state: RxState::Initial,
            tx_state: TxState::Idle,
            tx_queue: Queue::new(),
            rx_queue: Queue::new(),
        }
    }

    pub fn init(&mut self, clock: &CLOCK) {
        clock.events_hfclkstarted.write(|w| unsafe { w.bits(0) });
        clock.tasks_hfclkstart.write(|w| unsafe { w.bits(1) });
        while clock.events_hfclkstarted.read().bits() == 0 {}

        // Configure radio to match microbit defaults
        self.radio.txpower.write(|w| w.txpower().pos4d_bm()); // +4 dBm
        self.radio.frequency.write(|w| unsafe { w.bits(7) }); // Default channel: 7
        self.radio.mode.write(|w| w.mode().nrf_1mbit()); // Default data rate: 1 Mbps
        self.radio.base0.write(|w| unsafe { w.bits(0x75626974) }); // "uBit"
        self.radio.prefix0.write(|w| unsafe { w.bits(0) });
        self.radio.txaddress.write(|w| unsafe { w.bits(0) }); // Transmit on logical address 0
        self.radio.rxaddresses.write(|w| w.addr0().enabled()); // Enable reception on logical address 0 only
        self.radio.pcnf0.write(|w| unsafe {
            w.lflen()
                .bits(8) // 8-bit length field
                .s0len()
                .bit(false) // No S0 field
                .s1len()
                .bits(0) // No S1 field
        });
        self.radio.pcnf1.write(|w| unsafe {
            w.maxlen()
                .bits((MAX_PACKET_SIZE - 1) as u8) // Maximum payload
                .statlen()
                .bits(0)
                .balen()
                .bits(4) // 4-byte base address length
                .endian()
                .little() // Little endian payload
                .whiteen()
                .enabled() // Enable packet whitening
        });
        self.radio.crccnf.write(|w| w.len().two()); // 16-bit CRC
        self.radio.crcinit.write(|w| unsafe { w.bits(0xFFFF) }); // CRC initial value
        self.radio.crcpoly.write(|w| unsafe { w.bits(0x11021) }); // CRC polynomial
        self.radio.datawhiteiv.write(|w| unsafe { w.bits(0x18) }); // Initial value for the data whitening algorithm

        let packet_ptr = self.packet.as_ptr() as u32;
        self.radio
            .packetptr
            .write(|w| unsafe { w.bits(packet_ptr) });

        // Shortcut READY -> START
        self.radio.shorts.write(|w| w.ready_start().enabled());

        self.radio.tasks_rxen.write(|w| unsafe { w.bits(1) });
        self.radio_state = RadioState::RxIdle;

        debug!("Radio initialized");
    }

    pub fn tick(&mut self) {
        self.radio_state = match self.radio_state {
            RadioState::Uninitialized => RadioState::Uninitialized,
            RadioState::RxIdle => {
                if self.radio.events_address.read().bits() != 0 {
                    debug!("radio - receiving");
                    self.radio.events_address.write(|w| unsafe { w.bits(0) });
                    RadioState::Rx
                } else if self.assemble_packet() {
                    debug!("radio - disable rx");
                    self.radio.tasks_disable.write(|w| unsafe { w.bits(1) });
                    RadioState::RxDisable
                } else {
                    RadioState::RxIdle
                }
            }
            RadioState::Rx => {
                if self.radio.events_end.read().bits() != 0 {
                    self.radio.events_end.write(|w| unsafe { w.bits(0) });
                    if self.radio.crcstatus.read().crcstatus().is_crcok() {
                        // CRC ok
                        self.disassemble_packet();
                    }
                    self.radio.tasks_start.write(|w| unsafe { w.bits(1) });
                    debug!("radio - receive done - restarted rx");
                    RadioState::RxIdle
                } else {
                    RadioState::Rx
                }
            }
            RadioState::RxDisable => {
                if self.radio.events_disabled.read().bits() != 0 {
                    debug!("radio - rx disabled");
                    self.radio.events_disabled.write(|w| unsafe { w.bits(0) });
                    self.radio.tasks_txen.write(|w| unsafe { w.bits(1) });
                    RadioState::Tx
                } else {
                    RadioState::RxDisable
                }
            }
            RadioState::Tx => {
                if self.radio.events_end.read().bits() != 0 {
                    debug!("radio - tx done");
                    self.radio.events_address.write(|w| unsafe { w.bits(0) });
                    self.radio.events_end.write(|w| unsafe { w.bits(0) });
                    self.radio.tasks_disable.write(|w| unsafe { w.bits(1) });
                    RadioState::TxDisable
                } else {
                    RadioState::Tx
                }
            }
            RadioState::TxDisable => {
                if self.radio.events_disabled.read().bits() != 0 {
                    debug!("radio - tx disabled");
                    self.radio.events_disabled.write(|w| unsafe { w.bits(0) });
                    self.radio.tasks_rxen.write(|w| unsafe { w.bits(1) });
                    RadioState::RxIdle
                } else {
                    RadioState::TxDisable
                }
            }
        };
    }

    pub fn write(&mut self, byte: u8) {
        self.tx_queue.enqueue(byte).unwrap();
    }

    pub fn read(&mut self) -> Option<u8> {
        self.rx_queue.dequeue()
    }

    fn handle_rx_ack(&mut self, ack: u8) {
        if let TxState::WaitingForAck { id, .. } = self.tx_state {
            if id == ack {
                self.tx_state = TxState::Idle;
            }
        }
    }

    fn handle_rx_data(&mut self, id: u8, start: usize, end: usize) {
        match self.rx_state {
            RxState::Initial => {
                // This is the first received data
                for &byte in self.packet[start..end].iter() {
                    self.rx_queue.enqueue(byte).unwrap();
                }
                self.rx_state = RxState::PendingAck { id };
            }
            RxState::LastAck { id: last_acked_id } => {
                if id != last_acked_id {
                    for &byte in self.packet[start..end].iter() {
                        self.rx_queue.enqueue(byte).unwrap();
                    }
                    self.rx_state = RxState::PendingAck { id };
                }
            }
            RxState::PendingAck { .. } => {
                // Got a new packet before sending an ack => must be a retransmit
            }
        }
    }

    fn disassemble_packet(&mut self) {
        let len = self.packet[0];
        if (len as usize) < MIN_PACKET_SIZE || (len as usize) > MAX_PACKET_SIZE {
            return;
        }

        let packet_type = self.packet[1];
        if packet_type == b'D' {
            debug!("Disassembled packet: {} D {}", len, self.packet[2]);
            self.handle_rx_data(self.packet[2], 3, len as usize);
        } else if packet_type == b'A' {
            debug!("Disassembled packet: {} A {}", len, self.packet[2]);
            self.handle_rx_ack(self.packet[2]);
        } else if packet_type == b'X' {
            debug!(
                "Disassembled packet: {} X {} {}",
                len, self.packet[2], self.packet[3]
            );
            self.handle_rx_ack(self.packet[2]);
            self.handle_rx_data(self.packet[3], 4, len as usize);
        } else {
            debug!("Unknown packet type: {}", packet_type);
        }
    }

    fn assemble_packet(&mut self) -> bool {
        match (&self.rx_state, &self.tx_state) {
            (_, TxState::WaitingForAck { .. }) => {
                // Waiting for ack to the last tx packet => don't send more
                false
            }
            (&RxState::PendingAck { id: ack_id }, _) if !self.tx_queue.is_empty() => {
                // Waiting to ack the last rx packet and has data to transmit => send ack and data
                self.packet[1] = b'X';
                self.packet[2] = ack_id;
                self.packet[3] = self.next_packet_id;
                self.next_packet_id = self.next_packet_id.wrapping_add(1);
                let mut len = 4;
                while len < MAX_PACKET_SIZE && !self.tx_queue.is_empty() {
                    self.packet[len] = self.tx_queue.dequeue().unwrap();
                    len += 1;
                }
                self.packet[0] = len as u8;
                self.rx_state = RxState::LastAck { id: ack_id };
                self.tx_state = TxState::waiting_for_ack(self.packet[3], &self.packet[4..len]);
                debug!("Assembled packet: {} X {} {}", len, ack_id, self.packet[3]);
                true
            }
            (&RxState::PendingAck { id }, _) if self.tx_queue.is_empty() => {
                // Waiting to ack the last rx packet and no data to transmit => send ack
                self.packet[0] = 3;
                self.packet[1] = b'A';
                self.packet[2] = id;
                self.rx_state = RxState::LastAck { id };
                debug!("Assembled packet: 3 A {}", id);
                true
            }
            _ if !self.tx_queue.is_empty() => {
                // Has data to transmit => send data
                self.packet[1] = b'D';
                self.packet[2] = self.next_packet_id;
                self.next_packet_id = self.next_packet_id.wrapping_add(1);
                let mut len = 3;
                while len < MAX_PACKET_SIZE && !self.tx_queue.is_empty() {
                    self.packet[len] = self.tx_queue.dequeue().unwrap();
                    len += 1;
                }
                self.packet[0] = len as u8;
                self.tx_state = TxState::waiting_for_ack(self.packet[2], &self.packet[3..len]);
                debug!("Assembled packet: {} D {}", len, self.packet[2]);
                true
            }
            _ => false,
        }
    }
}
