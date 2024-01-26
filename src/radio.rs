use crate::queue::Queue;
use defmt::{debug, Format};
use microbit::pac::{CCM, CLOCK, PPI, RADIO};

const MIN_PAYLOAD_SIZE: usize = 1;
const MAX_PAYLOAD_SIZE: usize = 27;
const MAX_PACKET_SIZE: usize = MAX_PAYLOAD_SIZE + 5;

#[derive(Clone, Copy, PartialEq, Eq, Format)]
enum RadioState {
    Uninitialized,
    RxIdle,
    Rx,
    RxDisable,
    Tx,
    TxDisable,
}

#[derive(Clone, Copy)]
enum ConnectionState {
    Probing,
    Client,
    Server,
}

#[derive(Clone, Copy)]
enum RxState {
    Initial,
    NeedsAck { id: u8 },
    Acked { id: u8 },
}

impl RxState {
    fn debug(&self) {
        match self {
            RxState::Initial => {
                debug!("radio - rx_state: Initial",);
            }
            RxState::NeedsAck { id } => {
                debug!("radio - rx_state: NeedsAck id={=u8}", id);
            }
            RxState::Acked { id } => {
                debug!("radio - rx_state: Acked id={=u8}", id);
            }
        }
    }
}

#[derive(Clone, Copy)]
enum TxState {
    Idle,
    Sent {
        packet_data: PacketData,
        tx_count: u32,
        since: u32,
    },
}

impl TxState {
    fn debug(&self) {
        match self {
            TxState::Idle => {
                debug!("radio - tx_state: Idle",);
            }
            TxState::Sent {
                packet_data: PacketData { id, .. },
                tx_count,
                since,
            } => {
                debug!(
                    "radio - tx_state: Sent id={=u8} tx_count={=u32} since={=u32}",
                    id, tx_count, since
                );
            }
        }
    }
}

#[derive(Clone, Copy)]
struct PacketData {
    id: u8,
    data_len: u8,
    data: [u8; MAX_PAYLOAD_SIZE],
}

impl PacketData {
    fn from_queue(id: u8, queue: &mut Queue) -> Self {
        let mut data = [0; MAX_PAYLOAD_SIZE];
        let mut len = 0;
        while len < MAX_PAYLOAD_SIZE && !queue.is_empty() {
            data[len] = queue.dequeue().unwrap();
            len += 1;
        }
        Self {
            id,
            data_len: len as u8,
            data,
        }
    }

    fn iter(&self) -> core::slice::Iter<'_, u8> {
        self.data[..self.data_len as usize].iter()
    }
}

enum Packet {
    Ack(u8),
    Data(PacketData),
    Both(u8, PacketData),
}

const S0: usize = 0;
const LEN: usize = 1;
const S1: usize = 2;
const PAYLOAD: usize = 3;

impl Packet {
    fn read(source: &[u8]) -> Option<Self> {
        if (source[LEN] as usize) < MIN_PAYLOAD_SIZE || (source[LEN] as usize) > MAX_PAYLOAD_SIZE {
            None
        } else {
            match source[S0] {
                b'A' => Some(Self::Ack(source[PAYLOAD])),
                b'D' => {
                    let data_len = source[LEN] - 1;
                    let id = source[PAYLOAD];
                    let mut data = [0; MAX_PAYLOAD_SIZE];
                    data[..(data_len as usize)]
                        .copy_from_slice(&source[(PAYLOAD + 1)..(data_len as usize + PAYLOAD + 1)]);
                    Some(Self::Data(PacketData { id, data_len, data }))
                }
                b'X' => {
                    let data_len = source[LEN] - 2;
                    let ack_id = source[PAYLOAD];
                    let id = source[PAYLOAD + 1];
                    let mut data = [0; MAX_PAYLOAD_SIZE];
                    data[..(data_len as usize)]
                        .copy_from_slice(&source[(PAYLOAD + 2)..(PAYLOAD + 2 + data_len as usize)]);
                    Some(Self::Both(ack_id, PacketData { id, data_len, data }))
                }
                _ => None,
            }
        }
    }

