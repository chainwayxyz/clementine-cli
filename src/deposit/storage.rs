use crate::core::errors::BridgeCliError;
use crate::deposit::CitreaAddress;
use crate::sqlite_db::deposit_db::{DepositRecord, DepositTable};
use crate::sqlite_db::sqlite_client::{SqliteDb, resolve_sqlite_client};
use crate::wallet::BitcoinAddress;
use crate::wallet::TaprootAddressWithPrefix;
use bitcoin::Network;
use bitcoin::address::NetworkUnchecked;
use bitcoin::secp256k1::XOnlyPublicKey;
use std::time::{SystemTime, UNIX_EPOCH};

/// Data needed to store a deposit address.
#[derive(Debug, Clone)]
pub struct DepositData {
    pub deposit_address: BitcoinAddress,
    pub recovery_taproot_address: TaprootAddressWithPrefix<NetworkUnchecked>,
    pub aggregated_public_key: XOnlyPublicKey,
    pub citrea_address: CitreaAddress,
    pub user_takes_after: u64,
    pub network: Network,
}

/// Public view of a stored deposit address.
#[derive(Debug, Clone)]
pub struct DepositAddressDetails {
    pub deposit_address: String,
    pub aggregated_public_key: String,
    pub recovery_taproot_address: String,
    pub citrea_address: String,
    pub user_takes_after: u64,
    pub network: Network,
    pub created_at: u64,
}

#[derive(Debug, PartialEq, Eq)]
pub enum DepositAddressStorageResult {
    Exists,
    NewlyStored,
}

/// Store a deposit address and its metadata.
///
/// If the address already exists, the existing record is kept.
/// Returns an error if the database cannot be read or written.
pub async fn store_deposit_address(
    deposit_data: &DepositData,
    sqlite_client: Option<&SqliteDb>,
) -> Result<DepositAddressStorageResult, BridgeCliError> {
    let sqlite_client = resolve_sqlite_client(sqlite_client).await?;
    let deposit_address = deposit_data.deposit_address.to_string();

    if DepositTable::deposit_exists(sqlite_client.pool(), &deposit_address).await? {
        return Ok(DepositAddressStorageResult::Exists);
    }

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or_else(|e| {
            tracing::warn!("System time error, using 0 as timestamp: {}", e);
            0
        });

    let record = DepositRecord {
        deposit_address,
        aggregated_public_key: deposit_data.aggregated_public_key.to_string(),
        recovery_taproot_address: deposit_data.recovery_taproot_address.address_with_prefix(),
        citrea_address: deposit_data.citrea_address.to_string(),
        user_takes_after: deposit_data.user_takes_after,
        network: deposit_data.network,
        created_at: now,
    };

    DepositTable::insert_deposit(sqlite_client.pool(), &record).await?;

    Ok(DepositAddressStorageResult::NewlyStored)
}

/// List all stored deposits.
///
/// Returns an empty vector if no deposits have been stored.
pub async fn get_all_deposit_address_details(
    sqlite_client: Option<&SqliteDb>,
) -> Result<Vec<DepositAddressDetails>, BridgeCliError> {
    let sqlite_client = resolve_sqlite_client(sqlite_client).await?;
    let records = DepositTable::get_all_deposits(sqlite_client.pool()).await?;

    Ok(records
        .into_iter()
        .map(|record| DepositAddressDetails {
            deposit_address: record.deposit_address,
            aggregated_public_key: record.aggregated_public_key,
            recovery_taproot_address: record.recovery_taproot_address,
            citrea_address: record.citrea_address,
            user_takes_after: record.user_takes_after,
            network: record.network,
            created_at: record.created_at,
        })
        .collect())
}

