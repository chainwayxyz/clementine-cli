use crate::errors::BridgeCliError;
use crate::sqlite_db::sqlite_client::SqliteTable;
use bitcoin::Network;
use eyre::{Context, eyre};
use sqlx::{FromRow, Pool, Sqlite};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub(crate) struct DepositRecord {
    pub deposit_address: String,
    pub aggregated_public_key: String,
    pub recovery_taproot_address: String,
    pub citrea_address: String,
    pub user_takes_after: u64,
    pub network: Network,
    pub created_at: u64,
}

#[derive(Debug, FromRow)]
struct DepositRow {
    deposit_address: String,
    aggregated_public_key: String,
    recovery_taproot_address: String,
    citrea_address: String,
    user_takes_after: i64,
    network: String,
    created_at: i64,
}

impl TryFrom<DepositRow> for DepositRecord {
    type Error = BridgeCliError;

    fn try_from(row: DepositRow) -> Result<Self, Self::Error> {
        let network = Network::from_str(&row.network).map_err(|e| {
            BridgeCliError::Eyre(eyre!(
                "Invalid network '{}' stored in deposits table: {e}",
                row.network
            ))
        })?;

        let user_takes_after = u64::try_from(row.user_takes_after).map_err(|_| {
            BridgeCliError::Eyre(eyre!(
                "Invalid user_takes_after '{}' stored in deposits table",
                row.user_takes_after
            ))
        })?;

        let created_at = u64::try_from(row.created_at).map_err(|_| {
            BridgeCliError::Eyre(eyre!(
                "Invalid created_at '{}' stored in deposits table",
                row.created_at
            ))
        })?;

        Ok(DepositRecord {
            deposit_address: row.deposit_address,
            aggregated_public_key: row.aggregated_public_key,
            recovery_taproot_address: row.recovery_taproot_address,
            citrea_address: row.citrea_address,
            user_takes_after,
            network,
            created_at,
        })
    }
}

pub struct DepositTable;

impl SqliteTable for DepositTable {
    const TABLE_NAME: &'static str = "deposits";
}

impl DepositTable {
    pub(crate) async fn insert_deposit(
        pool: &Pool<Sqlite>,
        deposit: &DepositRecord,
    ) -> Result<(), BridgeCliError> {
        let user_takes_after = i64::try_from(deposit.user_takes_after).map_err(|_| {
            BridgeCliError::Eyre(eyre!(
                "Invalid user_takes_after '{}' for deposits table",
                deposit.user_takes_after
            ))
        })?;
        let created_at = i64::try_from(deposit.created_at).map_err(|_| {
            BridgeCliError::Eyre(eyre!(
                "Invalid created_at '{}' for deposits table",
                deposit.created_at
            ))
        })?;

        sqlx::query(&format!(
            "INSERT INTO {} (deposit_address, aggregated_public_key, \
             recovery_taproot_address, citrea_address, user_takes_after, network, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            Self::TABLE_NAME
        ))
        .bind(&deposit.deposit_address)
        .bind(&deposit.aggregated_public_key)
        .bind(&deposit.recovery_taproot_address)
        .bind(&deposit.citrea_address)
        .bind(user_takes_after)
        .bind(deposit.network.to_string())
        .bind(created_at)
        .execute(pool)
        .await
        .wrap_err("Failed to insert deposit into database")?;

        Ok(())
    }

    pub(crate) async fn deposit_exists(
        pool: &Pool<Sqlite>,
        deposit_address: &str,
    ) -> Result<bool, BridgeCliError> {
        let exists: Option<i64> = sqlx::query_scalar(&format!(
            "SELECT 1 FROM {} WHERE deposit_address = ?1 LIMIT 1",
            Self::TABLE_NAME
        ))
        .bind(deposit_address)
        .fetch_optional(pool)
        .await
        .wrap_err("Failed to check deposit existence in database")?;

        Ok(exists.is_some())
    }

    pub(crate) async fn get_all_deposits(
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<DepositRecord>, BridgeCliError> {
        let rows = sqlx::query_as::<_, DepositRow>(&format!(
            "SELECT deposit_address, aggregated_public_key, recovery_taproot_address, \
             citrea_address, user_takes_after, network, created_at FROM {} \
             ORDER BY created_at ASC, deposit_address ASC",
            Self::TABLE_NAME
        ))
        .fetch_all(pool)
        .await
        .wrap_err("Failed to fetch deposits from database")?;

        rows.into_iter()
            .map(DepositRecord::try_from)
            .collect::<Result<Vec<_>, _>>()
    }

    pub(crate) async fn get_deposit_by_address(
        pool: &Pool<Sqlite>,
        deposit_address: &str,
    ) -> Result<Option<DepositRecord>, BridgeCliError> {
        let row: Option<DepositRow> = sqlx::query_as::<_, DepositRow>(&format!(
            "SELECT deposit_address, aggregated_public_key, recovery_taproot_address, \
             citrea_address, user_takes_after, network, created_at FROM {} \
             WHERE deposit_address = ?1",
            Self::TABLE_NAME
        ))
        .bind(deposit_address)
        .fetch_optional(pool)
        .await
        .wrap_err("Failed to fetch deposit by address from database")?;

        row.map(DepositRecord::try_from).transpose()
    }
}