    fn write(&self, target: &mut [u8]) {
        match self {
            Packet::Ack(ack) => {
                target[S0] = b'A';
                target[LEN] = 1;
                target[S1] = 0;
                target[PAYLOAD] = *ack;
            }
            Packet::Data(PacketData { id, data_len, data }) => {
                target[S0] = b'D';
                target[LEN] = *data_len + 1;
                target[S1] = 0;
                target[PAYLOAD] = *id;
                target[(PAYLOAD + 1)..(PAYLOAD + 1 + *data_len as usize)]
                    .copy_from_slice(&data[0..*data_len as usize]);
            }
            Packet::Both(ack_id, PacketData { id, data_len, data }) => {
                target[S0] = b'X';
                target[LEN] = *data_len + 2;
                target[S1] = 0;
                target[PAYLOAD] = *ack_id;
                target[PAYLOAD + 1] = *id;
                target[(PAYLOAD + 2)..(PAYLOAD + 2 + *data_len as usize)]
                    .copy_from_slice(&data[0..*data_len as usize]);
            }
        }
    }

    fn debug_assembled(&self) {
        match self {
            Packet::Ack(ack) => {
                debug!("radio - assembled packet: A ack={=u8}", ack);
            }
            Packet::Data(PacketData { id, data_len, .. }) => {
                debug!(
                    "radio - assembled packet: D id={=u8} data_len={=u8}",
                    id, data_len
                );
            }
            Packet::Both(ack, PacketData { id, data_len, .. }) => {
                debug!(
                    "radio - assembled packet: X ack={=u8} id={=u8} data_len={=u8}",
                    ack, id, data_len
                );
            }
        }
    }

    fn debug_received(&self) {
        match self {
            Packet::Ack(ack) => {
                debug!("radio - received packet: A ack={=u8}", ack);
            }
            Packet::Data(PacketData { id, data_len, .. }) => {
                debug!(
                    "radio - received packet: D id={=u8} data_len={=u8}",
                    id, data_len
                );
            }
            Packet::Both(ack, PacketData { id, data_len, .. }) => {
                debug!(
                    "radio - received packet: X ack={=u8} id={=u8} data_len={=u8}",
                    ack, id, data_len
                );
            }
        }
    }
}

pub struct Radio {
    radio: RADIO,
    ccm: CCM,
    ppi: PPI,
    packet: [u8; MAX_PAYLOAD_SIZE],
    plaintext: [u8; MAX_PAYLOAD_SIZE],
    ccm_config: [u8; 33],
    ccm_scratch: [u8; 16 + MAX_PACKET_SIZE],
    next_packet_id: u8,
    radio_state: RadioState,
    connection_state: ConnectionState,
    rx_state: RxState,
    tx_state: TxState,
}

impl Radio {
    pub fn new(radio: RADIO, ccm: CCM, ppi: PPI) -> Self {
        Self {
            radio,
            ccm,
            ppi,
            packet: [0; MAX_PAYLOAD_SIZE],
            plaintext: [0; MAX_PAYLOAD_SIZE],
            ccm_config: [
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 0xa, 0xb, 0xc, 0xd, 0xe, 0xf, // AES key
                0, 0, 0, 0, 0, 0, 0, 0, // Counter
                0, // Direction
                7, 6, 5, 4, 3, 2, 1, 0, // IV
            ],
            ccm_scratch: [0; 16 + MAX_PACKET_SIZE],
            next_packet_id: 0,
            radio_state: RadioState::Uninitialized,
            connection_state: ConnectionState::Probing,
            rx_state: RxState::Initial,
            tx_state: TxState::Idle,
        }
    }

