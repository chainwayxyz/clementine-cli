use std::borrow::Cow;
use std::fmt::Display;

use serde::{Deserialize, Serialize};

pub(crate) enum WithdrawStatusEnum {
    New,
    InProgress,
    OptimisticPayoutFailed,
    Completed,
    Unknown,
}

impl WithdrawStatusEnum {
    pub(crate) fn from_backend_status(status: &str) -> Self {
        match status {
            "new" => WithdrawStatusEnum::New,
            "completed" => WithdrawStatusEnum::Completed,
            "sending-to-optimistic-payout"
            | "sent-to-optimistic-payout"
            | "sending-to-operator-withdraw"
            | "sent-to-operator-withdraw" => WithdrawStatusEnum::InProgress,
            "optimistic-payout-failed" => WithdrawStatusEnum::OptimisticPayoutFailed,
            unknown_status => {
                tracing::debug!("Returned unknown status: {}", unknown_status);
                WithdrawStatusEnum::Unknown
            }
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            WithdrawStatusEnum::New => "New".to_string(),
            WithdrawStatusEnum::InProgress => "In Progress".to_string(),
            WithdrawStatusEnum::OptimisticPayoutFailed => {
                "Optimistic payout failed! Please proceed with operator paid withdrawal..."
                    .to_string()
            }
            WithdrawStatusEnum::Completed => "Completed".to_string(),
            WithdrawStatusEnum::Unknown => "Unknown".to_string(),
        }
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
