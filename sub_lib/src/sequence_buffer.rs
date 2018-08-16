// Copyright (c) 2017-2018, Substratum LLC (https://substratum.net) and/or its affiliates. All rights reserved.

use proxy_server::ClientRequestPayload;
use std::cmp::Ordering;
use stream_handler_pool::TransmitDataMsg;
use std::collections::BinaryHeap;
use utils;

#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SequencedPacket {
    pub data: Vec<u8>,
    pub sequence_number: u64,
    pub last_data: bool
}

impl Ord for SequencedPacket {
    fn cmp(&self, other: &SequencedPacket) -> Ordering {
        // This can not be self.sequence_number.cmp(&other.sequence_number) since BinaryHeap is a
        // max-heap. We want to retrieve the SequencedPackets with the lowest sequence number first.
        // Therefore, we reverse this to make BinaryHeap behave as a min-heap.
        other.sequence_number.cmp(&self.sequence_number)
    }
}

impl PartialOrd for SequencedPacket {
    fn partial_cmp(&self, other: &SequencedPacket) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<'a> From<&'a ClientRequestPayload> for SequencedPacket {
    fn from(crp: &'a ClientRequestPayload) -> Self {
        SequencedPacket::new(
            crp.data.data.clone(),
            crp.sequence_number,
            crp.last_data
        )
    }
}

impl<'a> From<&'a TransmitDataMsg> for SequencedPacket {
    fn from(tdm: &'a TransmitDataMsg) -> Self {
        SequencedPacket::new(
            tdm.data.clone(),
            tdm.sequence_number.unwrap_or(0),
            tdm.last_data
        )
    }
}

impl SequencedPacket {
    pub fn new(data: Vec<u8>, sequence_number: u64, last_data: bool) -> SequencedPacket {
        SequencedPacket {
            data,
            sequence_number,
            last_data
        }
    }
}

#[derive(Clone)]
pub struct SequenceBuffer {
    // BinaryHeap is a Priority Queue implemented with a heap. The priority queue allows
    // SequencedPackets to come in in any order and be retrieved in a sorted order.
    buffer: BinaryHeap<SequencedPacket>,
    next_expected_sequence_number: u64,
    seen_sequence_numbers: Vec<u64>,
}

impl SequenceBuffer {
    pub fn new() -> SequenceBuffer {
        SequenceBuffer {
            buffer: BinaryHeap::new(),
            next_expected_sequence_number: 0,
            seen_sequence_numbers: vec!(),
        }
    }

    pub fn push(&mut self, packet: SequencedPacket) {
        if packet.sequence_number >= self.next_expected_sequence_number && !self.seen_sequence_numbers.contains(&packet.sequence_number) {
            self.seen_sequence_numbers.push(packet.sequence_number);
            self.buffer.push(packet);
        }
    }

    pub fn poll(&mut self) -> Option<SequencedPacket> {
        if self.buffer.is_empty() {
            None
        } else {
            let sequence_number_to_pop = self.buffer.peek().expect("internal error").sequence_number;
            if sequence_number_to_pop == self.next_expected_sequence_number {
                self.next_expected_sequence_number += 1;
                let packet = self.buffer.pop();

                if let Some(index) = utils::index_of(self.seen_sequence_numbers.as_slice(), &[sequence_number_to_pop]) {
                    self.seen_sequence_numbers.remove(index);
                }

                packet
            } else {
                None
            }
        }
    }

    pub fn repush(&mut self, packet: SequencedPacket) {
        if packet.sequence_number != self.next_expected_sequence_number - 1 {
            panic!("improper use of repush")
        } else {
            self.next_expected_sequence_number = packet.sequence_number;
            self.seen_sequence_numbers.push(packet.sequence_number);
            self.buffer.push(packet);
        }
    }