    pub fn init(&mut self, clock: &CLOCK) {
        clock.events_hfclkstarted.write(|w| unsafe { w.bits(0) });
        clock.tasks_hfclkstart.write(|w| unsafe { w.bits(1) });
        while clock.events_hfclkstarted.read().bits() == 0 {}
        debug!("High frequency clock started");

        // ppi: enable radio READY -> ccm KSGEN (ch24)
        self.ppi.chenset.write(|w| w.ch24().set());

        self.ccm
            .cnfptr
            .write(|w| unsafe { w.bits(self.ccm_config.as_ptr() as u32) });
        self.ccm
            .scratchptr
            .write(|w| unsafe { w.bits(self.ccm_scratch.as_ptr() as u32) });
        self.ccm.enable.write(|w| w.enable().enabled());
        debug!("CCM initialized");

        // Configure radio to match microbit defaults
        self.radio.txpower.write(|w| w.txpower().pos4d_bm()); // +4 dBm
        self.radio.frequency.write(|w| unsafe { w.bits(7) }); // Default channel: 7
        self.radio.mode.write(|w| w.mode().ble_1mbit()); // Data rate: 1 Mbps (BLE)
        self.radio.base0.write(|w| unsafe { w.bits(0x75626974) }); // "uBit"
        self.radio.prefix0.write(|w| unsafe { w.bits(0) });
        self.radio.txaddress.write(|w| unsafe { w.bits(0) }); // Transmit on logical address 0
        self.radio.rxaddresses.write(|w| w.addr0().enabled()); // Enable reception on logical address 0 only
        self.radio.pcnf0.write(|w| unsafe {
            w.s0len()
                .bit(true) // S0 field => CCM header
                .lflen()
                .bits(5) // 5-bit length field
                .s1len()
                .bits(3) // 3-bit RFU field
        });
        self.radio.pcnf1.write(|w| unsafe {
            w.maxlen()
                .bits((MAX_PAYLOAD_SIZE - 1) as u8) // Maximum payload
                .statlen()
                .bits(0)
                .balen()
                .bits(3) // 4-byte base address length
                .endian()
                .little() // Little endian payload
                .whiteen()
                .enabled() // Enable packet whitening
        });
        self.radio.crccnf.write(|w| w.len().three()); // 24-bit CRC
        self.radio.crcinit.write(|w| unsafe { w.bits(0xFFFF) }); // CRC initial value
        self.radio.crcpoly.write(|w| unsafe { w.bits(0x11021) }); // CRC polynomial
        self.radio.datawhiteiv.write(|w| unsafe { w.bits(0x18) }); // Initial value for the data whitening algorithm

        self.radio
            .packetptr
            .write(|w| unsafe { w.bits(self.packet.as_ptr() as u32) });

        // Shortcut READY -> START
        self.radio.shorts.write(|w| w.ready_start().enabled());

        // CCM on-the-fly decryption
        self.ccm.mode.write(|w| w.mode().decryption());
        self.ccm
            .inptr
            .write(|w| unsafe { w.bits(self.packet.as_ptr() as u32) });
        self.ccm
            .outptr
            .write(|w| unsafe { w.bits(self.plaintext.as_ptr() as u32) });
        self.ccm.shorts.write(|w| w.endksgen_crypt().disabled()); // enable shortcut ENDKSGEN -> CRYPT
        self.ppi.chenset.write(|w| w.ch25().set()); // enable radio event ADDRESS -> ccm task CRYPT (ch25)

        // Enable radio RX
        self.radio.tasks_rxen.write(|w| unsafe { w.bits(1) });
        self.radio_state = RadioState::RxIdle;

        debug!("Radio initialized");
    }

