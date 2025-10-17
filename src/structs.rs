use std::{borrow::Cow, fmt::Display};

use bitcoin::{
    Address,
    address::{NetworkChecked, NetworkUnchecked, NetworkValidation},
};

use serde::{Deserialize, Serialize};

const PROGRESS_BAR_WIDTH: usize = 40;

use crate::{
    BitcoinAddress,
    deposit::DepositStatusEnum,
    errors::BridgeCliError,
    wallet::{Purpose, address::parse_taproot_address},
    withdraw::WithdrawStatusEnum,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TaprootAddressWithPrefix<T: NetworkValidation> {
    pub address: Address<T>,
    pub purpose: Purpose,
}

impl TaprootAddressWithPrefix<NetworkChecked> {
    pub fn new(address: Address<NetworkChecked>, purpose: Purpose) -> Result<Self, BridgeCliError> {
        let address_type = if let Some(t) = address.address_type() {
            t
        } else {
            return Err(BridgeCliError::InvalidAddressFormat);
        };

        if address_type != bitcoin::AddressType::P2tr {
            return Err(BridgeCliError::InvalidAddressFormat);
        }

        Ok(Self { address, purpose })
    }

    pub fn from_string_with_prefix(
        address: &str,
        network: bitcoin::Network,
    ) -> Result<Self, BridgeCliError> {
        if address.len() < 3 {
            return Err(BridgeCliError::InvalidAddressFormat);
        }

        let purpose = Purpose::purpose_from_str(&address[0..3]).map_err(|e| {
            BridgeCliError::Eyre(eyre::eyre!(
                "Failed to parse purpose from address: {} Error: {}",
                address,
                e
            ))
        })?;

        let addr_str = &address[3..];

        let bitcoin_address = parse_taproot_address(addr_str, network)?;

        let taproot_address_with_prefix = Self::new(bitcoin_address, purpose)?;

        Ok(taproot_address_with_prefix)
    }

    pub fn from_string_without_prefix(
        address: &str,
        purpose: Purpose,
        network: bitcoin::Network,
    ) -> Result<Self, BridgeCliError> {
        let bitcoin_address = parse_taproot_address(address, network)?;
        let taproot_address_with_prefix = Self::new(bitcoin_address, purpose)?;
        Ok(taproot_address_with_prefix)
    }
}

impl TaprootAddressWithPrefix<NetworkUnchecked> {
    pub fn from_string_with_prefix_unchecked(address: &str) -> Result<Self, BridgeCliError> {
        if address.len() < 4 {
            return Err(BridgeCliError::InvalidAddressFormat);
        }

        let purpose = Purpose::purpose_from_str(&address[0..3])?;
        let addr_str = &address[3..];

        let unchecked_address: BitcoinAddress<NetworkUnchecked> =
            addr_str.parse().map_err(|e| {
                BridgeCliError::Eyre(eyre::eyre!("Failed to parse Bitcoin address: {}", e))
            })?;

        let taproot_address_with_prefix = Self {
            address: unchecked_address,
            purpose,
        };

        Ok(taproot_address_with_prefix)
    }
}

pub trait AddrDisplay {
    fn as_display_str(&self) -> String;
}

impl AddrDisplay for Address<NetworkChecked> {
    fn as_display_str(&self) -> String {
        self.to_string()
    }
}
impl AddrDisplay for Address<NetworkUnchecked> {
    fn as_display_str(&self) -> String {
        self.clone().assume_checked().to_string()
    }
}

impl<T> TaprootAddressWithPrefix<T>
where
    T: NetworkValidation,
    Address<T>: AddrDisplay,
{
    pub fn address_without_prefix(&self) -> String {
        self.address.as_display_str()
    }

    pub fn address_with_prefix(&self) -> String {
        format!(
            "{}{}",
            self.purpose.to_prefix(),
            self.address_without_prefix()
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct DepositStatus {
    pub id: u64,
    pub status: String,
    pub txid: String,
    pub evm_addr: String,
    pub move_tx_raw: String,
    pub move_txid: String,
    pub created_at: String,
    pub mint_txid: String,
}

pub struct DepositStatusWithVout<'a> {
    pub deposit_status: &'a DepositStatus,
    pub vout: Option<u32>,
}

#[allow(clippy::too_many_arguments)]
fn format_deposit_status(
    f: &mut std::fmt::Formatter<'_>,
    id: u64,
    status: &str,
    raw_status: &str,
    txid: &str,
    vout: Option<u32>,
    evm_addr: &str,
    move_txid: &str,
    move_tx_raw: &str,
    mint_txid: &str,
    print_na_for_vout: bool,
) -> std::fmt::Result {
    let display_or = |v: &str| {
        if v.is_empty() {
            "--".to_string()
        } else {
            v.to_string()
        }
    };

    writeln!(f, "\nDeposit Info")?;
    writeln!(f, "  ID:                 {}", id)?;
    writeln!(f, "  Status:             {}", status)?;

    // Add progress bar using the raw backend status
    let status_enum = DepositStatusEnum::from_status(raw_status);
    let (current, total) = status_enum.progress();

    if current > 0 {
        // Create a visual progress bar using ASCII characters
        let bar_width = PROGRESS_BAR_WIDTH;
        let filled = (current * bar_width) / total;
        let empty = bar_width - filled;

        let bar = if current == total {
            format!("[{}] {}/{}", "=".repeat(bar_width), current, total)
        } else {
            format!(
                "[{}{}] {}/{}",
                "=".repeat(filled.saturating_sub(1)) + if filled > 0 { ">" } else { "" },
                "-".repeat(empty),
                current,
                total
            )
        };

        writeln!(f, "  Progress:           {}", bar)?;
        writeln!(
            f,
            "  Current Step:       {}",
            status_enum.step_description()
        )?;
    }

    writeln!(f, "  TXID:               {}", display_or(txid))?;
    match vout {
        Some(v) => {
            writeln!(f, "  Vout:               {}", v)?;
            writeln!(f, "  UTXO Outpoint:      {}:{}", txid, v)?;
        }
        None => {
            if print_na_for_vout {
                writeln!(f, "  Vout:               N/A")?;
                writeln!(f, "  UTXO Outpoint:      N/A")?;
            }
        }
    }
    writeln!(f, "  EVM Addr:           {}", display_or(evm_addr))?;
    writeln!(f, "  Move TXID:          {}", display_or(move_txid))?;
    writeln!(f, "  Mint TXID:          {}", display_or(mint_txid))?;
    writeln!(f, "  Raw MoveToVault TX: {}", display_or(move_tx_raw))
}

#[allow(clippy::too_many_arguments)]
fn format_deposit_status_default(
    f: &mut std::fmt::Formatter<'_>,
    id: u64,
    status: &str,
    raw_status: &str,
    txid: &str,
    move_tx_raw: &str,
    evm_addr: &str,
    move_txid: &str,
    mint_txid: &str,
) -> std::fmt::Result {
    format_deposit_status(
        f,
        id,
        status,
        raw_status,
        txid,
        None,
        evm_addr,
        move_txid,
        move_tx_raw,
        mint_txid,
        false,
    )
}
impl Display for DepositStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.status.is_empty() {
            Cow::Borrowed("--")
        } else {
            Cow::Owned(DepositStatusEnum::from_status(&self.status).as_string())
        };
        format_deposit_status_default(
            f,
            self.id,
            &status,
            &self.status, // Pass raw status for progress bar
            &self.txid,
            &self.move_tx_raw,
            &self.evm_addr,
            &self.move_txid,
            &self.mint_txid,
        )
    }
}

impl Display for DepositStatusWithVout<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = if self.deposit_status.status.is_empty() {
            Cow::Borrowed("--")
        } else {
            Cow::Owned(DepositStatusEnum::from_status(&self.deposit_status.status).as_string())
        };
        format_deposit_status(
            f,
            self.deposit_status.id,
            &status,
            &self.deposit_status.status, // Pass raw status for progress bar
            &self.deposit_status.txid,
            self.vout,
            &self.deposit_status.evm_addr,
            &self.deposit_status.move_txid,
            &self.deposit_status.move_tx_raw,
            &self.deposit_status.mint_txid,
            true,
        )
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WithdrawStatus {
    pub idx: u64,
    pub status: String,
    pub btc_payment_txid: String,
    pub from_safe_withdraw: bool,
    pub optimistic_payout_started_at: Option<String>,
    pub optimistic_payout_deadline_at: Option<String>,
    pub created_at: String,
    pub optimistic_payout_payment: Option<OptimisticPayoutStatus>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct OptimisticPayoutStatus {
    pub tx_raw: String,
    pub txid: String,
}

impl Display for WithdrawStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn display_or(v: &str) -> &str {
            if v.is_empty() { "--" } else { v }
        }

        fn display_option_or<T: ToString>(v: &Option<T>) -> String {
            if v.is_none() {
                "--".to_string()
            } else {
                v.as_ref().unwrap().to_string()
            }
        }

        fn display_t<T: ToString + Display>(v: &T) -> String {
            v.to_string()
        }

        let status = if self.status.is_empty() {
            Cow::Borrowed("--")
        } else {
            Cow::Owned(WithdrawStatusEnum::from_backend_status(&self.status).as_string())
        };

        writeln!(f, "\nWithdrawal Info")?;
        writeln!(f, "  Index:                 {}", self.idx)?;
        writeln!(f, "  Status:                {}", status)?;
        writeln!(
            f,
            "  BTC Payment TXID:      {}",
            display_or(&self.btc_payment_txid)
        )?;
        writeln!(
            f,
            "  From Safe Withdraw:    {}",
            display_t(&self.from_safe_withdraw)
        )?;
        writeln!(
            f,
            "  Payout Started:        {}",
            display_option_or(&self.optimistic_payout_started_at)
        )?;
        writeln!(
            f,
            "  Payout Deadline:       {}",
            display_option_or(&self.optimistic_payout_deadline_at)
        )?;
        writeln!(
            f,
            "  Created:               {}",
            display_or(&self.created_at)
        )?;
        writeln!(f, "  Optimistic Payout Info")?;
        let (raw_tx, txid) = match &self.optimistic_payout_payment {
            Some(payout) => (display_or(&payout.tx_raw), display_or(&payout.txid)),
            None => ("--", "--"),
        };
        writeln!(f, "    Raw TX: {}", raw_tx)?;
        writeln!(f, "    TXID:   {}", txid)?;
        Ok(())
    }
}
