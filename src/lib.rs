use std::env;

/// Check if debug mode is enabled via CLEMENTINE_DEBUG environment variable
pub fn is_debug_enabled() -> bool {
    env::var("CLEMENTINE_DEBUG").is_ok()
}

/// Debug macro that only prints when CLEMENTINE_DEBUG is set
#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {
        {
            use colored::*;
            if $crate::is_debug_enabled() {
                println!("{} {}", "DEBUG".magenta().bold(), format!($($arg)*));
            }
        }
    };
}

/// Debug macro for colored output that only prints when CLEMENTINE_DEBUG is set
#[macro_export]
macro_rules! debug_colored {
    ($color:expr, $($arg:tt)*) => {
        {
            use colored::*;
            if $crate::is_debug_enabled() {
                println!("{} {}", "DEBUG".magenta().bold(), format!($($arg)*).color($color));
            }
        }
    };
}

pub mod backend;
pub mod bitcoin_merkle;
pub mod bitcoin_utils;
pub mod config;
pub mod deposit;
pub mod musig2;
pub mod parameters;
pub mod script;
pub mod storage;
pub mod withdrawal;

/// EVM Address type - 20 bytes
#[derive(Copy, Clone, Debug, PartialOrd, Ord, PartialEq, Eq, Hash)]
pub struct EVMAddress(pub [u8; 20]);

impl TryFrom<Vec<u8>> for EVMAddress {
    type Error = &'static str;

    fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
        if value.len() == 20 {
            Ok(EVMAddress(value.try_into().unwrap()))
        } else {
            Err("Expected a Vec<u8> of length 20")
        }
    }
}

impl TryFrom<&str> for EVMAddress {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let clean_address = value.strip_prefix("0x").unwrap_or(value);
        let bytes = hex::decode(clean_address).map_err(|_| "Invalid hex format for EVM address")?;
        Self::try_from(bytes)
    }
}

impl std::fmt::Display for EVMAddress {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "0x{}", hex::encode(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_evm_address_from_vec() {
        let bytes = vec![0u8; 20];
        let addr = EVMAddress::try_from(bytes).unwrap();
        assert_eq!(addr.0, [0u8; 20]);

        let too_short = vec![0u8; 19];
        assert!(EVMAddress::try_from(too_short).is_err());

        let too_long = vec![0u8; 21];
        assert!(EVMAddress::try_from(too_long).is_err());
    }

    #[test]
    fn test_evm_address_from_str() {
        let addr_str = "0x0000000000000000000000000000000000000000";
        let addr = EVMAddress::try_from(addr_str).unwrap();
        assert_eq!(addr.0, [0u8; 20]);

        let without_prefix = "0000000000000000000000000000000000000000";
        let addr = EVMAddress::try_from(without_prefix).unwrap();
        assert_eq!(addr.0, [0u8; 20]);

        let invalid_hex = "0xgggggggggggggggggggggggggggggggggggggggg";
        assert!(EVMAddress::try_from(invalid_hex).is_err());

        let wrong_length = "0x00000000000000000000000000000000000000";
        assert!(EVMAddress::try_from(wrong_length).is_err());
    }

    #[test]
    fn test_evm_address_display() {
        let bytes = vec![0u8; 20];
        let addr = EVMAddress::try_from(bytes).unwrap();
        assert_eq!(
            addr.to_string(),
            "0x0000000000000000000000000000000000000000"
        );
    }
}
