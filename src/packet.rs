use std::fmt::Debug;

use crate::error::{Error, Result};

/// Represents a packet received from the ELM327
#[derive(Debug)]
pub struct ObdPacket {
    packet: u64,
}

impl ObdPacket {
    /// Builds an empty OBD packet, consisting of all zeros.
    pub fn empty() -> Self {
        ObdPacket { packet: 0 }
    }

    /// Builds an OBD packet from a string.
    ///
    /// # Parameters
    ///
    /// * `s` - String to build packet from.
    pub fn new(s: impl AsRef<str>) -> Result<Self> {
        let x = s.as_ref().split('<').collect::<Vec<_>>()[0];
        let mut parts = x.split(' ').collect::<Vec<_>>();

        while parts.len() > 8 {
            parts.remove(0);
        }

        while parts.len() < 8 {
            parts.push("00");
        }

        Ok(ObdPacket {
            packet: u64::from_str_radix(parts.join("").as_str(), 16)
                .map_err(|_| Error::Conversion)?,
        })
    }

    /// Takes a 'slice' of the packet, by retrieving the bits between two given
    /// values, the `lower` bound and `upper` bound.
    ///
    /// # Parameters
    ///
    /// * `lower` - The lower bound.
    /// * `upper` - The upper bound.
    pub fn get(&self, lower: u8, upper: u8) -> Result<u64> {
        if upper > 63 {
            return Err(Error::Packet("Upper bound must be less than 64."));
        } else if lower > 63 || lower > upper {
            return Err(Error::Packet(
                "Lower bound must be less than the lesser of 63 or `upper`.",
            ));
        }

        let mut mask = 0u64;

        for i in lower..=upper {
            mask |= 1 << (i - lower);
        }

        Ok(mask & (self.packet >> lower))
    }
}
