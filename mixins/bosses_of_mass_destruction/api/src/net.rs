//! The one channel between the server half and the client half
//! (`horizon:mod_net`, docs/native-mods.md, "Mod messages").
//!
//! The server tells clients which attack a boss started, so the model animates
//! the right move; clients send nothing back. A message is
//! `[anim][entity][phase][duration]`, all little endian.
pub const CHANNEL: &str = "bosses_of_mass_destruction:anim";

/// Which part of an attack a message describes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Telegraph,
    Landed,
    Ended,
}

impl Phase {
    fn byte(self) -> u8 {
        match self {
            Phase::Telegraph => 0,
            Phase::Landed => 1,
            Phase::Ended => 2,
        }
    }

    fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Phase::Telegraph),
            1 => Some(Phase::Landed),
            2 => Some(Phase::Ended),
            _ => None,
        }
    }
}

/// An animation message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Message {
    pub entity: u32,
    pub anim: u8,
    pub phase: Phase,
    /// How many ticks the phase lasts, so a client can time the move without
    /// another message.
    pub duration: u16,
}

impl Message {
    /// `anim` (1) + `entity` (4) + `phase` (1) + `duration` (2).
    pub const LEN: usize = 8;

    pub fn encode(self) -> [u8; Self::LEN] {
        let mut bytes = [0u8; Self::LEN];
        bytes[0] = self.anim;
        bytes[1..5].copy_from_slice(&self.entity.to_le_bytes());
        bytes[5] = self.phase.byte();
        bytes[6..8].copy_from_slice(&self.duration.to_le_bytes());
        bytes
    }

    pub fn decode(data: &[u8]) -> Option<Self> {
        if data.len() != Self::LEN {
            return None;
        }
        Some(Self {
            anim: data[0],
            entity: u32::from_le_bytes([data[1], data[2], data[3], data[4]]),
            phase: Phase::from_byte(data[5])?,
            duration: u16::from_le_bytes([data[6], data[7]]),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_round_trips() {
        let message = Message {
            entity: 0xDEAD_BEEF,
            anim: 13,
            phase: Phase::Landed,
            duration: 40,
        };
        let bytes = message.encode();
        assert_eq!(bytes.len(), Message::LEN);
        assert_eq!(Message::decode(&bytes), Some(message));
    }

    #[test]
    fn a_short_or_unknown_message_is_refused() {
        assert_eq!(Message::decode(&[1, 2, 3]), None);
        let mut bytes = [0u8; Message::LEN];
        bytes[5] = 9;
        assert_eq!(Message::decode(&bytes), None);
    }
}
