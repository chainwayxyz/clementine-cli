use std::borrow::Cow;
use std::fmt::Display;

use serde::{Deserialize, Serialize};

use crate::core::status_format::{Row, write_rows};

const KV_INDENT: &str = "  ";
const KV_GAP: &str = "    ";
const SUB_KV_INDENT: &str = "    ";
const SUB_KV_GAP: &str = " ";

pub(crate) enum WithdrawStatusEnum {
    New,
    InProgress,
    SendingToOptimisticPayout,
    OptimisticPayoutFailed,
    SendingToOperatorWithdraw,
    Completed,
    Unknown,
}

impl WithdrawStatusEnum {
    pub(crate) fn from_backend_status(status: &str) -> Self {
        match status {
            "new" | "cleared" => WithdrawStatusEnum::New,
            "completed" => WithdrawStatusEnum::Completed,
            "sanctioned" => WithdrawStatusEnum::InProgress,
            "sending-to-optimistic-payout" | "sent-to-optimistic-payout" => {
                WithdrawStatusEnum::SendingToOptimisticPayout
            }
            "sending-to-operator-withdraw" | "sent-to-operator-withdraw" => {
                WithdrawStatusEnum::SendingToOperatorWithdraw
            }
            "optimistic-payout-failed" => WithdrawStatusEnum::OptimisticPayoutFailed,
            unknown_status => {
                tracing::debug!("Returned unknown status: {}", unknown_status);
                WithdrawStatusEnum::Unknown
            }
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            WithdrawStatusEnum::New => {
                "Withdrawal detected on Citrea, initiating withdrawal.".to_string()
            }
            WithdrawStatusEnum::InProgress => "In Progress".to_string(),
            WithdrawStatusEnum::SendingToOptimisticPayout => {
                "Sending optimistic payout request to Clementine verifiers.".to_string()
            }
            WithdrawStatusEnum::OptimisticPayoutFailed => {
                "Optimistic payout failed! Please proceed with operator paid withdrawal."
                    .to_string()
            }
            WithdrawStatusEnum::SendingToOperatorWithdraw => {
                "Sending payout request to operators.".to_string()
            }
            WithdrawStatusEnum::Completed => {
                "Withdrawal completed! Please check your funds on your receiving address."
                    .to_string()
            }
            WithdrawStatusEnum::Unknown => "Unknown".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct WithdrawStatus {
    pub idx: u64,
    pub status: String,
    pub btc_payment_txid: Option<String>,
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

        let status = if self.status.is_empty() {
            Cow::Borrowed("--")
        } else {
            Cow::Owned(WithdrawStatusEnum::from_backend_status(&self.status).as_string())
        };

        writeln!(f, "\nWithdrawal Info")?;
        let rows = [
            Row {
                label: "Index:",
                value: self.idx.to_string(),
            },
            Row {
                label: "Status:",
                value: status.to_string(),
            },
            Row {
                label: "BTC Payment TXID:",
                value: display_option_or(&self.btc_payment_txid),
            },
            Row {
                label: "From Safe Withdraw:",
                value: self.from_safe_withdraw.to_string(),
            },
            Row {
                label: "Payout Started:",
                value: display_option_or(&self.optimistic_payout_started_at),
            },
            Row {
                label: "Payout Deadline:",
                value: display_option_or(&self.optimistic_payout_deadline_at),
            },
            Row {
                label: "Created:",
                value: display_or(&self.created_at).to_string(),
            },
        ];
        write_rows(f, KV_INDENT, KV_GAP, &rows)?;
        writeln!(f, "  Optimistic Payout Info")?;
        let (raw_tx, txid) = match &self.optimistic_payout_payment {
            Some(payout) => (display_or(&payout.tx_raw), display_or(&payout.txid)),
            None => ("--", "--"),
        };
        let sub_rows = [
            Row {
                label: "Raw TX:",
                value: raw_tx.to_string(),
            },
            Row {
                label: "TXID:",
                value: txid.to_string(),
            },
        ];
        write_rows(f, SUB_KV_INDENT, SUB_KV_GAP, &sub_rows)?;
        Ok(())
    }
}
