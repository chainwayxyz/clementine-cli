use crate::structs::AddrDisplay;
use crate::wallet::encryption::EncryptedDataHex;
use crate::wallet::ImportMethod;
use crate::{errors::BridgeCliError, structs::TaprootAddressWithPrefix};
use bitcoin::address::{NetworkChecked, NetworkValidation};
use bitcoin::{Address, Network};
use chrono::{DateTime, TimeZone, Utc};
use eyre::{Context, eyre};
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
    pub encrypted_private_key: EncryptedDataHex,
    pub created_at: DateTime<Utc>,
    pub encryption_method: String,
    pub imported: bool,
    pub original_import_method: Option<ImportMethod>,
    pub import_method: Option<ImportMethod>,
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
    pub encrypted_private_key: EncryptedDataHex,
    pub created_at: String,
    pub encryption_method: String,
    pub imported: bool,
    pub original_import_method: Option<ImportMethod>,
    pub import_method: Option<ImportMethod>,
}

#[derive(Debug, FromRow, Serialize, Deserialize)]
pub(crate) struct WalletRaw {
    label: String,
    address: String,
    network: String,
    encrypted_mnemonic: Option<Json<EncryptedDataHex>>,
    encrypted_private_key: Json<EncryptedDataHex>,
    created_at: i64,
    encryption_method: String,
    imported: bool,
    original_import_method: Option<String>,
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
            original_import_method: wallet.original_import_method.clone(),
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
            original_import_method: export.original_import_method,
            import_method: export.import_method,
        })
    }
}

fn parse_import_method(
    value: Option<&str>,
    field: &str,
) -> Result<Option<ImportMethod>, BridgeCliError> {
    value
        .map(|s| {
            ImportMethod::from_str(s).map_err(|_| {
                BridgeCliError::Eyre(eyre!("Invalid {} '{}' stored in wallets table", field, s))
            })
        })
        .transpose()
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

        let created_at = Utc
            .timestamp_opt(row.created_at, 0)
            .single()
            .ok_or_else(|| {
                BridgeCliError::Eyre(eyre!(
                    "Invalid created_at '{}' stored in wallets table",
                    row.created_at
                ))
            })?;
        let encrypted_mnemonic = row.encrypted_mnemonic.map(|Json(v)| v);
        let encrypted_private_key = row.encrypted_private_key.0;

        let original_import_method =
            parse_import_method(row.original_import_method.as_deref(), "original_import_method")?;
        let import_method = parse_import_method(row.import_method.as_deref(), "import_method")?;

        Ok(WalletData {
            label: row.label,
            address,
            network,
            encrypted_mnemonic,
            encrypted_private_key,
            created_at,
            encryption_method: row.encryption_method,
            imported: row.imported,
            original_import_method,
            import_method,
        })
    }
}

