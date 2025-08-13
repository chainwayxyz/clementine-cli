use std::env;
pub type BitcoinAddress<V = bitcoin::address::NetworkChecked> = bitcoin::Address<V>;
pub use bitcoin::address::{NetworkChecked, NetworkUnchecked};

pub type CitreaAddress = alloy::primitives::Address;

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
pub mod errors;
pub mod musig2;
pub mod parameters;
pub mod script;
pub mod storage;
pub mod types;
pub mod withdrawal;

// todo dont make it public
pub mod utils;