    pub fn tick(&mut self, now: u32, tx_queue: &mut Queue, rx_queue: &mut Queue) {
        self.radio_state = match self.radio_state {
            RadioState::Uninitialized => RadioState::Uninitialized,
            RadioState::RxIdle => {
                if self.radio.events_address.read().bits() != 0 {
                    debug!("radio - receiving at {=u32}", now);
                    self.radio.events_address.write(|w| unsafe { w.bits(0) });
                    RadioState::Rx
                } else {
                    let (tx_packet, rx_state, tx_state) = self.assemble_packet(now, tx_queue);
                    self.rx_state = rx_state;
                    self.tx_state = tx_state;

                    if let Some(packet) = tx_packet {
                        packet.debug_assembled();
                        rx_state.debug();
                        tx_state.debug();
                        packet.write(&mut self.plaintext);

                        debug!("radio - disable rx at {=u32}", now);
                        self.radio.tasks_disable.write(|w| unsafe { w.bits(1) });
                        RadioState::RxDisable
                    } else {
                        RadioState::RxIdle
                    }
                }
            }
            RadioState::Rx => {
                if self.radio.events_end.read().bits() != 0 {
                    self.radio.events_end.write(|w| unsafe { w.bits(0) });
                    debug!("radio - received packet: {=[u8]:x}", self.packet);
                    debug!("radio - received plaintext: {=[u8]:x}", self.plaintext);
                    debug!(
                        "radio - crc ok: {=bool} at {=u32}",
                        self.radio.crcstatus.read().crcstatus().is_crcok(),
                        now
                    );
                    debug!(
                        "radio - mic ok: {=bool} at {=u32}",
                        self.ccm.micstatus.read().micstatus().bits(),
                        now
                    );
                    if self.radio.crcstatus.read().crcstatus().is_crcok()
                        && self.ccm.micstatus.read().micstatus().bits()
                    {
                        // CRC & MIC ok
                        if let Some(packet) = Packet::read(&self.plaintext) {
                            match packet {
                                Packet::Ack(ack) => {
                                    // debug!("radio - received ack: {=u8}", ack);
                                    self.handle_rx_ack(ack);
                                    self.ccm_config[16] += 1; // increase counter
                                }
                                Packet::Data(packet_data) => {
                                    // debug!("radio - received data: {=u8}", packet_data.id);
                                    self.handle_rx_data(packet_data, rx_queue);
                                }
                                Packet::Both(ack, packet_data) => {
                                    // debug!("radio - received ack and data: {=u8}", packet_data.id);
                                    self.handle_rx_ack(ack);
                                    self.handle_rx_data(packet_data, rx_queue);
                                }
                            }
                            packet.debug_received();
                            self.rx_state.debug();
                            self.tx_state.debug();
                        } else {
                            debug!("radio - received malformed packet {=[u8]:x}", self.packet);
                        }
                    } else {
                        // CRC or MIC error
                        debug!("radio - crc or mic error");
                    }
                    self.radio.tasks_start.write(|w| unsafe { w.bits(1) });
                    debug!("radio - receive done - restarted rx at {=u32}", now);
                    RadioState::RxIdle
                } else {
                    RadioState::Rx
                }
            }
            RadioState::RxDisable => {
                if self.radio.events_disabled.read().bits() != 0 {
                    debug!("radio - rx disabled at {=u32}", now);
                    self.radio.events_disabled.write(|w| unsafe { w.bits(0) });

                    // CCM on-the-fly encryption
                    self.ccm.mode.write(|w| w.mode().encryption());
                    self.ccm
                        .inptr
                        .write(|w| unsafe { w.bits(self.plaintext.as_ptr() as u32) });
                    self.ccm
                        .outptr
                        .write(|w| unsafe { w.bits(self.packet.as_ptr() as u32) });
                    self.ccm.shorts.write(|w| w.endksgen_crypt().enabled()); // enable shortcut ENDKSGEN -> CRYPT
                    self.ppi.chenclr.write(|w| w.ch25().clear()); // disable ppi radio ADDRESS -> ccm CRYPT (ch25)

                    self.radio.tasks_txen.write(|w| unsafe { w.bits(1) });
                    RadioState::Tx
                } else {
                    RadioState::RxDisable
                }
            }
            RadioState::Tx => {
                if self.radio.events_end.read().bits() != 0 {
                    debug!("radio - tx done at {=u32}", now);
                    debug!("radio - sent plaintext: {=[u8]:x}", self.plaintext);
                    debug!("radio - sent packet: {=[u8]:x}", self.packet);
                    self.radio.events_address.write(|w| unsafe { w.bits(0) });
                    self.radio.events_end.write(|w| unsafe { w.bits(0) });
                    self.radio.tasks_disable.write(|w| unsafe { w.bits(1) });

                    // If ack was sent, increase counter
                    // TODO: it's wasteful to parse the sent plaintext again
                    if let Some(Packet::Ack(_)) = Packet::read(&self.plaintext) {
                        self.ccm_config[16] += 1;
                    }

                    RadioState::TxDisable
                } else {
                    RadioState::Tx
                }
            }
            RadioState::TxDisable => {
                if self.radio.events_disabled.read().bits() != 0 {
                    debug!("radio - tx disabled at {=u32}", now);
                    self.radio.events_disabled.write(|w| unsafe { w.bits(0) });

                    // CCM on-the-fly decryption
                    self.ccm.mode.write(|w| w.mode().decryption());
                    self.ccm
                        .inptr
                        .write(|w| unsafe { w.bits(self.packet.as_ptr() as u32) });
                    self.ccm
                        .outptr
                        .write(|w| unsafe { w.bits(self.plaintext.as_ptr() as u32) });
                    self.ccm.shorts.write(|w| w.endksgen_crypt().disabled()); // disable shortcut ENDKSGEN -> CRYPT
                    self.ppi.chenset.write(|w| w.ch25().set()); // enable ppi radio ADDRESS -> ccm CRYPT (ch25)

                    // Enable radio RX
                    self.radio.tasks_rxen.write(|w| unsafe { w.bits(1) });
                    RadioState::RxIdle
                } else {
                    RadioState::TxDisable
                }
            }
        };
    }

    fn get_packet_id(&mut self) -> u8 {
        let id = self.next_packet_id;
        self.next_packet_id = self.next_packet_id.wrapping_add(1);
        id
    }