    pub fn next_expected(&self) -> u64 {
        self.next_expected_sequence_number
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cryptde::PlainData;
    use proxy_server::ProxyProtocol;
    use cryptde::Key;
    use std::net::SocketAddr;
    use std::str::FromStr;
    use stream_handler_pool::TransmitDataMsg;
    use dispatcher::Endpoint;

    #[test]
    fn can_create_sequenced_packet_from_client_request_payload() {
        let crp = ClientRequestPayload {
            stream_key: SocketAddr::from_str("1.2.3.4:80").unwrap(),
            last_data: true,
            sequence_number: 2,
            data: PlainData::new(&[1, 2, 3, 5]),
            target_hostname: None,
            target_port: 0,
            protocol: ProxyProtocol::HTTP,
            originator_public_key: Key::new(&[5, 6, 9, 1]),
        };

        let result = SequencedPacket::from(&crp);

        assert_eq!(result.data, vec![1, 2, 3, 5]);
        assert_eq!(result.sequence_number, 2);
        assert!(result.last_data, true);
    }

    #[test] // TODO: This is probably not the right thing to do; see SC-390
    fn uses_zero_when_creating_sequenced_packet_from_transmit_data_msg_with_no_sequence_number() {
        let tdm = TransmitDataMsg {
            endpoint: Endpoint::Socket(SocketAddr::from_str("1.2.3.4:80").unwrap()),
            last_data: true,
            data: vec![1, 4, 5, 9],
            sequence_number: None,
        };

        let result = SequencedPacket::from(&tdm);

        assert_eq! (result.sequence_number, 0)
    }

    #[test]
    fn can_create_sequenced_packet_from_transmit_data_msg() {
        let tdm = TransmitDataMsg {
            endpoint: Endpoint::Socket(SocketAddr::from_str("1.2.3.4:80").unwrap()),
            last_data: true,
            data: vec![1, 4, 5, 9],
            sequence_number: Some(1),
        };

        let result = SequencedPacket::from(&tdm);

        assert_eq!(result.data, vec![1, 4, 5, 9]);
        assert_eq!(result.sequence_number, 1);
        assert!(result.last_data);
    }

    #[test]
    fn sequence_buffer_reorders_out_of_order_sequenced_packets() {
        let a = SequencedPacket::new(vec!(1, 23, 6, 5), 0, false);
        let b = SequencedPacket::new(vec!(5, 9, 1, 2, 5), 1, false);
        let c = SequencedPacket::new(vec!(1, 1, 1, 1, 0), 2, false);
        let d = SequencedPacket::new(vec!(32, 41, 0, 5, 1, 2, 6), 3, false);
        let e = SequencedPacket::new(vec!(), 4, true);

        let mut subject = SequenceBuffer::new();

        subject.push(b.clone());
        subject.push(d.clone());
        subject.push(a.clone());
        subject.push(e.clone());
        subject.push(c.clone());

        assert_eq!(subject.poll(), Some(a));
        assert_eq!(subject.poll(), Some(b));
        assert_eq!(subject.poll(), Some(c));
        assert_eq!(subject.poll(), Some(d));
        assert_eq!(subject.poll(), Some(e));
        assert_eq!(subject.poll(), None);
    }

    #[test]
    fn sequence_buffer_returns_none_while_waiting_for_next_ordered_sequenced_packet() {
        let a = SequencedPacket::new(vec!(1, 23, 6, 5), 0, false);
        let b = SequencedPacket::new(vec!(5, 9, 1, 2, 5), 1, false);
        let c = SequencedPacket::new(vec!(1, 1, 1, 1, 0), 2, false);
        let d = SequencedPacket::new(vec!(32, 41, 0, 5, 1, 2, 6), 3, false);
        let e = SequencedPacket::new(vec!(), 4, true);

        let mut subject = SequenceBuffer::new();

        subject.push(b.clone());
        assert_eq!(subject.poll(), None);
        subject.push(d.clone());
        assert_eq!(subject.poll(), None);
        subject.push(a.clone());
        assert_eq!(subject.poll(), Some(a));
        assert_eq!(subject.poll(), Some(b));
        assert_eq!(subject.poll(), None);
        subject.push(e.clone());
        assert_eq!(subject.poll(), None);
        subject.push(c.clone());
        assert_eq!(subject.poll(), Some(c));
        assert_eq!(subject.poll(), Some(d));
        assert_eq!(subject.poll(), Some(e));
        assert_eq!(subject.poll(), None);
    }

    #[test]
    fn sequence_buffer_ignores_packets_with_duplicate_sequence_numbers() {
        let a = SequencedPacket::new(vec!(1, 23, 6, 5), 0, false);
        let b = SequencedPacket::new(vec!(5, 9, 1, 2, 5), 1, false);
        let b_dup = SequencedPacket::new(vec!(6, 8, 2, 3, 6), 1, false);
        let c = SequencedPacket::new(vec!(1, 1, 1, 1, 0), 2, false);
        let d = SequencedPacket::new(vec!(32, 41, 0, 5, 1, 2, 6), 3, false);
        let e = SequencedPacket::new(vec!(), 4, true);

        let mut subject = SequenceBuffer::new();

        subject.push(b.clone());
        assert_eq!(subject.poll(), None);
        subject.push(d.clone());
        assert_eq!(subject.poll(), None);
        subject.push(b_dup.clone());
        assert_eq!(subject.poll(), None);
        subject.push(a.clone());
        assert_eq!(subject.poll(), Some(a));
        assert_eq!(subject.poll(), Some(b));
        assert_eq!(subject.poll(), None);
        subject.push(e.clone());
        assert_eq!(subject.poll(), None);
        subject.push(c.clone());
        assert_eq!(subject.poll(), Some(c));
        assert_eq!(subject.poll(), Some(d));
        assert_eq!(subject.poll(), Some(e));
        assert_eq!(subject.poll(), None);
    }

    #[test]
    fn sequence_buffer_ignores_delayed_duplicate_sequence_number() {
        let a = SequencedPacket::new(vec!(1, 23, 6, 5), 0, false);
        let b = SequencedPacket::new(vec!(5, 9, 1, 2, 5), 1, false);
        let b_dup = SequencedPacket::new(vec!(6, 8, 2, 3, 6), 1, false);
        let c = SequencedPacket::new(vec!(1, 1, 1, 1, 0), 2, false);
        let d = SequencedPacket::new(vec!(32, 41, 0, 5, 1, 2, 6), 3, false);
        let e = SequencedPacket::new(vec!(), 4, true);

        let mut subject = SequenceBuffer::new();

        subject.push(b.clone());
        assert_eq!(subject.poll(), None);
        subject.push(d.clone());
        assert_eq!(subject.poll(), None);
        subject.push(a.clone());
        assert_eq!(subject.poll(), Some(a));
        assert_eq!(subject.poll(), Some(b));
        assert_eq!(subject.poll(), None);
        subject.push(e.clone());
        assert_eq!(subject.poll(), None);
        subject.push(c.clone());
        assert_eq!(subject.poll(), Some(c));
        assert_eq!(subject.poll(), Some(d));
        subject.push(b_dup.clone());
        assert_eq!(subject.poll(), Some(e));
        assert_eq!(subject.poll(), None);
    }

    #[test]
    fn sequence_buffer_does_not_explode_when_popping_a_packet_that_seems_unseen() {
        let a = SequencedPacket::new(vec!(1, 2, 3), 0, false);
        let mut subject = SequenceBuffer::new();
        subject.push(a.clone());
        subject.seen_sequence_numbers.clear();

        let result = subject.poll();

        assert_eq!(result, Some(a));
    }

    #[test]
    fn sequence_buffer_can_re_add_a_popped_packet() {
        let mut subject = SequenceBuffer::new();
        let a = SequencedPacket::new(vec!(1, 23, 6, 5), 1, false);
        let b = SequencedPacket::new(vec!(5, 9, 1, 2, 5), 2, false);
        let c = SequencedPacket::new(vec!(5, 9, 1, 2, 5), 0, false);

        subject.push(a);
        subject.push(b);
        subject.push(c.clone());

        let thing_we_pushed_back = subject.poll().unwrap();
        assert_eq!(thing_we_pushed_back, c);
        subject.repush(thing_we_pushed_back.clone());
        assert_eq!(subject.poll().unwrap(), thing_we_pushed_back);
    }

    #[test]
    #[should_panic(expected = "improper use of repush")]
    fn repush_panics_if_repushee_sequence_number_is_too_low() {
        let mut subject = SequenceBuffer::new();
        let a = SequencedPacket::new(vec!(1, 23, 6, 5), 1, false);
        let b = SequencedPacket::new(vec!(5, 9, 1, 2, 5), 2, false);
        let c = SequencedPacket::new(vec!(5, 9, 1, 2, 5), 0, false);

        subject.push(a);
        subject.push(b);
        subject.push(c);

        let first_thing_we_pulled_out = subject.poll().unwrap();
        let _second_thing_we_pulled_out = subject.poll().unwrap();
        subject.repush(first_thing_we_pulled_out);
    }

    #[test]
    fn repush_does_not_interfere_with_ignoring_duplicate_sequence_numbers() {
        let mut subject = SequenceBuffer::new();

        let a = SequencedPacket::new(vec!(4, 5, 6), 0, false);
        let b = SequencedPacket::new(vec!(89), 1, false);
        let b_imposter = SequencedPacket::new(vec!(254, 5, 7), 1, false);
        let c = SequencedPacket::new(vec!(89), 2, false);

        subject.push(a.clone());
        subject.push(b.clone());

        assert_eq!(subject.poll(), Some(a));
        assert_eq!(subject.poll(), Some(b.clone()));

        subject.repush(b.clone());

        subject.push(b_imposter);

        assert_eq!(subject.poll(), Some(b));
        assert_eq!(subject.poll(), None);

        subject.push(c.clone());
        assert_eq!(subject.poll(), Some(c));
    }
}