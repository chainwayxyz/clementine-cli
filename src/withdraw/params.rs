use crate::core::types::{MerkleProof, Transaction};
use crate::wallet::BitcoinAddress;
use crate::wallet::TaprootAddressWithPrefix;
use bitcoin::taproot::Signature;
use bitcoin::{Amount, OutPoint};

#[derive(Debug)]
pub struct WithdrawalUrl(pub String);

#[derive(Debug)]
pub struct TxJson(pub String);

/// Parameters for safe withdrawal operations
#[derive(Debug)]
pub struct SafeWithdrawalParams {
    pub signer_address: TaprootAddressWithPrefix<bitcoin::address::NetworkChecked>,
    pub destination_address: BitcoinAddress,
    pub withdrawal_outpoint: OutPoint,
    pub withdrawal_amount: Amount,
    pub signature: Signature,
}

#[derive(Debug, Clone)]
pub struct WithdrawalParams {
    pub transaction: Transaction,
    pub merkle_proof: MerkleProof,
    pub payout_transaction: Transaction,
    pub block_header: alloy::sol_types::private::Bytes,
    pub output_script_pk: alloy::sol_types::private::Bytes,
}

impl std::fmt::Display for WithdrawalParams {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "\nSafe Withdraw Params")?;
        writeln!(f, "  Transaction:          {:?}", self.transaction)?;
        writeln!(f, "  Merkle Proof:         {:?}", self.merkle_proof)?;
        writeln!(f, "  Payout Transaction:   {:?}", self.payout_transaction)?;
        writeln!(f, "  Block Header:         {:?}", self.block_header)?;
        writeln!(f, "  Output Script PK:     {:?}", self.output_script_pk)?;
        Ok(())
    }
}