    fn handle_rx_ack(&mut self, ack: u8) {
        if let TxState::Sent {
            packet_data: PacketData { id, .. },
            ..
        } = self.tx_state
        {
            if id == ack {
                self.tx_state = TxState::Idle;
            }
        }
    }

    fn handle_rx_data(&mut self, packet_data: PacketData, rx_queue: &mut Queue) {
        match self.rx_state {
            RxState::Initial => {
                // This is the first received data packet
                for &byte in packet_data.iter() {
                    rx_queue.enqueue(byte);
                }
                self.rx_state = RxState::NeedsAck { id: packet_data.id };
            }
            RxState::Acked { id: last_acked_id } => {
                if packet_data.id != last_acked_id {
                    // Write data to rx queue only if it's a new packet
                    for &byte in packet_data.iter() {
                        rx_queue.enqueue(byte);
                    }
                } else {
                    debug!(
                        "radio - received an already acked packet {=u8}",
                        packet_data.id
                    );
                }
                self.rx_state = RxState::NeedsAck { id: packet_data.id };
            }
            RxState::NeedsAck { .. } => {
                // Got a new packet before sending an ack => must be a retransmit
                debug!(
                    "radio - received a packet before sending ack {=u8}",
                    packet_data.id
                );
            }
        }
    }

    fn assemble_packet(
        &mut self,
        now: u32,
        tx_queue: &mut Queue,
    ) -> (Option<Packet>, RxState, TxState) {
        match (self.rx_state, self.tx_state) {
            (
                RxState::NeedsAck { id: ack_id },
                TxState::Sent {
                    packet_data,
                    tx_count,
                    ..
                },
            ) => {
                // Should ack the last rx packet, and still waiting for tx ack =>
                // send ack and retransmit last data
                (
                    Some(Packet::Both(ack_id, packet_data)),
                    RxState::Acked { id: ack_id },
                    TxState::Sent {
                        tx_count: tx_count + 1,
                        since: now,
                        packet_data,
                    },
                )
            }
            (
                rx_state,
                tx_state @ TxState::Sent {
                    packet_data,
                    tx_count,
                    since,
                },
            ) => {
                // Waiting for ack to the last tx packet => retransmit if enough time has passed (with exponential backoff).
                // The (now % 3) term adds some randomness to the retransmit interval.
                // if now - since > 2 + 2u32.pow(tx_count) + (now % 3) {
                if now - since > 2 + ((now * 7) % 89) {
                    if tx_count <= 16 {
                        (
                            // Re-send ack too, because they might be waiting for it
                            if let RxState::Acked { id: ack_id } = rx_state {
                                Some(Packet::Both(ack_id, packet_data))
                            } else {
                                Some(Packet::Data(packet_data))
                            },
                            rx_state,
                            TxState::Sent {
                                tx_count: tx_count + 1,
                                since: now,
                                packet_data,
                            },
                        )
                    } else {
                        debug!(
                            "radio - no ack received for {=u8} after {=u32} transmits, giving up",
                            packet_data.id, tx_count
                        );
                        (None, rx_state, TxState::Idle)
                    }
                } else {
                    (None, rx_state, tx_state)
                }
            }
            (RxState::NeedsAck { id: ack_id }, TxState::Idle) => {
                if !tx_queue.is_empty() {
                    // Should ack the last rx packet and has data to transmit => send ack and data
                    let packet_data = PacketData::from_queue(self.get_packet_id(), tx_queue);
                    (
                        Some(Packet::Both(ack_id, packet_data)),
                        RxState::Acked { id: ack_id },
                        TxState::Sent {
                            tx_count: 1,
                            since: now,
                            packet_data,
                        },
                    )
                } else {
                    // Should ack the last rx packet and no data to transmit => send ack
                    (
                        Some(Packet::Ack(ack_id)),
                        RxState::Acked { id: ack_id },
                        TxState::Idle,
                    )
                }
            }
            (rx_state, TxState::Idle) => {
                if !tx_queue.is_empty() {
                    // Data to transmit => send data
                    let packet_data = PacketData::from_queue(self.get_packet_id(), tx_queue);
                    (
                        Some(Packet::Data(packet_data)),
                        rx_state,
                        TxState::Sent {
                            tx_count: 1,
                            since: now,
                            packet_data,
                        },
                    )
                } else {
                    (None, rx_state, TxState::Idle)
                }
            }
        }
    }
}
