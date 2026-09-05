use alloy::{hex::FromHex, primitives::Address};
use async_trait::async_trait;
use bitcoin::{Amount, OutPoint, secp256k1::Secp256k1};
use citrea_e2e::{
    Result,
    bitcoin::DEFAULT_FINALITY_DEPTH,
    config::{
        BitcoinConfig, ClementineConfig, LightClientProverConfig, OperatorConfig, SequencerConfig,
        TestCaseConfig, TestCaseDockerConfig, VerifierConfig,
    },
    framework::TestFramework,
    test_case::{TestCase, TestCaseRunner},
    traits::NodeT,
};
use tracing::info;

use bitcoincore_rpc::RpcApi;
use clementine_cli::{
    broadcast_recovery_tx,
    deposit::{
        CitreaAddress, RecoveryTxParams, VerifyRecoveryTxParams, create_signed_recovery_tx,
        get_deposit_address, verify_recovery_tx,
    },
    musig2::aggregate_public_keys_from_str,
    secure_types::{SecureKeypair, SecureString},
    sqlite_db::test_utils::fresh_db_with_test_name,
    wallet::get_private_key_from_wallet,
};

use clementine_cli::wallet::{Purpose, create_encrypted_wallet};
use clementine_e2e_tests::{
    constants::TEST_EVM_ADDRESS_DEPOSIT,
    helper::{
        ensure_bridge_contract_deployed, get_default_bridge_params, parse_evm_address_to_20,
        regtest_bridge_cli_config_from_bitcoin_config, wait_for_citrea,
    },
};
/// Initializes the Citrea/Clementine test framework and node topology.
pub struct DepositRecoveryTest;

fn create_test_passphrase() -> SecureString {
    SecureString::init_with(|| "test-passphrase".to_string())
}

#[async_trait]
impl TestCase for DepositRecoveryTest {
    fn bitcoin_config() -> BitcoinConfig {
        BitcoinConfig {
            extra_args: vec![
                "-txindex=1",
                "-fallbackfee=0.000001",
                "-rpcallowip=0.0.0.0/0",
                "-dustrelayfee=0",
            ],
            ..Default::default()
        }
    }

    fn test_config() -> TestCaseConfig {
        TestCaseConfig {
            with_sequencer: true,
            with_full_node: true,
            with_clementine: true,
            n_verifiers: 2,
            n_operators: 2,
            docker: TestCaseDockerConfig {
                bitcoin: true,
                citrea: true,
                clementine: true,
            },
            ..Default::default()
        }
    }

    fn sequencer_config() -> SequencerConfig {
        SequencerConfig {
            test_mode: true,
            bridge_initialize_params: get_default_bridge_params(
                Self::test_config().n_verifiers.into(),
            ),
            ..Default::default()
        }
    }

    fn light_client_prover_config() -> LightClientProverConfig {
        LightClientProverConfig {
            enable_recovery: false,
            initial_da_height: 175,
            ..Default::default()
        }
    }

    fn scan_l1_start_height() -> Option<u64> {
        Some(175)
    }

    fn clementine_verifier_config(idx: u8) -> ClementineConfig<VerifierConfig> {
        ClementineConfig::<VerifierConfig> {
            entity_config: VerifierConfig::default_for_idx(idx),
            ..Default::default()
        }
    }

    fn clementine_operator_config(idx: u8) -> ClementineConfig<OperatorConfig> {
        ClementineConfig::<OperatorConfig> {
            entity_config: OperatorConfig::default_for_idx(idx),
            ..Default::default()
        }
    }

