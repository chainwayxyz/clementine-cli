use crate::sqlite_db::sqlite_client::SqliteTable;
use crate::structs::AddrDisplay;
use crate::wallet::encryption::EncryptedDataHex;
use crate::{errors::BridgeCliError, structs::TaprootAddressWithPrefix};
use bitcoin::address::{NetworkChecked, NetworkValidation};
use bitcoin::{Address, Network};
use chrono::{DateTime, Utc};
use eyre::{eyre, Context};
use serde::{Deserialize, Serialize};
use sqlx::types::Json;
use sqlx::{FromRow, Pool, Sqlite};
use std::str::FromStr;

#[derive(Debug, Clone)]
pub(crate) struct WalletData {
    pub label: String,
    pub address: TaprootAddressWithPrefix<NetworkChecked>,
    pub network: Network,
    pub encrypted_mnemonic: Option<EncryptedDataHex>,
    pub encrypted_private_key: Option<EncryptedDataHex>,
    pub created_at: DateTime<Utc>,
    pub encryption_method: String,
    pub imported: Option<bool>,
    pub import_method: Option<String>,
}

/// Stable on-disk export/import format for wallets.
///
/// This exists to decouple backup files from the SQLx row mapping and DB schema.
/// The DB schema can evolve without breaking existing wallet backups.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct WalletExport {
    pub label: String,
    pub address: String,
    pub network: String,
    pub encrypted_mnemonic: Option<EncryptedDataHex>,
    pub encrypted_private_key: Option<EncryptedDataHex>,
    pub created_at: String,
    pub encryption_method: String,
    pub imported: Option<bool>,
    pub import_method: Option<String>,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub(crate) struct WalletRaw {
    label: String,
    address: String,
    network: String,
    encrypted_mnemonic: Option<Json<EncryptedDataHex>>,
    encrypted_private_key: Option<Json<EncryptedDataHex>>,
    created_at: String,
    encryption_method: String,
    imported: Option<bool>,
    import_method: Option<String>,
}

impl From<&WalletData> for WalletExport {
    fn from(wallet: &WalletData) -> Self {
        WalletExport {
            label: wallet.label.clone(),
            address: wallet.address.address_with_prefix(),
            network: wallet.network.to_string(),
            encrypted_mnemonic: wallet.encrypted_mnemonic.clone(),
            encrypted_private_key: wallet.encrypted_private_key.clone(),
            created_at: wallet.created_at.to_rfc3339(),
            encryption_method: wallet.encryption_method.clone(),
            imported: wallet.imported,
            import_method: wallet.import_method.clone(),
        }
    }
}

impl TryFrom<WalletExport> for WalletData {
    type Error = BridgeCliError;

    fn try_from(export: WalletExport) -> Result<Self, Self::Error> {
        let network = Network::from_str(&export.network).map_err(|e| {
            BridgeCliError::Eyre(eyre!(
                "Invalid network '{}' stored in wallet export: {e}",
                export.network
            ))
        })?;

        let address = TaprootAddressWithPrefix::from_string_with_prefix(&export.address, network)
            .map_err(|e| {
                BridgeCliError::Eyre(eyre!(
                    "Invalid address '{}' stored in wallet export: {e}",
                    export.address
                ))
            })?;

        let created_at = DateTime::parse_from_rfc3339(&export.created_at)
            .map_err(|e| {
                BridgeCliError::Eyre(eyre!(
                    "Invalid timestamp '{}' stored in wallet export: {e}",
                    export.created_at
                ))
            })?
            .with_timezone(&Utc);

        Ok(WalletData {
            label: export.label,
            address,
            network,
            encrypted_mnemonic: export.encrypted_mnemonic,
            encrypted_private_key: export.encrypted_private_key,
            created_at,
            encryption_method: export.encryption_method,
            imported: export.imported,
            import_method: export.import_method,
        })
    }
}

impl TryFrom<WalletRaw> for WalletData {
    type Error = BridgeCliError;

    fn try_from(row: WalletRaw) -> Result<Self, Self::Error> {
        let network = Network::from_str(&row.network).map_err(|e| {
            BridgeCliError::Eyre(eyre!(
                "Invalid network '{}' stored in wallets table: {e}",
                row.network
            ))
        })?;

        let address = TaprootAddressWithPrefix::from_string_with_prefix(&row.address, network)
            .map_err(|e| {
                BridgeCliError::Eyre(eyre!(
                    "Invalid address '{}' stored in wallets table: {e}",
                    row.address
                ))
            })?;

        let created_at = DateTime::parse_from_rfc3339(&row.created_at)
            .map_err(|e| {
                BridgeCliError::Eyre(eyre!(
                    "Invalid timestamp '{}' stored in wallets table: {e}",
                    row.created_at
                ))
            })?
            .with_timezone(&Utc);
        let encrypted_mnemonic = row.encrypted_mnemonic.map(|Json(v)| v);
        let encrypted_private_key = row.encrypted_private_key.map(|Json(v)| v);

        Ok(WalletData {
            label: row.label,
            address,
            network,
            encrypted_mnemonic,
            encrypted_private_key,
            created_at,
            encryption_method: row.encryption_method,
            imported: row.imported,
            import_method: row.import_method,
        })
    }
}

