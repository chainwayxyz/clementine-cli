pub use bitcoin::address::{NetworkChecked, NetworkUnchecked};

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

pub type BitcoinAddress<V = bitcoin::address::NetworkChecked> = bitcoin::Address<V>;
pub type CitreaAddress = alloy::primitives::Address;