    async fn run_test(&mut self, framework: &mut TestFramework) -> Result<()> {
        let bitcoin_node = framework
            .bitcoin_nodes
            .get(0)
            .expect("No Bitcoin node found");

        let clementine_cluster = framework
            .clementine_nodes
            .as_mut()
            .expect("No Clementine nodes found");

        let sequencer = framework.sequencer.as_mut().expect("No Sequencer found");

        bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;

        let balance = bitcoin_node.get_balance(None, None).await?;

        if balance.to_btc() < 1.0 {
            Err(anyhow::anyhow!(
                "Bitcoin node has insufficient balance for deposit test"
            ))?;
        }

        info!("Waiting for Citrea to be ready...");

        wait_for_citrea(sequencer).await?;

        ensure_bridge_contract_deployed(sequencer).await?;

        info!("Setting up Clementine aggregator...");

        let verifier_public_keys_response = clementine_cluster
            .aggregator
            .client
            .setup()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to setup Clementine aggregator: {}", e))?;

        let verifier_keys = verifier_public_keys_response.verifier_public_keys;

        info!(
            "Clementine setup complete with {} verifiers",
            verifier_keys.len()
        );

        // Aggregate the verifier public keys
        let verifier_keys_hex: Vec<String> = verifier_keys.iter().map(hex::encode).collect();
        let verifier_keys_str = verifier_keys_hex.join(",");
        let aggregated_pubkey = aggregate_public_keys_from_str(&verifier_keys_str)
            .map_err(|e| anyhow::anyhow!("Failed to aggregate public keys: {}", e))?;

        info!("Aggregated public key: {}", aggregated_pubkey);

        let db_client = fresh_db_with_test_name().await;
        let (recovery_address, _mnemonic) = create_encrypted_wallet(
            bitcoin::Network::Regtest,
            "e2e-recovery-wallet".to_string(),
            Purpose::Deposit,
            create_test_passphrase(),
            Some(&db_client),
        )
        .await?;

        // Parse EVM address
        info!("Generating deposit address...");

        let mut config = regtest_bridge_cli_config_from_bitcoin_config(&bitcoin_node.config)?;

        // Update config with the aggregated public key
        config.aggregated_public_key = aggregated_pubkey;

        let test_evm_address = Address::from_hex(TEST_EVM_ADDRESS_DEPOSIT)?;
        let deposit_address = get_deposit_address(
            &test_evm_address,
            &recovery_address,
            &config,
            Some(&db_client),
        )
        .await?;

        // Submit a wrong deposit amount
        let deposit_amount = Amount::from_btc(9.0)?;
        let deposit_txid = bitcoin_node
            .send_to_address(
                &deposit_address.deposit_address,
                deposit_amount,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await?;

        bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;

        let deposit_tx = bitcoin_node.get_transaction(&deposit_txid, None).await?;

        anyhow::ensure!(
            deposit_tx.info.blockhash.is_some(),
            "Deposit tx not yet confirmed"
        );

        // Find the output index for the deposit address
        let tx_bytes: bitcoin::Transaction =
            bitcoin::consensus::encode::deserialize(&deposit_tx.hex)?;
        let mut deposit_vout = None;
        for (index, output) in tx_bytes.output.iter().enumerate() {
            if let Ok(address) =
                bitcoin::Address::from_script(&output.script_pubkey, bitcoin::Network::Regtest)
                && address == deposit_address.deposit_address
            {
                deposit_vout = Some(index as u32);
                break;
            }
        }

        let vout =
            deposit_vout.ok_or_else(|| anyhow::anyhow!("No output found to deposit address"))?;
        let outpoint = OutPoint {
            txid: deposit_txid,
            vout,
        };

        // Wait until recovery is allowed (after 200 blocks)
        bitcoin_node.generate(config.user_takes_after).await?;

        // Create a recovery transaction to refund the wrong deposit amount
        let evm_addr_bytes = parse_evm_address_to_20(TEST_EVM_ADDRESS_DEPOSIT)?;
        let citrea_addr = CitreaAddress::from(evm_addr_bytes);

        let recovery_secret = get_private_key_from_wallet(
            &recovery_address,
            &create_test_passphrase(),
            Some(&db_client),
        )
        .await?;
        let recovery_keypair = SecureKeypair::new(bitcoin::secp256k1::Keypair::from_secret_key(
            &Secp256k1::new(),
            recovery_secret.as_ref_inner(),
        ));

        let destination_addr = bitcoin_node.client().get_new_address(None, None).await?;

        let destination_addr = destination_addr.assume_checked();

        let fee_rate = 1u64;

        let recovery_tx = create_signed_recovery_tx(
            RecoveryTxParams {
                citrea_addr,
                recovery_taproot_address: recovery_address.clone(),
                outpoint,
                destination_addr: destination_addr.clone(),
                fee_rate: Some(fee_rate),
                amount: Some(deposit_amount.to_btc()),
            },
            &config,
            recovery_keypair,
            Some(&db_client),
        )
        .await?;

        let (recovery_txid, recovery_to, recovery_amount) = verify_recovery_tx(
            VerifyRecoveryTxParams {
                recovery_tx: recovery_tx.clone(),
                citrea_address: citrea_addr,
                recovery_taproot_address: recovery_address.clone(),
                amount: Some(deposit_amount.to_btc()),
            },
            &config,
        )?;

        let recovery_tx_weight = recovery_tx.weight();
        let recovery_tx_wu = recovery_tx_weight.to_wu();
        let expected_fee_sat = fee_rate.saturating_mul(recovery_tx_wu).div_ceil(4);
        let expected_received_sat = deposit_amount.to_sat().saturating_sub(expected_fee_sat);

        info!(
            "Recovery tx verified: {} -> {} ({} sats), weight: {}, fee: {} sats",
            recovery_txid,
            recovery_to,
            recovery_amount.to_sat(),
            recovery_tx_wu,
            expected_fee_sat
        );

        let dest_initial_balance = bitcoin_node
            .get_received_by_address(&destination_addr, Some(0))
            .await?;

        let raw_recovery_tx = hex::encode(bitcoin::consensus::encode::serialize(&recovery_tx));
        let broadcast_txid = broadcast_recovery_tx(&config, raw_recovery_tx).await?;

        info!("Recovery tx broadcasted: {}", broadcast_txid);

        bitcoin_node.generate(10).await?;

        // check if the funds have arrived at the destination address
        let dest_balance = bitcoin_node
            .get_received_by_address(&destination_addr, Some(0))
            .await?;

        info!(
            "Destination address balance: initial {}, final {}",
            dest_initial_balance, dest_balance
        );

        // fetch broadcasted tx details
        let tx = bitcoin_node
            .get_transaction(&broadcast_txid, None)
            .await?
            .transaction()
            .expect("Tx not found");

        info!("Broadcasted recovery tx details: {:?}", tx);

        let delta_sat = dest_balance
            .to_sat()
            .saturating_sub(dest_initial_balance.to_sat());
        anyhow::ensure!(
            delta_sat == expected_received_sat,
            "Destination received {} sats but expected {} sats",
            delta_sat,
            expected_received_sat
        );

        info!("Deposit recovery successful!");

        Ok(())
    }
}

#[tokio::test]
async fn test_deposit_recovery() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();
    TestCaseRunner::new(DepositRecoveryTest).run().await
}
