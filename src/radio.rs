use crate::{radio, uart};
use defmt::{info, trace, warn};
use heapless::spsc::{Consumer, Producer, Queue};
use nrf51_hal::pac::{CLOCK, RADIO};

const MAX_DATA_SIZE: usize = 60;
const MIN_PACKET_SIZE: usize = 3;
pub const MAX_PACKET_SIZE: usize = 64;

pub type PacketBuffer = [u8; MAX_PACKET_SIZE];

pub type QueueType = Queue<Packet, 8>;
pub type ConsumerType = Consumer<'static, Packet, 8>;
pub type ProducerType = Producer<'static, Packet, 8>;

enum Mode {
    Idle,
    Rx,
    Tx,
    TxDisable,
}

pub struct Radio {
    radio: RADIO,
    packet: PacketBuffer,
    mode: Mode,
    tx: ConsumerType,
    rx: ProducerType,
}

impl Radio {
    pub fn new(radio: RADIO, tx: ConsumerType, rx: ProducerType) -> Self {
        Self {
            radio,
            packet: [0; MAX_PACKET_SIZE],
            mode: Mode::Idle,
            tx,
            rx,
        }
    }

    pub fn init(&self, clock: &CLOCK) {
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

        self.radio
            .intenset
            .write(|w| w.ready().set().address().set().end().set().disabled().set());

        self.radio.tasks_rxen.write(|w| unsafe { w.bits(1) });

        info!("Radio initialized");
    }

    /// Returns true if a packet was received
    pub fn handle_interrupt(&mut self) -> bool {
        if self.radio.events_ready.read().bits() != 0 {
            // Make sure PACKETPTR is set before starting the radio
            self.radio.events_ready.write(|w| unsafe { w.bits(0) });
            self.radio
                .packetptr
                .write(|w| unsafe { w.bits(self.packet.as_ptr() as u32) });
            self.radio.tasks_start.write(|w| unsafe { w.bits(1) });
        }
        match &self.mode {
            Mode::Idle => {
                if self.radio.events_address.read().bits() != 0 {
                    trace!("radio: receiving");
                    self.radio.events_address.write(|w| unsafe { w.bits(0) });
                    self.mode = Mode::Rx;
                } else if let Some(packet) = self.tx.dequeue() {
                    trace!("radio: gonna transmit bytes {}", packet);
                    packet.write(&mut self.packet);
                    self.radio.tasks_disable.write(|w| unsafe { w.bits(1) });
                    self.mode = Mode::Tx;
                }
            }
            Mode::Rx => {
                if self.radio.events_end.read().bits() != 0 {
                    self.radio.events_end.write(|w| unsafe { w.bits(0) });
                    let mut result = false;
                    if self.radio.crcstatus.read().crcstatus().is_crcok() {
                        // CRC ok
                        trace!("radio: crc ok, data: {}", self.packet);
                        trace!("radio: PACKETPTR {=u32:x}", self.packet.as_ptr() as u32);
                        if let Some(packet) = Packet::read(&self.packet) {
                            if self.rx.enqueue(packet).is_err() {
                                warn!("radio: rx queue full");
                            }
                        } else {
                            warn!("radio: received malformed packet {}", self.packet);
                        }
                        result = true
                    } else {
                        warn!("radio: crc error");
                    };
                    self.radio.tasks_start.write(|w| unsafe { w.bits(1) });
                    trace!("radio: receive done - restarted rx");
                    self.mode = Mode::Idle;
                    return result;
                }
            }
            Mode::Tx => {
                if self.radio.events_disabled.read().bits() != 0 {
                    trace!("radio: rx disabled");
                    self.radio.events_disabled.write(|w| unsafe { w.bits(0) });
                    self.radio.tasks_txen.write(|w| unsafe { w.bits(1) });
                } else if self.radio.events_end.read().bits() != 0 {
                    trace!("radio: tx done");
                    self.radio.events_address.write(|w| unsafe { w.bits(0) });
                    self.radio.events_end.write(|w| unsafe { w.bits(0) });
                    self.radio.tasks_disable.write(|w| unsafe { w.bits(1) });
                    self.mode = Mode::TxDisable;
                }
            }
            Mode::TxDisable => {
                if self.radio.events_disabled.read().bits() != 0 {
                    trace!("radio: tx disabled");
                    self.radio.events_disabled.write(|w| unsafe { w.bits(0) });
                    self.radio.tasks_rxen.write(|w| unsafe { w.bits(1) });
                    self.mode = Mode::Idle;
                }
            }
        }
        false
    }
}

