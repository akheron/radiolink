use crate::pend::Pend;
use crate::radio::{Packet, PacketData};
use crate::{radio, uart};
use defmt::{error, trace, warn};

struct WaitingForAck {
    packet_data: PacketData,
    tx_count: u32,
    since: u32,
}

pub struct RadioProtocol {
    last_acked: Option<u8>,
    waiting_for_ack: Option<WaitingForAck>,
    next_packet_id: u8,
    radio_rx: radio::ConsumerType,
    radio_tx: radio::ProducerType,
    uart_rx: uart::ConsumerType,
    uart_tx: uart::ProducerType,
}

impl RadioProtocol {
    pub fn new(
        radio_rx: radio::ConsumerType,
        radio_tx: radio::ProducerType,
        uart_rx: uart::ConsumerType,
        uart_tx: uart::ProducerType,
    ) -> Self {
        Self {
            last_acked: None,
            waiting_for_ack: None,
            next_packet_id: 0,
            radio_rx,
            radio_tx,
            uart_rx,
            uart_tx,
        }
    }

    pub fn run(&mut self, now: u32) -> Pend {
        let rx = match self.radio_rx.dequeue() {
            Some(Packet::Data(rx_packet_data)) => self.handle_rx_data(now, rx_packet_data),
            Some(Packet::Ack(id)) => self.handle_rx_ack(id),
            Some(Packet::Both(ack_id, rx_packet_data)) => {
                // ack MUST be handled first
                self.handle_rx_ack(ack_id) + self.handle_rx_data(now, rx_packet_data)
            }
            None => Pend::Nothing,
        };
        let tx = if self.waiting_for_ack.is_some() {
            self.handle_waiting_for_ack(now)
        } else if self.uart_rx.ready() {
            self.handle_tx_data(now)
        } else {
            Pend::Nothing
        };
        rx + tx
    }

    fn handle_rx_data(&mut self, now: u32, rx_packet_data: PacketData) -> Pend {
        let rx = if self.last_acked != Some(rx_packet_data.id) {
            for i in rx_packet_data.iter() {
                if self.uart_tx.enqueue(*i).is_err() {
                    warn!(
                        "radio_protocol: uart tx queue full ({=usize}) (1)",
                        self.uart_tx.len()
                    );
                }
            }
            Pend::Uart
        } else {
            warn!(
                "radio_protocol: received duplicate packet {}",
                rx_packet_data.id
            );
            Pend::Nothing
        };

        let tx_packet = if let Some(state) = self.waiting_for_ack.as_mut() {
            // Received data while waiting for ack => resend
            warn!("radio_protocol: received data while waiting for ack, resend {=u8} (tx_count {=u32})", state.packet_data.id, state.tx_count + 1);
            state.since = now;
            state.tx_count += 1;
            Packet::Both(rx_packet_data.id, state.packet_data)
        } else if self.uart_rx.ready() {
            let tx_packet_id = self.get_packet_id();
            let tx_packet_data = PacketData::from_consumer(tx_packet_id, &mut self.uart_rx);
            self.waiting_for_ack = Some(WaitingForAck {
                packet_data: tx_packet_data,
                since: now,
                tx_count: 1,
            });
            Packet::Both(rx_packet_data.id, tx_packet_data)
        } else {
            Packet::Ack(rx_packet_data.id)
        };

        trace!("radio_protocol: enqueuing tx packet: {}", tx_packet);
        let tx = if self.radio_tx.enqueue(tx_packet).is_err() {
            warn!("radio_protocol: radio tx queue full");
            // Pend the radio to process the queue
            Pend::Radio
        } else {
            self.last_acked = Some(rx_packet_data.id);
            Pend::Radio
        };

        rx + tx
    }

    fn handle_rx_ack(&mut self, rx_id: u8) -> Pend {
        match &self.waiting_for_ack {
            Some(WaitingForAck { packet_data, .. }) => {
                if packet_data.id == rx_id {
                    self.waiting_for_ack = None;
                } else {
                    trace!(
                        "radio_protocol: expected ack {} but received ack {}",
                        packet_data.id,
                        rx_id
                    );
                }
            }
            _ => {
                warn!("radio_protocol: received unexpected ack {}", rx_id);
            }
        }
        Pend::Nothing
    }

    fn handle_tx_data(&mut self, now: u32) -> Pend {
        // We don't send new data if we're waiting for an ack
        if self.waiting_for_ack.is_some() {
            error!("radio_protocol: handle_tx_data called while waiting for ack");
            return Pend::Nothing;
        }

        let id = self.get_packet_id();
        let tx_packet_data = PacketData::from_consumer(id, &mut self.uart_rx);

        let tx_packet = Packet::Data(tx_packet_data);

        trace!("radio_protocol: enqueuing tx packet: {}", tx_packet);
        if self.radio_tx.enqueue(tx_packet).is_err() {
            warn!("radio_protocol: radio tx queue full");
            // Pend the radio to process the queue
            Pend::Radio
        } else {
            self.waiting_for_ack = Some(WaitingForAck {
                packet_data: tx_packet_data,
                since: now,
                tx_count: 1,
            });
            Pend::Radio
        }
    }

    fn handle_waiting_for_ack(&mut self, now: u32) -> Pend {
        let state = self.waiting_for_ack.as_mut().unwrap();
        if now - state.since > 2 + ((now.wrapping_mul(7)) % 89) {
            if state.tx_count <= 16 {
                // Re-send ack too, because they might be waiting for it
                let packet = if let Some(ack_id) = self.last_acked {
                    warn!(
                        "radio_protocol: resend packet {=u8} (tx_count {=u32}) (also ack {=u8})",
                        state.packet_data.id,
                        state.tx_count + 1,
                        ack_id
                    );
                    Packet::Both(ack_id, state.packet_data)
                } else {
                    warn!(
                        "radio_protocol: resend packet {=u8} (tx_count {=u32})",
                        state.packet_data.id,
                        state.tx_count + 1
                    );
                    Packet::Data(state.packet_data)
                };
                state.tx_count += 1;
                state.since = now;
                if self.radio_tx.enqueue(packet).is_err() {
                    Pend::Nothing
                } else {
                    Pend::Radio
                }
            } else {
                warn!(
                    "radio_protocol: no ack received for {=u8} after {=u32} transmits, giving up",
                    state.packet_data.id, state.tx_count
                );
                self.waiting_for_ack = None;
                Pend::Nothing
            }
        } else {
            Pend::Nothing
        }
    }

    fn get_packet_id(&mut self) -> u8 {
        let id = self.next_packet_id;
        self.next_packet_id = self.next_packet_id.wrapping_add(1);
        id
    }
}
