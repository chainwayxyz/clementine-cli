use alloy::{hex::FromHex, primitives::Address};
use async_trait::async_trait;
use bitcoin::Amount;
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
    deposit::get_deposit_address, musig2::aggregate_public_keys_from_str,
    secure_types::SecureString, sqlite_db::test_utils::fresh_db_with_test_name,
};

use clementine_cli::wallet::{Purpose, create_encrypted_wallet};
use clementine_e2e_tests::{
    constants::TEST_EVM_ADDRESS_DEPOSIT,
    helper::{
        deposit_to_citrea, ensure_bridge_contract_deployed, get_citrea_balance,
        get_default_bridge_params, regtest_bridge_cli_config_from_bitcoin_config,
        submit_deposit_and_move, wait_for_balance_change, wait_for_citrea,
    },
};
/// Initializes the Citrea/Clementine test framework and node topology.
pub struct DepositTest;

#[async_trait]
impl TestCase for DepositTest {
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

        ensure_bridge_contract_deployed(&sequencer).await?;

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
        let verifier_keys_hex: Vec<String> =
            verifier_keys.iter().map(|key| hex::encode(key)).collect();
        let verifier_keys_str = verifier_keys_hex.join(",");
        let aggregated_pubkey = aggregate_public_keys_from_str(&verifier_keys_str)
            .map_err(|e| anyhow::anyhow!("Failed to aggregate public keys: {}", e))?;

        info!("Aggregated public key: {}", aggregated_pubkey);

        // Check initial balance on Citrea
        let initial_balance = get_citrea_balance(sequencer, TEST_EVM_ADDRESS_DEPOSIT).await?;

        let passphrase = SecureString::init_with(|| "test-passphrase".to_string());

        let db_client = fresh_db_with_test_name().await;
        let (recovery_address, _mnemonic) = create_encrypted_wallet(
            bitcoin::Network::Regtest,
            "e2e-recovery-wallet".to_string(),
            Purpose::Deposit,
            passphrase,
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

        // Submit deposit and move transaction using utility
        let deposit_amount = Amount::from_btc(10.0)?;
        let submitted = submit_deposit_and_move(
            bitcoin_node,
            &mut clementine_cluster.aggregator.client,
            &deposit_address.deposit_address,
            deposit_amount,
            test_evm_address.0.into(),
            recovery_address.address_without_prefix(),
        )
        .await?;

        deposit_to_citrea(
            bitcoin_node.client(),
            sequencer,
            submitted.move_txid,
            &config,
        )
        .await?;

        // Wait for balance change on Citrea
        info!("Waiting for balance change on Citrea...");
        let final_balance =
            wait_for_balance_change(sequencer, TEST_EVM_ADDRESS_DEPOSIT, initial_balance).await?;

        // Verify the deposit was successful
        assert!(
            final_balance > initial_balance,
            "No balance increase detected"
        );

        info!(
            "Deposit successful! Initial balance: {}, Final balance: {}",
            initial_balance, final_balance
        );

        Ok(())
    }
}

#[tokio::test]
async fn test_deposit() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();
    TestCaseRunner::new(DepositTest).run().await
}