#[derive(Clone, Copy)]
pub struct PacketData {
    pub id: u8,
    pub data_len: u8,
    pub data: [u8; MAX_DATA_SIZE],
}

impl PacketData {
    pub fn from_consumer(id: u8, queue: &mut uart::ConsumerType) -> Self {
        let mut data = [0; MAX_DATA_SIZE];
        let mut len = 0;
        while let Some(c) = queue.dequeue() {
            data[len] = c;
            len += 1;
            if len >= MAX_DATA_SIZE {
                break;
            }
        }
        Self {
            id,
            data_len: len as u8,
            data,
        }
    }

    pub fn iter(&self) -> core::slice::Iter<'_, u8> {
        self.data[..self.data_len as usize].iter()
    }
}

#[derive(Clone, Copy)]
pub enum Packet {
    Ack(u8),
    Data(PacketData),
    Both(u8, PacketData),
}

impl Packet {
    fn read(source: &[u8]) -> Option<Self> {
        let len = source[0];
        if (len as usize) < MIN_PACKET_SIZE || (len as usize) > radio::MAX_PACKET_SIZE {
            None
        } else {
            match source[1] {
                b'A' => Some(Self::Ack(source[2])),
                b'D' => {
                    let mut data = [0; MAX_DATA_SIZE];
                    data[..(len as usize - 3)].copy_from_slice(&source[3..(len as usize)]);
                    Some(Self::Data(PacketData {
                        id: source[2],
                        data_len: len - 3,
                        data,
                    }))
                }
                b'X' => {
                    let mut data = [0; MAX_DATA_SIZE];
                    data[..(len as usize - 4)].copy_from_slice(&source[4..(len as usize)]);
                    Some(Self::Both(
                        source[2],
                        PacketData {
                            id: source[3],
                            data_len: len - 4,
                            data,
                        },
                    ))
                }
                _ => None,
            }
        }
    }

    fn write(&self, target: &mut [u8]) {
        match self {
            Packet::Ack(ack) => {
                target[0] = 3;
                target[1] = b'A';
                target[2] = *ack;
            }
            Packet::Data(PacketData { id, data_len, data }) => {
                target[0] = data_len + 3;
                target[1] = b'D';
                target[2] = *id;
                target[3..(*data_len as usize + 3)].copy_from_slice(&data[0..*data_len as usize]);
            }
            Packet::Both(ack_id, PacketData { id, data_len, data }) => {
                target[0] = data_len + 4;
                target[1] = b'X';
                target[2] = *ack_id;
                target[3] = *id;
                target[4..(*data_len as usize + 4)].copy_from_slice(&data[0..*data_len as usize]);
            }
        }
    }

    fn trace_assembled(&self) {
        match self {
            Packet::Ack(ack) => {
                trace!("radio: assembled packet: A ack={=u8}", ack);
            }
            Packet::Data(PacketData { id, data_len, .. }) => {
                trace!(
                    "radio: assembled packet: D id={=u8} data_len={=u8}",
                    id,
                    data_len
                );
            }
            Packet::Both(ack, PacketData { id, data_len, .. }) => {
                trace!(
                    "radio: assembled packet: X ack={=u8} id={=u8} data_len={=u8}",
                    ack,
                    id,
                    data_len
                );
            }
        }
    }

    pub fn trace_received(&self) {
        match self {
            Packet::Ack(ack) => {
                trace!("radio: received packet: A ack={=u8}", ack);
            }
            Packet::Data(PacketData { id, data_len, .. }) => {
                trace!(
                    "radio: received packet: D id={=u8} data_len={=u8}",
                    id,
                    data_len
                );
            }
            Packet::Both(ack, PacketData { id, data_len, .. }) => {
                trace!(
                    "radio: received packet: X ack={=u8} id={=u8} data_len={=u8}",
                    ack,
                    id,
                    data_len
                );
            }
        }
    }
}