impl Into<WalletRaw> for WalletData {
    fn into(self) -> WalletRaw {
        WalletRaw {
            label: self.label,
            address: self.address.address_with_prefix(),
            network: self.network.to_string(),
            encrypted_mnemonic: self.encrypted_mnemonic.map(|v| Json(v)),
            encrypted_private_key: self.encrypted_private_key.map(|v| Json(v)),
            created_at: self.created_at.to_rfc3339(),
            encryption_method: self.encryption_method,
            imported: self.imported,
            import_method: self.import_method,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct MinimalWalletData {
    pub label: String,
    pub address: String,
    pub network: String,
    pub created_at: String,
    pub imported: Option<bool>,
    pub import_method: Option<String>,
}

pub struct WalletTable;

impl SqliteTable for WalletTable {
    const TABLE_NAME: &'static str = "wallets";

    const CREATE_SQL: &'static str = r#"
        CREATE TABLE IF NOT EXISTS wallets (
            id                     INTEGER PRIMARY KEY AUTOINCREMENT,
            label                  TEXT    NOT NULL UNIQUE,
            address                TEXT    NOT NULL UNIQUE,
            network                TEXT    NOT NULL,
            encrypted_mnemonic     TEXT,
            encrypted_private_key  TEXT,
            created_at             TEXT    NOT NULL,
            encryption_method      TEXT    NOT NULL,
            imported               INTEGER,
            import_method          TEXT
        );
    "#;
}

impl WalletTable {
    pub(crate) async fn insert_wallet(
        pool: &Pool<Sqlite>,
        wallet: &WalletData,
    ) -> Result<(), BridgeCliError> {
        let enc_mn = wallet.encrypted_mnemonic.as_ref().map(|e| Json(e.clone()));

        let enc_pk = wallet
            .encrypted_private_key
            .as_ref()
            .map(|e| Json(e.clone()));

        sqlx::query(&format!(
            "INSERT INTO {} (label, address, network, encrypted_mnemonic, \
                 encrypted_private_key, created_at, encryption_method, imported, import_method) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            Self::TABLE_NAME
        ))
        .bind(&wallet.label)
        .bind(wallet.address.address_with_prefix())
        .bind(wallet.network.to_string())
        .bind(enc_mn)
        .bind(enc_pk)
        .bind(wallet.created_at.to_rfc3339())
        .bind(&wallet.encryption_method)
        .bind(wallet.imported.map(|b| if b { 1 } else { 0 }))
        .bind(&wallet.import_method)
        .execute(pool)
        .await
        .wrap_err("Failed to insert wallet into database")?;

        Ok(())
    }

    pub(crate) async fn get_all_wallets(
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<MinimalWalletData>, BridgeCliError> {
        let rows = sqlx::query_as::<_, MinimalWalletData>(&format!(
            "SELECT label, address, network, created_at, imported, import_method FROM {}",
            Self::TABLE_NAME
        ))
        .fetch_all(pool)
        .await
        .wrap_err("Failed to fetch wallets from database")?;

        Ok(rows)
    }

    pub(crate) async fn get_wallet_by_address<T>(
        pool: &Pool<Sqlite>,
        address: TaprootAddressWithPrefix<T>,
    ) -> Result<Option<WalletData>, BridgeCliError>
    where
        T: NetworkValidation,
        Address<T>: AddrDisplay,
    {
        let row: Option<WalletRaw> = sqlx::query_as::<_, WalletRaw>(&format!(
            "SELECT label, address, network, encrypted_mnemonic, \
             encrypted_private_key, created_at, encryption_method, imported, import_method \
             FROM {} WHERE address = ?1",
            Self::TABLE_NAME
        ))
        .bind(address.address_with_prefix())
        .fetch_optional(pool)
        .await
        .wrap_err("Failed to fetch wallet by address from database")?;

        Ok(row.map(WalletData::try_from).transpose()?)
    }

    pub async fn label_exists(
        pool: &Pool<Sqlite>,
        label: &str,
    ) -> Result<bool, BridgeCliError> {
        let exists: Option<i64> = sqlx::query_scalar(&format!(
            "SELECT 1 FROM {} WHERE label = ?1 LIMIT 1",
            Self::TABLE_NAME
        ))
        .bind(label)
        .fetch_optional(pool)
        .await
        .wrap_err("Failed to check wallet label existence in database")?;

        Ok(exists.is_some())
    }

    pub async fn address_exists<T>(
        pool: &Pool<Sqlite>,
        address: &TaprootAddressWithPrefix<T>,
    ) -> Result<bool, BridgeCliError>
    where
        T: NetworkValidation,
        Address<T>: AddrDisplay,
    {
        let exists: Option<i64> = sqlx::query_scalar(&format!(
            "SELECT 1 FROM {} WHERE address = ?1 LIMIT 1",
            Self::TABLE_NAME
        ))
        .bind(address.address_with_prefix())
        .fetch_optional(pool)
        .await
        .wrap_err("Failed to check wallet address existence in database")?;

        Ok(exists.is_some())
    }
}
