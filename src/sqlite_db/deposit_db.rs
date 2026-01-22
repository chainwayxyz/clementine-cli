use crate::core::errors::BridgeCliError;
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
            tracing::error!("Invalid network '{}' in deposits table: {}", row.network, e);
            BridgeCliError::Eyre(eyre!(
                "Invalid network '{}' stored in deposits table",
                row.network
            ))
        })?;

        let user_takes_after = u64::try_from(row.user_takes_after).map_err(|e| {
            tracing::error!(
                "Invalid user_takes_after '{}' in deposits table: {}",
                row.user_takes_after,
                e
            );
            BridgeCliError::Eyre(eyre!(
                "Invalid user_takes_after '{}' stored in deposits table",
                row.user_takes_after
            ))
        })?;

        let created_at = u64::try_from(row.created_at).map_err(|e| {
            tracing::error!(
                "Invalid created_at '{}' in deposits table: {}",
                row.created_at,
                e
            );
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

impl DepositTable {
    pub(crate) async fn insert_deposit(
        pool: &Pool<Sqlite>,
        deposit: &DepositRecord,
    ) -> Result<(), BridgeCliError> {
        let user_takes_after = i64::try_from(deposit.user_takes_after).map_err(|e| {
            tracing::error!(
                "Invalid user_takes_after '{}' for deposits table: {}",
                deposit.user_takes_after,
                e
            );
            BridgeCliError::Eyre(eyre!(
                "Invalid user_takes_after '{}' for deposits table",
                deposit.user_takes_after
            ))
        })?;
        let created_at = i64::try_from(deposit.created_at).map_err(|e| {
            tracing::error!(
                "Invalid created_at '{}' for deposits table: {}",
                deposit.created_at,
                e
            );
            BridgeCliError::Eyre(eyre!(
                "Invalid created_at '{}' for deposits table",
                deposit.created_at
            ))
        })?;

        sqlx::query(
            "INSERT INTO deposits (deposit_address, aggregated_public_key, \
             recovery_taproot_address, citrea_address, user_takes_after, network, created_at) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT DO NOTHING",
        )
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
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM deposits WHERE deposit_address = ?1 LIMIT 1")
                .bind(deposit_address)
                .fetch_optional(pool)
                .await
                .wrap_err("Failed to check deposit existence in database")?;

        Ok(exists.is_some())
    }

    pub(crate) async fn get_all_deposits(
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<DepositRecord>, BridgeCliError> {
        let rows = sqlx::query_as::<_, DepositRow>(
            "SELECT deposit_address, aggregated_public_key, recovery_taproot_address, \
             citrea_address, user_takes_after, network, created_at FROM deposits \
             ORDER BY created_at ASC, deposit_address ASC",
        )
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
        let row: Option<DepositRow> = sqlx::query_as::<_, DepositRow>(
            "SELECT deposit_address, aggregated_public_key, recovery_taproot_address, \
             citrea_address, user_takes_after, network, created_at FROM deposits \
             WHERE deposit_address = ?1",
        )
        .bind(deposit_address)
        .fetch_optional(pool)
        .await
        .wrap_err("Failed to fetch deposit by address from database")?;

        row.map(DepositRecord::try_from).transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use sqlx::migrate::Migrator;
    use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

    static MIGRATOR: Migrator = sqlx::migrate!();

    async fn setup_db() -> Result<Pool<Sqlite>, BridgeCliError> {
        let options = SqliteConnectOptions::new()
            .filename(":memory:")
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .wrap_err("Failed to open in-memory SQLite database")?;

        MIGRATOR.run(&pool).await.map_err(|e| {
            tracing::error!("Failed to run database migrations: {}", e);
            BridgeCliError::Eyre(eyre!("Failed to run database migrations"))
        })?;

        Ok(pool)
    }

    #[tokio::test]
    async fn insert_deposit_conflict_does_nothing() -> Result<(), BridgeCliError> {
        let pool = setup_db().await?;

        let deposit = DepositRecord {
            deposit_address: "depb1qhf0k9x2e0et39wzu8h5qqqqqqqqqqqqqqqqqqqqqqqqqqqg0ftqv"
                .to_string(),
            aggregated_public_key: "02abcdef".to_string(),
            recovery_taproot_address: "bcrt1p7exampleaddress000000000000000000000000000"
                .to_string(),
            citrea_address: "citrea1exampleaddress000000000000000000000000".to_string(),
            user_takes_after: 10,
            network: Network::Testnet4,
            created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap().timestamp() as u64,
        };

        let mut deposit_conflict = deposit.clone();
        deposit_conflict.aggregated_public_key = "02deadbeef".to_string();
        deposit_conflict.citrea_address = "citrea1different".to_string();
        deposit_conflict.created_at += 100;

        DepositTable::insert_deposit(&pool, &deposit).await?;
        DepositTable::insert_deposit(&pool, &deposit_conflict).await?; // should be ignored by ON CONFLICT

        let deposits = DepositTable::get_all_deposits(&pool).await?;
        assert_eq!(deposits.len(), 1);
        assert_eq!(deposits[0].deposit_address, deposit.deposit_address);

        let fetched = DepositTable::get_deposit_by_address(&pool, &deposit.deposit_address)
            .await?
            .expect("deposit should exist");
        assert_eq!(fetched.aggregated_public_key, deposit.aggregated_public_key);
        assert_eq!(fetched.citrea_address, deposit.citrea_address);
        assert_eq!(fetched.created_at, deposit.created_at);

        Ok(())
    }

    #[tokio::test]
    async fn insert_and_fetch_deposit_roundtrip() -> Result<(), BridgeCliError> {
        let pool = setup_db().await?;

        let deposit = DepositRecord {
            deposit_address: "depb1qy9x5k2r6p4n8zzexampleexampleexample0000000".to_string(),
            aggregated_public_key: "03123456abcd".to_string(),
            recovery_taproot_address: "bcrt1p7roundtripaddress00000000000000000000000000"
                .to_string(),
            citrea_address: "citrea1roundtrip0000000000000000000000000".to_string(),
            user_takes_after: 42,
            network: Network::Signet,
            created_at: Utc.timestamp_opt(1_700_100_000, 0).unwrap().timestamp() as u64,
        };

        DepositTable::insert_deposit(&pool, &deposit).await?;

        let fetched = DepositTable::get_deposit_by_address(&pool, &deposit.deposit_address)
            .await?
            .expect("deposit should exist");

        assert_eq!(fetched.deposit_address, deposit.deposit_address);
        assert_eq!(fetched.aggregated_public_key, deposit.aggregated_public_key);
        assert_eq!(
            fetched.recovery_taproot_address,
            deposit.recovery_taproot_address
        );
        assert_eq!(fetched.citrea_address, deposit.citrea_address);
        assert_eq!(fetched.user_takes_after, deposit.user_takes_after);
        assert_eq!(fetched.network, deposit.network);
        assert_eq!(fetched.created_at, deposit.created_at);

        Ok(())
    }
}