impl From<WalletData> for WalletRaw {
    fn from(data: WalletData) -> WalletRaw {
        let import_method = data.import_method.map(|m| m.to_string());
        WalletRaw {
            label: data.label,
            address: data.address.address_with_prefix(),
            network: data.network.to_string(),
            encrypted_mnemonic: data.encrypted_mnemonic.map(Json),
            encrypted_private_key: Json(data.encrypted_private_key),
            created_at: data.created_at.timestamp(),
            encryption_method: data.encryption_method,
            imported: data.imported,
            original_import_method: data.original_import_method.map(|m| m.to_string()),
            import_method,
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub(crate) struct MinimalWalletData {
    pub label: String,
    pub address: String,
    pub network: String,
    pub created_at: i64,
    pub imported: bool,
    pub import_method: Option<String>,
}

pub struct WalletTable;

impl WalletTable {
    pub(crate) async fn insert_wallet(
        pool: &Pool<Sqlite>,
        wallet: &WalletData,
    ) -> Result<(), BridgeCliError> {
        let enc_mn = wallet.encrypted_mnemonic.as_ref().map(|e| Json(e.clone()));

        let enc_pk = Json(wallet.encrypted_private_key.clone());

        sqlx::query(
              "INSERT INTO wallets (label, address, network, encrypted_mnemonic, \
                  encrypted_private_key, created_at, encryption_method, imported, original_import_method, import_method) \
                  VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                  ON CONFLICT DO NOTHING",
        )
        .bind(&wallet.label)
        .bind(wallet.address.address_with_prefix())
        .bind(wallet.network.to_string())
        .bind(enc_mn)
        .bind(enc_pk)
        .bind(wallet.created_at.timestamp())
        .bind(&wallet.encryption_method)
        .bind(if wallet.imported { 1 } else { 0 })
           .bind(wallet.original_import_method.as_ref().map(|m| m.to_string()))
        .bind(wallet.import_method.as_ref().map(|m| m.to_string()))
        .execute(pool)
        .await
        .wrap_err("Failed to insert wallet into database")?;

        Ok(())
    }

    pub(crate) async fn get_all_wallets(
        pool: &Pool<Sqlite>,
    ) -> Result<Vec<MinimalWalletData>, BridgeCliError> {
        let rows = sqlx::query_as::<_, MinimalWalletData>(
            "SELECT label, address, network, created_at, imported, import_method FROM wallets",
        )
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
        let row: Option<WalletRaw> = sqlx::query_as::<_, WalletRaw>(
            "SELECT label, address, network, encrypted_mnemonic, \
                             encrypted_private_key, created_at, encryption_method, imported, original_import_method, import_method \
             FROM wallets WHERE address = ?1",
        )
        .bind(address.address_with_prefix())
        .fetch_optional(pool)
        .await
        .wrap_err("Failed to fetch wallet by address from database")?;

        row.map(WalletData::try_from).transpose()
    }

    pub async fn label_exists(pool: &Pool<Sqlite>, label: &str) -> Result<bool, BridgeCliError> {
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM wallets WHERE label = ?1 LIMIT 1")
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
        let exists: Option<i64> =
            sqlx::query_scalar("SELECT 1 FROM wallets WHERE address = ?1 LIMIT 1")
                .bind(address.address_with_prefix())
                .fetch_optional(pool)
                .await
                .wrap_err("Failed to check wallet address existence in database")?;

        Ok(exists.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
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
    async fn insert_and_fetch_wallet_roundtrip() -> Result<(), BridgeCliError> {
        let pool = setup_db().await?;

        let address = TaprootAddressWithPrefix::from_string_with_prefix(
            "wittb1p6j7432u040m9xmpumtmjk0vn5mwkzhm8g6xaphkmmgjsj87r4p2sqje5p7",
            Network::Testnet4,
        )?;

        let created_at = Utc.timestamp_opt(1_700_000_000, 0).unwrap();

        let wallet = WalletData {
            label: "test-wallet".to_string(),
            address: address.clone(),
            network: Network::Testnet4,
            encrypted_mnemonic: Some(EncryptedDataHex {
                ciphertext: "00".to_string(),
                nonce: "00".to_string(),
                salt: "00".to_string(),
            }),
            encrypted_private_key: EncryptedDataHex {
                ciphertext: "11".to_string(),
                nonce: "11".to_string(),
                salt: "11".to_string(),
            },
            created_at,
            encryption_method: "test-method".to_string(),
            imported: true,
            original_import_method: Some(ImportMethod::FileImport),
            import_method: Some(ImportMethod::FileImport),
        };

        WalletTable::insert_wallet(&pool, &wallet).await?;

        let fetched = WalletTable::get_wallet_by_address(&pool, address.clone())
            .await?
            .expect("wallet should exist");

        assert_eq!(fetched.label, wallet.label);
        assert_eq!(
            fetched.address.address_with_prefix(),
            wallet.address.address_with_prefix()
        );
        assert_eq!(fetched.network, wallet.network);
        assert_eq!(fetched.encrypted_mnemonic, wallet.encrypted_mnemonic);
        assert_eq!(fetched.encrypted_private_key, wallet.encrypted_private_key);
        assert_eq!(fetched.created_at, wallet.created_at);
        assert_eq!(fetched.encryption_method, wallet.encryption_method);
        assert_eq!(fetched.imported, wallet.imported);
        assert_eq!(fetched.original_import_method, wallet.original_import_method);
        assert_eq!(fetched.import_method, wallet.import_method);

        let export = WalletExport::from(&fetched);
        let roundtrip = WalletData::try_from(export)?;
        assert_eq!(roundtrip.label, wallet.label);
        assert_eq!(
            roundtrip.address.address_with_prefix(),
            wallet.address.address_with_prefix()
        );
        assert_eq!(roundtrip.network, wallet.network);
        assert_eq!(roundtrip.encrypted_mnemonic, wallet.encrypted_mnemonic);
        assert_eq!(
            roundtrip.encrypted_private_key,
            wallet.encrypted_private_key
        );
        assert_eq!(roundtrip.created_at, wallet.created_at);
        assert_eq!(roundtrip.encryption_method, wallet.encryption_method);
        assert_eq!(roundtrip.imported, wallet.imported);
        assert_eq!(roundtrip.original_import_method, wallet.original_import_method);
        assert_eq!(roundtrip.import_method, wallet.import_method);

        Ok(())
    }

    #[tokio::test]
    async fn insert_wallet_conflict_does_nothing() -> Result<(), BridgeCliError> {
        let pool = setup_db().await?;

        let address = TaprootAddressWithPrefix::from_string_with_prefix(
            "wittb1p6j7432u040m9xmpumtmjk0vn5mwkzhm8g6xaphkmmgjsj87r4p2sqje5p7",
            Network::Testnet4,
        )?;

        let wallet = WalletData {
            label: "conflict-wallet".to_string(),
            address: address.clone(),
            network: Network::Testnet4,
            encrypted_mnemonic: None,
            encrypted_private_key: EncryptedDataHex {
                ciphertext: "22".to_string(),
                nonce: "22".to_string(),
                salt: "22".to_string(),
            },
            created_at: Utc.timestamp_opt(1_700_000_000, 0).unwrap(),
            encryption_method: "test-method".to_string(),
            imported: false,
            original_import_method: None,
            import_method: None,
        };

        let mut wallet_conflict = wallet.clone();
        wallet_conflict.encryption_method = "should-not-overwrite".to_string();
        wallet_conflict.imported = true;
        wallet_conflict.created_at = Utc.timestamp_opt(1_800_000_000, 0).unwrap();

        WalletTable::insert_wallet(&pool, &wallet).await?;
        WalletTable::insert_wallet(&pool, &wallet_conflict).await?; // should be ignored by ON CONFLICT

        let wallets = WalletTable::get_all_wallets(&pool).await?;
        assert_eq!(wallets.len(), 1);
        assert_eq!(wallets[0].label, wallet.label);
        assert_eq!(wallets[0].address, wallet.address.address_with_prefix());

        let fetched = WalletTable::get_wallet_by_address(&pool, address)
            .await?
            .expect("wallet should exist");

        assert_eq!(fetched.encryption_method, wallet.encryption_method);
        assert_eq!(fetched.imported, wallet.imported);
        assert_eq!(fetched.created_at, wallet.created_at);

        Ok(())
    }
}
