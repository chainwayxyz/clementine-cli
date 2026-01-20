use std::borrow::Cow;
use std::fmt::Display;

use serde::{Deserialize, Serialize};

pub(crate) enum DepositStatusEnum {
    New,
    InProgress,
    MoveTxSent,
    Completed,
    Unknown,
}

impl DepositStatusEnum {
    pub(crate) fn from_status(status: &str) -> Self {
        match status {
            "new" => DepositStatusEnum::New,
            "minted" => DepositStatusEnum::Completed,
            "flushing_initiating" | "flushing_initiated" | "flushing_broadcasting" => {
                DepositStatusEnum::InProgress
            }
            "sent" => DepositStatusEnum::MoveTxSent,
            _ => DepositStatusEnum::Unknown,
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            DepositStatusEnum::New => "New".to_string(),
            DepositStatusEnum::InProgress => "In Progress".to_string(),
            DepositStatusEnum::Completed => "Completed".to_string(),
            DepositStatusEnum::MoveTxSent => "Move To Vault Transaction Sent".to_string(),
            DepositStatusEnum::Unknown => "Unknown".to_string(),
        }
    }

    /// Returns the progress position (current step, total steps)
    pub fn progress(&self) -> (usize, usize) {
        match self {
            DepositStatusEnum::New => (1, 4),
            DepositStatusEnum::InProgress => (2, 4),
            DepositStatusEnum::MoveTxSent => (3, 4),
            DepositStatusEnum::Completed => (4, 4),
            DepositStatusEnum::Unknown => (0, 4),
        }
    }

    /// Returns a description of the current step
    pub fn step_description(&self) -> &str {
        match self {
            DepositStatusEnum::New => "Deposit detected on Bitcoin network",
            DepositStatusEnum::InProgress => "The deposit is being processed",
            DepositStatusEnum::MoveTxSent => {
                "Move transaction broadcasted, waiting for confirmation and minting"
            }
            DepositStatusEnum::Completed => "Funds minted on Citrea network",
            DepositStatusEnum::Unknown => "Status unknown",
        }
    }
}

const PROGRESS_BAR_WIDTH: usize = 40;

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
    pub remaining_finalization_blocks: Option<u64>,
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
    move_tx_finalization_blocks: Option<u64>,
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

    let remaining_blocks_msg = match move_tx_finalization_blocks {
        Some(0) => "Finalized".to_string(),
        Some(blocks) => format!("Approx. {} blocks remaining", blocks),
        None => "N/A".to_string(),
    };

    writeln!(f, "  EVM Addr:           {}", display_or(evm_addr))?;
    writeln!(f, "  Move TXID:          {}", display_or(move_txid))?;
    writeln!(
        f,
        "  Move Tx Status:     {}",
        display_or(&remaining_blocks_msg)
    )?;
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
        None,
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
            self.remaining_finalization_blocks,
            &self.deposit_status.move_tx_raw,
            &self.deposit_status.mint_txid,
            true,
        )
    }
}
