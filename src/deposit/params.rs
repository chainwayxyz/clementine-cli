use crate::deposit::CitreaAddress;
use crate::wallet::BitcoinAddress;
use crate::wallet::TaprootAddressWithPrefix;
use bitcoin::{OutPoint, Transaction};

/// Parameters for creating a signed recovery transaction
pub struct RecoveryTxParams {
    pub citrea_addr: CitreaAddress,
    pub recovery_taproot_address: TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    pub outpoint: OutPoint,
    pub destination_addr: BitcoinAddress,
    pub fee_rate: Option<u64>,
    pub amount: Option<f64>,
}

/// Parameters for verifying a recovery transaction
#[derive(Debug)]
pub struct VerifyRecoveryTxParams {
    pub recovery_tx: Transaction,
    pub citrea_address: CitreaAddress,
    pub recovery_taproot_address: TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    pub amount: Option<f64>,
}