/// Retrieves full deposit details for a specific deposit address.
///
/// The deposit address is treated as a string key; the function does not
/// perform any additional validation beyond lookup.
///
/// Returns `Ok(Some(..))` if the deposit address exists in storage, or
/// `Ok(None)` if it is not found.
pub async fn get_deposit_address_details_for_deposit_address(
    deposit_address: &str,
    sqlite_client: Option<&SqliteDb>,
) -> Result<Option<DepositAddressDetails>, BridgeCliError> {
    let sqlite_client = resolve_sqlite_client(sqlite_client).await?;
    let record =
        DepositTable::get_deposit_by_address(sqlite_client.pool(), deposit_address).await?;

    Ok(record.map(|record| DepositAddressDetails {
        deposit_address: record.deposit_address,
        aggregated_public_key: record.aggregated_public_key,
        recovery_taproot_address: record.recovery_taproot_address,
        citrea_address: record.citrea_address,
        user_takes_after: record.user_takes_after,
        network: record.network,
        created_at: record.created_at,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::BridgeCliConfig;
    use crate::deposit::CitreaAddress;
    use crate::deposit::get_deposit_address;
    use crate::sqlite_db::test_utils::fresh_db_with_test_name;
    use crate::wallet::BitcoinAddress;
    use crate::wallet::Purpose;
    use crate::wallet::TaprootAddressWithPrefix;
    use bitcoin::key::TweakedPublicKey;
    use bitcoin::secp256k1::{Keypair, Secp256k1, SecretKey, XOnlyPublicKey};

    fn sample_deposit_data(network: Network, seed: u8) -> DepositData {
        let secp = Secp256k1::new();
        let secret_bytes = [seed; 32];
        let secret_key = SecretKey::from_slice(&secret_bytes).expect("valid secret key");
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let (xonly, _) = XOnlyPublicKey::from_keypair(&keypair);
        let tweaked = TweakedPublicKey::dangerous_assume_tweaked(xonly);

        let deposit_address = BitcoinAddress::p2tr_tweaked(tweaked, network);

        let purpose = Purpose::Deposit;
        let recovery_prefixed = format!("{}{}", purpose.to_prefix(), deposit_address);
        let recovery_taproot_address =
            TaprootAddressWithPrefix::from_string_with_prefix_unchecked(&recovery_prefixed)
                .expect("valid recovery address");

        let citrea_address = CitreaAddress::from([seed; 20]);

        DepositData {
            deposit_address,
            recovery_taproot_address,
            aggregated_public_key: xonly,
            citrea_address,
            user_takes_after: 100 + seed as u64,
            network,
        }
    }

    #[tokio::test]
    async fn get_all_deposit_address_details_returns_inserted_records() {
        let db = fresh_db_with_test_name().await;
        let data1 = sample_deposit_data(Network::Regtest, 1);
        let data2 = sample_deposit_data(Network::Regtest, 2);

        store_deposit_address(&data1, Some(&db))
            .await
            .expect("store first deposit");
        store_deposit_address(&data2, Some(&db))
            .await
            .expect("store second deposit");

        let details = get_all_deposit_address_details(Some(&db))
            .await
            .expect("fetch deposits");

        assert_eq!(details.len(), 2);

        let first = details
            .iter()
            .find(|d| d.deposit_address == data1.deposit_address.to_string())
            .expect("first deposit present");
        assert_eq!(
            first.aggregated_public_key,
            data1.aggregated_public_key.to_string()
        );
        assert_eq!(
            first.recovery_taproot_address,
            data1.recovery_taproot_address.address_with_prefix()
        );
        assert_eq!(first.citrea_address, data1.citrea_address.to_string());

        let second = details
            .iter()
            .find(|d| d.deposit_address == data2.deposit_address.to_string())
            .expect("second deposit present");
        assert_eq!(second.network, data2.network);
        assert_eq!(second.user_takes_after, data2.user_takes_after);
    }

    #[tokio::test]
    async fn get_deposit_address_details_returns_specific_record() {
        let db = fresh_db_with_test_name().await;
        let data = sample_deposit_data(Network::Signet, 9);

        store_deposit_address(&data, Some(&db))
            .await
            .expect("store deposit");

        let details = get_deposit_address_details_for_deposit_address(
            &data.deposit_address.to_string(),
            Some(&db),
        )
        .await
        .expect("fetch deposit")
        .expect("deposit exists");

        assert_eq!(details.deposit_address, data.deposit_address.to_string());
        assert_eq!(
            details.aggregated_public_key,
            data.aggregated_public_key.to_string()
        );
        assert_eq!(
            details.recovery_taproot_address,
            data.recovery_taproot_address.address_with_prefix()
        );
        assert_eq!(details.citrea_address, data.citrea_address.to_string());
        assert_eq!(details.user_takes_after, data.user_takes_after);
        assert_eq!(details.network, data.network);

        let missing =
            get_deposit_address_details_for_deposit_address("depb1qmissingaddress", Some(&db))
                .await
                .expect("fetch missing");
        assert!(missing.is_none());
    }

    #[tokio::test]
    async fn get_deposit_address_stores_and_fetches_details() {
        let db = fresh_db_with_test_name().await;

        let secp = Secp256k1::new();
        let secret_key = SecretKey::from_slice(&[7u8; 32]).expect("secret");
        let keypair = Keypair::from_secret_key(&secp, &secret_key);
        let (xonly, _parity) = XOnlyPublicKey::from_keypair(&keypair);
        let tweaked = TweakedPublicKey::dangerous_assume_tweaked(xonly);
        let recovery_address = BitcoinAddress::p2tr_tweaked(tweaked, Network::Regtest);
        let recovery_taproot =
            TaprootAddressWithPrefix::new(recovery_address, Purpose::Deposit).expect("taproot");

        let config = BridgeCliConfig::defaults_for(Network::Regtest);
        let citrea_address = CitreaAddress::from([5u8; 20]);

        let result = get_deposit_address(&citrea_address, &recovery_taproot, &config, Some(&db))
            .await
            .expect("get deposit address");

        let deposit_address = result.deposit_address;
        assert_eq!(
            result.storage_result,
            DepositAddressStorageResult::NewlyStored
        );

        let stored = get_deposit_address_details_for_deposit_address(
            &deposit_address.to_string(),
            Some(&db),
        )
        .await
        .expect("fetch stored")
        .expect("exists");

        assert_eq!(stored.deposit_address, deposit_address.to_string());
        assert_eq!(
            stored.recovery_taproot_address,
            recovery_taproot.address_with_prefix()
        );
        assert_eq!(stored.citrea_address, citrea_address.to_string());
        assert_eq!(stored.network, Network::Regtest);
        assert_eq!(stored.user_takes_after, config.user_takes_after);
    }
}
