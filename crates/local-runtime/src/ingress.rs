use crate::{CHANNEL_CAPACITY, MAX_INPUT_BYTES, MAX_MESSAGE_BYTES};
use std::sync::{
    atomic::{AtomicU64, Ordering},
    mpsc::{self, Receiver, SyncSender, TrySendError},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IngressMessage(pub Vec<u8>);
impl IngressMessage {
    pub fn new(bytes: Vec<u8>) -> Result<Self, IngressError> {
        if bytes.len() > MAX_MESSAGE_BYTES {
            Err(IngressError::TooLarge)
        } else {
            Ok(Self(bytes))
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IngressOutcome {
    Accepted,
    Full,
    Unavailable,
    Oversized,
}
#[derive(Debug, PartialEq, Eq)]
pub enum IngressError {
    TooLarge,
}
#[derive(Debug, Default)]
pub struct IngressCounters {
    pub accepted: AtomicU64,
    pub full: AtomicU64,
    pub unavailable: AtomicU64,
    pub oversized: AtomicU64,
}
impl IngressCounters {
    pub fn snapshot(&self) -> (u64, u64, u64, u64) {
        (
            self.accepted.load(Ordering::Relaxed),
            self.full.load(Ordering::Relaxed),
            self.unavailable.load(Ordering::Relaxed),
            self.oversized.load(Ordering::Relaxed),
        )
    }
}
#[derive(Debug)]
pub struct Ingress {
    sender: Option<SyncSender<IngressMessage>>,
    pub counters: IngressCounters,
}
impl Ingress {
    pub fn new() -> (Self, Receiver<IngressMessage>) {
        let (sender, receiver) = mpsc::sync_channel(CHANNEL_CAPACITY);
        (
            Self {
                sender: Some(sender),
                counters: IngressCounters::default(),
            },
            receiver,
        )
    }
    pub fn unavailable() -> Self {
        Self {
            sender: None,
            counters: IngressCounters::default(),
        }
    }
    pub fn try_send(&self, input: &[u8]) -> IngressOutcome {
        self.try_send_projected(input.len(), input)
    }
    pub fn try_send_projected(&self, raw_input_bytes: usize, projected: &[u8]) -> IngressOutcome {
        if raw_input_bytes > MAX_INPUT_BYTES || projected.len() > MAX_MESSAGE_BYTES {
            self.counters.oversized.fetch_add(1, Ordering::Relaxed);
            return IngressOutcome::Oversized;
        }
        let Some(sender) = &self.sender else {
            self.counters.unavailable.fetch_add(1, Ordering::Relaxed);
            return IngressOutcome::Unavailable;
        };
        match sender.try_send(IngressMessage(projected.to_vec())) {
            Ok(()) => {
                self.counters.accepted.fetch_add(1, Ordering::Relaxed);
                IngressOutcome::Accepted
            }
            Err(TrySendError::Full(_)) => {
                self.counters.full.fetch_add(1, Ordering::Relaxed);
                IngressOutcome::Full
            }
            Err(TrySendError::Disconnected(_)) => {
                self.counters.unavailable.fetch_add(1, Ordering::Relaxed);
                IngressOutcome::Unavailable
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_nonblocking_outcomes_and_counters() {
        let (i, r) = Ingress::new();
        assert_eq!(i.try_send(b"x"), IngressOutcome::Accepted);
        assert_eq!(r.try_recv().unwrap().0, b"x");
        assert_eq!(
            i.try_send(&vec![0; MAX_MESSAGE_BYTES + 1]),
            IngressOutcome::Oversized
        );
        let i = Ingress::unavailable();
        assert_eq!(i.try_send(b"x"), IngressOutcome::Unavailable);
        assert_eq!(i.counters.snapshot(), (0, 0, 1, 0));
    }

    #[test]
    fn full_channel_fails_open_without_waiting() {
        let (ingress, _receiver) = Ingress::new();
        for _ in 0..CHANNEL_CAPACITY {
            assert_eq!(ingress.try_send(b"x"), IngressOutcome::Accepted);
        }
        assert_eq!(ingress.try_send(b"x"), IngressOutcome::Full);
        assert_eq!(
            ingress.counters.snapshot(),
            (CHANNEL_CAPACITY as u64, 1, 0, 0)
        );
    }

    #[test]
    fn raw_and_projected_bounds_are_independent() {
        let (ingress, _receiver) = Ingress::new();
        assert_eq!(
            ingress.try_send_projected(MAX_INPUT_BYTES + 1, b"bounded"),
            IngressOutcome::Oversized
        );
        assert_eq!(
            ingress.try_send_projected(MAX_INPUT_BYTES, &vec![0; MAX_MESSAGE_BYTES + 1]),
            IngressOutcome::Oversized
        );
    }

    #[test]
    fn disconnected_receiver_fails_open_as_unavailable() {
        let (ingress, receiver) = Ingress::new();
        drop(receiver);
        assert_eq!(ingress.try_send(b"x"), IngressOutcome::Unavailable);
        assert_eq!(ingress.counters.snapshot(), (0, 0, 1, 0));
    }
}
