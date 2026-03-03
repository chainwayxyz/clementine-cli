use alloy::{hex::FromHex, primitives::Address as AlloyAddress};
use anyhow::Context;
use async_trait::async_trait;
use bitcoin::{
    Address as BitcoinAddress, Amount, OutPoint, TxOut, secp256k1::Secp256k1, secp256k1::SecretKey,
};
use bitcoincore_rpc::RpcApi;
use citrea_e2e::clementine::client::clementine::{
    OptimisticWithdrawParams, Outpoint as ClementineOutpoint, Txid as ClementineTxid,
    WithdrawParams,
};
use citrea_e2e::{
    Result,
    bitcoin::DEFAULT_FINALITY_DEPTH,
    config::{
        AggregatorConfig, BatchProverConfig, BitcoinConfig, ClementineConfig,
        LightClientProverConfig, OperatorConfig, SequencerConfig, TestCaseConfig,
        TestCaseDockerConfig, VerifierConfig,
    },
    framework::TestFramework,
    test_case::{TestCase, TestCaseRunner},
    traits::NodeT,
};
use clementine_cli::wallet::{Purpose, create_encrypted_wallet};
use clementine_cli::{
    deposit::get_deposit_address,
    musig2::aggregate_public_keys_from_str,
    secure_types::{SecureKeypair, SecureString},
    sqlite_db::test_utils::fresh_db_with_test_name,
    wallet::{get_private_key_from_wallet, import_wallet_from_private_key},
    withdraw::{
        PrecomputedWithdrawalData, SafeWithdrawalParams, generate_withdrawal_signatures,
        scan_withdrawal, send_safe_withdrawal,
    },
};
use clementine_e2e_tests::bitcoin::BitcoinRpcExt;
use clementine_e2e_tests::citrea_bridge::DEFAULT_EVM_PRIVATE_KEY;
use clementine_e2e_tests::helper::{
    wait_for_balance_change_u256, wait_until_all_state_managers_synced,
};
use clementine_e2e_tests::{
    constants::TEST_EVM_ADDRESS_WITHDRAW,
    helper::{
        deposit_to_citrea, ensure_bridge_contract_deployed, force_sequencer_to_commit,
        get_citrea_balance_u256, get_default_bridge_params,
        regtest_bridge_cli_config_from_bitcoin_config, seeded_key, submit_deposit_and_move,
        wait_for_citrea,
    },
};
use reqwest::Url;
use std::{path::PathBuf, sync::Arc, time::Duration};
use tracing::info;

/// Helper function to create a new passphrase instance
fn create_test_passphrase() -> SecureString {
    SecureString::init_with(|| "test-passphrase".to_string())
}

/// Verifies withdrawal completion with comprehensive checks
/// Validates:
/// - Original dust UTXO is spent
/// - User's withdrawal address receives funds with confirmation
/// - Amount received is within expected range (accounting for fees)
async fn verify_withdrawal_completion(
    bitcoin_node: &bitcoincore_rpc::Client,
    user_withdrawal_address: &BitcoinAddress,
    expected_amount: Amount,
) -> anyhow::Result<()> {
    tracing::info!("User withdrawal address: {}", user_withdrawal_address);

    let withdrawal_utxos = bitcoin_node
        .list_unspent(None, None, Some(&[user_withdrawal_address]), None, None)
        .await
        .context("Failed to list unspent outputs for withdrawal address")?;

    anyhow::ensure!(
        !withdrawal_utxos.is_empty(),
        "No unspent outputs found at withdrawal address"
    );

    // Calculate total received and verify confirmations
    let mut total_received = Amount::ZERO;
    for utxo in &withdrawal_utxos {
        total_received += utxo.amount;

        anyhow::ensure!(
            utxo.confirmations > 0,
            "Withdrawal UTXO has 0 confirmations"
        );

        info!(
            "Withdrawal UTXO: {} sats, {} confirmations",
            utxo.amount.to_sat(),
            utxo.confirmations
        );
    }

    anyhow::ensure!(
        total_received == expected_amount,
        "User received {} BTC but expected {} BTC",
        total_received.to_btc(),
        expected_amount.to_btc()
    );

    info!(
        "Withdrawal verification passed! Received: {} BTC (expected: {} BTC)",
        total_received.to_btc(),
        expected_amount.to_btc()
    );

    Ok(())
}

/// Determines which withdrawal test variant to run.
#[derive(Debug, Clone, Copy)]
pub enum WithdrawalTestVariant {
    DefaultDust,
    DustAmount(Amount),
}

/// Initializes the Citrea/Clementine test framework and node topology.
pub struct WithdrawalTest {
    variant: WithdrawalTestVariant,
}

impl WithdrawalTest {
    pub fn new_default() -> Self {
        Self {
            variant: WithdrawalTestVariant::DefaultDust,
        }
    }

    pub fn new_with_dust(dust_amount: Amount) -> Self {
        Self {
            variant: WithdrawalTestVariant::DustAmount(dust_amount),
        }
    }
}

#[async_trait]
impl TestCase for WithdrawalTest {
    fn bitcoin_config() -> BitcoinConfig {
        BitcoinConfig {
            network: bitcoin::Network::Regtest,
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
            with_batch_prover: true,
            with_light_client_prover: true,
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
            bridge_initialize_params: get_default_bridge_params(
                Self::test_config().n_verifiers.into(),
            ),
            ..Default::default()
        }
    }

    fn batch_prover_config() -> BatchProverConfig {
        BatchProverConfig {
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

    fn clementine_aggregator_config() -> ClementineConfig<AggregatorConfig> {
        ClementineConfig {
            protocol_paramset: Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/regtest_paramset.toml"),
            ),
            ..Default::default()
        }
    }

    fn clementine_verifier_config(idx: u8) -> ClementineConfig<VerifierConfig> {
        ClementineConfig::<VerifierConfig> {
            protocol_paramset: Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/regtest_paramset.toml"),
            ),
            entity_config: VerifierConfig {
                idx,
                secret_key: SecretKey::from_slice(&seeded_key("verifier", idx))
                    .expect("failed to create secret key"),
            },
            ..Default::default()
        }
    }

    fn clementine_operator_config(idx: u8) -> ClementineConfig<OperatorConfig> {
        ClementineConfig::<OperatorConfig> {
            protocol_paramset: Some(
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("resources/regtest_paramset.toml"),
            ),
            entity_config: OperatorConfig {
                idx,
                secret_key: SecretKey::from_slice(&seeded_key("operator", idx))
                    .expect("failed to create secret key"),
                winternitz_secret_key: SecretKey::from_slice(&seeded_key(
                    "operator-winternitz",
                    idx,
                ))
                .expect("failed to create secret key"),
                ..Default::default()
            },
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

        let sequencer = Arc::new(sequencer);

        bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;

        let balance = bitcoin_node.get_balance(None, None).await?;

        if balance.to_btc() < 1.0 {
            Err(anyhow::anyhow!(
                "Bitcoin node has insufficient balance for withdrawal test"
            ))?;
        }

        info!("Waiting for Citrea to be ready...");

        wait_for_citrea(&sequencer).await?;

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
        let verifier_keys_hex: Vec<String> = verifier_keys.iter().map(hex::encode).collect();
        let verifier_keys_str = verifier_keys_hex.join(",");
        let aggregated_pubkey = aggregate_public_keys_from_str(&verifier_keys_str)
            .map_err(|e| anyhow::anyhow!("Failed to aggregate public keys: {}", e))?;

        info!("Aggregated public key: {}", aggregated_pubkey);

        // Check initial balance on Citrea
        let initial_balance =
            match get_citrea_balance_u256(&sequencer, TEST_EVM_ADDRESS_WITHDRAW).await {
                Ok(balance) => {
                    info!("Initial balance: {} wei", balance);
                    balance
                }
                Err(e) => {
                    info!("Error getting initial balance: {}", e);
                    return Err(e);
                }
            };

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

        if let WithdrawalTestVariant::DustAmount(dust_amount) = self.variant {
            info!("Setting dust UTXO amount to {} sats", dust_amount.to_sat());
            config.dust_utxo_amount = dust_amount;
        }

        // Update config with the aggregated public key
        config.aggregated_public_key = aggregated_pubkey;

        let test_evm_address = AlloyAddress::from_hex(TEST_EVM_ADDRESS_WITHDRAW)?;
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
            &sequencer,
            submitted.move_txid,
            &config,
        )
        .await?;

        // Wait for balance change on Citrea
        info!("Waiting for balance change on Citrea...");
        let final_balance =
            wait_for_balance_change_u256(&sequencer, TEST_EVM_ADDRESS_WITHDRAW, initial_balance)
                .await?;

        // Verify the deposit was successful
        assert!(
            final_balance > initial_balance,
            "No balance increase detected"
        );

        let balance_after_deposit = final_balance;

        // ========== WITHDRAWAL PHASE ==========
        info!("=== Starting withdrawal phase ===");

        // Create a withdrawal signer wallet from a known private key
        let signer_private_key_hex = SecureString::init_with(|| hex::encode([13u8; 32]));
        let withdrawal_signer_address = import_wallet_from_private_key(
            bitcoin::Network::Regtest,
            "e2e-withdrawal-signer",
            Purpose::Withdrawal,
            signer_private_key_hex,
            create_test_passphrase(),
            Some(&db_client),
        )
        .await?;

        let signer_secret_key = get_private_key_from_wallet(
            &withdrawal_signer_address,
            &create_test_passphrase(),
            Some(&db_client),
        )
        .await?;
        let signer_keypair = SecureKeypair::new(bitcoin::secp256k1::Keypair::from_secret_key(
            &Secp256k1::new(),
            signer_secret_key.as_ref_inner(),
        ));

        // Prepare the user's withdrawal destination address
        let user_withdrawal_address = bitcoin_node
            .get_new_address(Some("withdrawal_address"), None)
            .await?;
        let user_withdrawal_address = user_withdrawal_address.assume_checked();

        // Create a dust UTXO for withdrawal using the signer address
        let dust_txid = bitcoin_node
            .send_to_address(
                &withdrawal_signer_address.address,
                config.dust_utxo_amount,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await?;

        bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;

        let dust_tx = bitcoin_node.get_transaction(&dust_txid, None).await?;
        let dust_tx: bitcoin::Transaction = bitcoin::consensus::encode::deserialize(&dust_tx.hex)?;
        let dust_vout = dust_tx
            .output
            .iter()
            .enumerate()
            .find_map(|(index, output)| {
                BitcoinAddress::from_script(&output.script_pubkey, bitcoin::Network::Regtest)
                    .ok()
                    .filter(|addr| addr == &withdrawal_signer_address.address)
                    .and_then(|_| {
                        if output.value == config.dust_utxo_amount {
                            Some(index as u32)
                        } else {
                            None
                        }
                    })
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No output found for withdrawal signer address with dust amount {} sats",
                    config.dust_utxo_amount.to_sat()
                )
            })?;

        let withdrawal_outpoint = OutPoint {
            txid: dust_txid,
            vout: dust_vout,
        };

        info!("Withdrawal UTXO: {}", withdrawal_outpoint);

        // Use CLI withdrawal scan to validate UTXO discovery
        let available_utxos = scan_withdrawal(
            &withdrawal_signer_address,
            &user_withdrawal_address,
            &config,
        )
        .await?;
        let _scanned_outpoint = available_utxos
            .iter()
            .find(|utxo| utxo.txid == dust_txid && utxo.vout == dust_vout)
            .map(|utxo| OutPoint {
                txid: utxo.txid,
                vout: utxo.vout,
            })
            .ok_or_else(|| anyhow::anyhow!("Withdrawal UTXO not found via scan"))?;

        info!("Generating withdrawal signatures...");

        // Generate withdrawal signatures using CLI method
        let (optimistic_sig, _operator_sig) = generate_withdrawal_signatures(
            signer_keypair,
            &withdrawal_signer_address,
            &user_withdrawal_address,
            &withdrawal_outpoint,
            &config.optimistic_withdrawal_amount,
            &config.operator_withdrawal_amount,
            &config,
            Some(&db_client),
        )
        .await?;

        info!("Sending safeWithdraw transaction...");

        // Configure Citrea RPC URL for safe withdrawal
        config.citrea_rpc_url = Some(Url::parse(&format!(
            "http://{}:{}",
            sequencer.config.rollup.rpc.bind_host, sequencer.config.rollup.rpc.bind_port
        ))?);

        force_sequencer_to_commit(&sequencer).await?;
        bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;

        info!("Calling safe_withdraw...");

        // Send safeWithdraw transaction using CLI withdraw logic
        let safe_params = SafeWithdrawalParams {
            signer_address: withdrawal_signer_address.clone(),
            destination_address: user_withdrawal_address.clone(),
            withdrawal_outpoint,
            withdrawal_amount: config.optimistic_withdrawal_amount,
            signature: optimistic_sig,
            precomputed: None,
        };

        let l2_secret = SecureString::init_with(|| DEFAULT_EVM_PRIVATE_KEY.to_string());

        let mut tick = tokio::time::interval(Duration::from_secs(5));
        let mut wait = Box::pin(send_safe_withdrawal(safe_params, l2_secret, &config));

        let res = loop {
            tokio::select! {
                r = &mut wait => break r,
                _ = tick.tick() => {
                    let _ = force_sequencer_to_commit(&sequencer).await;
                }
            }
        };

        info!("Submitting safeWithdraw transaction to Citrea...");

        let receipt = res.map_err(|e| anyhow::anyhow!("Failed to send safeWithdraw: {}", e))?;

        if !receipt.status() {
            return Err(anyhow::anyhow!("safeWithdraw transaction reverted"));
        }

        info!("safeWithdraw transaction successful");

        // Force sequencer to produce blocks for syncing
        force_sequencer_to_commit(&sequencer).await?;

        let payout_txout = TxOut {
            value: config.optimistic_withdrawal_amount,
            script_pubkey: user_withdrawal_address.script_pubkey(),
        };

        let mut attempts = 0;
        let opt_payout = loop {
            attempts += 1;

            info!("Calling optimistic_payout to register the withdrawal UTXO on Citrea");
            let input_outpoint = withdrawal_outpoint;
            let res = clementine_cluster
                .aggregator
                .client
                .optimistic_payout(OptimisticWithdrawParams {
                    withdrawal: WithdrawParams {
                        withdrawal_id: 0,
                        input_signature: optimistic_sig.serialize().to_vec(),
                        input_outpoint: ClementineOutpoint {
                            txid: ClementineTxid {
                                txid: bitcoin::consensus::encode::serialize(&input_outpoint.txid),
                            }
                            .into(),
                            vout: input_outpoint.vout,
                        }
                        .into(),
                        output_script_pubkey: payout_txout.script_pubkey.to_bytes(),
                        output_amount: payout_txout.value.to_sat(),
                    }
                    .into(),
                    verification_signature: None,
                })
                .await
                .context("optimistic_payout failed");

            match res {
                Ok(res) => break res,
                Err(_) => {
                    if attempts > 120 {
                        res.context(format!(
                            "Timeout waiting for optimistic payout to succeed after {} attempts",
                            attempts
                        ))?;
                    }
                    bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;
                    wait_until_all_state_managers_synced(
                        bitcoin_node.client(),
                        &mut clementine_cluster.aggregator,
                    )
                    .await?;
                }
            }
        };

        info!("Optimistic payout successful");

        let opt_payout_tx = bitcoin::consensus::deserialize(&opt_payout.raw_tx)
            .context("Failed to deserialize optimistic payout transaction")?;

        bitcoin_node
            .client()
            .send_cpfp_tx(&opt_payout_tx, None)
            .await
            .context("Failed to send CPFP transaction")?;

        bitcoin_node
            .client()
            .mine_once_after_in_mempool(
                opt_payout_tx.compute_txid(),
                Some("Optimistic payout"),
                None,
            )
            .await
            .context("Failed to mine optimistic payout transaction")?;

        let tx_out = bitcoin_node
            .get_tx_out(
                &withdrawal_outpoint.txid,
                withdrawal_outpoint.vout,
                Some(false),
            )
            .await?;

        anyhow::ensure!(
            tx_out.is_none(),
            "Withdrawal UTXO still exists in Bitcoin node after payout"
        );

        anyhow::ensure!(
            opt_payout_tx.output[0].script_pubkey == payout_txout.script_pubkey,
            "Output script pubkey mismatch"
        );

        anyhow::ensure!(
            opt_payout_tx.output[0].value == payout_txout.value,
            "Output value mismatch"
        );

        // Verify balance on Citrea decreased
        let final_citrea_balance =
            get_citrea_balance_u256(&sequencer, TEST_EVM_ADDRESS_WITHDRAW).await?;

        info!(
            "Citrea balance after withdrawal: {}, balance after deposit: {}",
            final_citrea_balance, balance_after_deposit
        );

        assert!(
            final_citrea_balance < balance_after_deposit,
            "Citrea balance did not decrease after withdrawal"
        );

        // Verify withdrawal completion with comprehensive checks
        verify_withdrawal_completion(
            bitcoin_node.client(),
            &user_withdrawal_address,
            config.optimistic_withdrawal_amount,
        )
        .await?;

        info!("Withdrawal successful!",);

        Ok(())
    }
}

#[tokio::test]
async fn test_withdrawal() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();
    unsafe { std::env::set_var("RISC0_DEV_MODE", "1") };
    TestCaseRunner::new(WithdrawalTest::new_default())
        .run()
        .await
}

#[tokio::test]
async fn test_withdrawal_dust_1000() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();
    unsafe { std::env::set_var("RISC0_DEV_MODE", "1") };
    TestCaseRunner::new(WithdrawalTest::new_with_dust(Amount::from_sat(1000)))
        .run()
        .await
}

/// Tests that `send_safe_withdrawal` works with precomputed Bitcoin data,
/// proving no Bitcoin network GET calls are needed.
pub struct PrecomputedWithdrawalTest;

#[async_trait]
impl TestCase for PrecomputedWithdrawalTest {
    fn bitcoin_config() -> BitcoinConfig {
        WithdrawalTest::bitcoin_config()
    }

    fn test_config() -> TestCaseConfig {
        WithdrawalTest::test_config()
    }

    fn sequencer_config() -> SequencerConfig {
        WithdrawalTest::sequencer_config()
    }

    fn batch_prover_config() -> BatchProverConfig {
        WithdrawalTest::batch_prover_config()
    }

    fn light_client_prover_config() -> LightClientProverConfig {
        WithdrawalTest::light_client_prover_config()
    }

    fn scan_l1_start_height() -> Option<u64> {
        WithdrawalTest::scan_l1_start_height()
    }

    fn clementine_aggregator_config() -> ClementineConfig<AggregatorConfig> {
        WithdrawalTest::clementine_aggregator_config()
    }

    fn clementine_verifier_config(idx: u8) -> ClementineConfig<VerifierConfig> {
        WithdrawalTest::clementine_verifier_config(idx)
    }

    fn clementine_operator_config(idx: u8) -> ClementineConfig<OperatorConfig> {
        WithdrawalTest::clementine_operator_config(idx)
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
        let sequencer = Arc::new(sequencer);

        bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;

        wait_for_citrea(&sequencer).await?;
        ensure_bridge_contract_deployed(&sequencer).await?;

        let verifier_public_keys_response = clementine_cluster
            .aggregator
            .client
            .setup()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to setup Clementine aggregator: {}", e))?;

        let verifier_keys = verifier_public_keys_response.verifier_public_keys;
        let verifier_keys_hex: Vec<String> = verifier_keys.iter().map(hex::encode).collect();
        let verifier_keys_str = verifier_keys_hex.join(",");
        let aggregated_pubkey = aggregate_public_keys_from_str(&verifier_keys_str)
            .map_err(|e| anyhow::anyhow!("Failed to aggregate public keys: {}", e))?;

        let initial_balance =
            get_citrea_balance_u256(&sequencer, TEST_EVM_ADDRESS_WITHDRAW).await?;

        let db_client = fresh_db_with_test_name().await;
        let (recovery_address, _mnemonic) = create_encrypted_wallet(
            bitcoin::Network::Regtest,
            "e2e-recovery-wallet".to_string(),
            Purpose::Deposit,
            create_test_passphrase(),
            Some(&db_client),
        )
        .await?;

        let mut config = regtest_bridge_cli_config_from_bitcoin_config(&bitcoin_node.config)?;
        config.aggregated_public_key = aggregated_pubkey;

        let test_evm_address = AlloyAddress::from_hex(TEST_EVM_ADDRESS_WITHDRAW)?;
        let deposit_address = get_deposit_address(
            &test_evm_address,
            &recovery_address,
            &config,
            Some(&db_client),
        )
        .await?;

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
            &sequencer,
            submitted.move_txid,
            &config,
        )
        .await?;

        let final_balance =
            wait_for_balance_change_u256(&sequencer, TEST_EVM_ADDRESS_WITHDRAW, initial_balance)
                .await?;
        assert!(final_balance > initial_balance);
        let balance_after_deposit = final_balance;

        // ========== WITHDRAWAL PHASE ==========
        info!("=== Starting precomputed withdrawal test ===");

        let signer_private_key_hex = SecureString::init_with(|| hex::encode([13u8; 32]));
        let withdrawal_signer_address = import_wallet_from_private_key(
            bitcoin::Network::Regtest,
            "e2e-withdrawal-signer",
            Purpose::Withdrawal,
            signer_private_key_hex,
            create_test_passphrase(),
            Some(&db_client),
        )
        .await?;

        let signer_secret_key = get_private_key_from_wallet(
            &withdrawal_signer_address,
            &create_test_passphrase(),
            Some(&db_client),
        )
        .await?;
        let signer_keypair = SecureKeypair::new(bitcoin::secp256k1::Keypair::from_secret_key(
            &Secp256k1::new(),
            signer_secret_key.as_ref_inner(),
        ));

        let user_withdrawal_address = bitcoin_node
            .get_new_address(Some("withdrawal_address"), None)
            .await?;
        let user_withdrawal_address = user_withdrawal_address.assume_checked();

        let dust_txid = bitcoin_node
            .send_to_address(
                &withdrawal_signer_address.address,
                config.dust_utxo_amount,
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .await?;

        bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;

        let dust_tx = bitcoin_node.get_transaction(&dust_txid, None).await?;
        let dust_tx: bitcoin::Transaction = bitcoin::consensus::encode::deserialize(&dust_tx.hex)?;
        let dust_vout = dust_tx
            .output
            .iter()
            .enumerate()
            .find_map(|(index, output)| {
                BitcoinAddress::from_script(&output.script_pubkey, bitcoin::Network::Regtest)
                    .ok()
                    .filter(|addr| addr == &withdrawal_signer_address.address)
                    .and_then(|_| {
                        if output.value == config.dust_utxo_amount {
                            Some(index as u32)
                        } else {
                            None
                        }
                    })
            })
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "No output found for withdrawal signer address with dust amount {} sats",
                    config.dust_utxo_amount.to_sat()
                )
            })?;

        let withdrawal_outpoint = OutPoint {
            txid: dust_txid,
            vout: dust_vout,
        };

        let (optimistic_sig, _operator_sig) = generate_withdrawal_signatures(
            signer_keypair,
            &withdrawal_signer_address,
            &user_withdrawal_address,
            &withdrawal_outpoint,
            &config.optimistic_withdrawal_amount,
            &config.operator_withdrawal_amount,
            &config,
            Some(&db_client),
        )
        .await?;

        config.citrea_rpc_url = Some(Url::parse(&format!(
            "http://{}:{}",
            sequencer.config.rollup.rpc.bind_host, sequencer.config.rollup.rpc.bind_port
        ))?);

        force_sequencer_to_commit(&sequencer).await?;
        bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;

        // ========== EXTRACT PRECOMPUTED DATA FROM RPC ==========
        info!("Extracting precomputed data from regtest RPC...");

        let rpc = bitcoin_node.client();

        let raw_tx = rpc
            .get_raw_transaction(&dust_txid, None)
            .await
            .context("Failed to get raw transaction")?;
        let tx_hex = hex::encode(bitcoin::consensus::encode::serialize(&raw_tx));

        let tx_info = rpc
            .get_raw_transaction_info(&dust_txid, None)
            .await
            .context("Failed to get raw transaction info")?;
        let block_hash = tx_info
            .blockhash
            .ok_or_else(|| anyhow::anyhow!("Transaction not in a block"))?;

        let block = rpc
            .get_block(&block_hash)
            .await
            .context("Failed to get block")?;
        let block_txids: Vec<bitcoin::Txid> =
            block.txdata.iter().map(|tx| tx.compute_txid()).collect();

        let block_header_bytes = bitcoin::consensus::encode::serialize(&block.header);
        let block_header_hex = hex::encode(&block_header_bytes);

        let block_info = rpc
            .get_block_header_info(&block_hash)
            .await
            .context("Failed to get block header info")?;
        let block_height = block_info.height as u32;

        info!(
            "Precomputed data: tx_hex len={}, block_txids count={}, block_height={}",
            tx_hex.len(),
            block_txids.len(),
            block_height
        );

        // ========== SEND WITH PRECOMPUTED DATA (no Bitcoin API) ==========
        // Clear Bitcoin API config to prove no GET calls are made.
        let mut precomputed_config = config.clone();
        precomputed_config.bitcoin_config = None;
        precomputed_config.esplora_rest_api = None;

        let precomputed = PrecomputedWithdrawalData {
            tx_hex,
            block_txids,
            block_header_hex,
            block_height,
        };

        let safe_params = SafeWithdrawalParams {
            signer_address: withdrawal_signer_address.clone(),
            destination_address: user_withdrawal_address.clone(),
            withdrawal_outpoint,
            withdrawal_amount: precomputed_config.optimistic_withdrawal_amount,
            signature: optimistic_sig,
            precomputed: Some(precomputed),
        };

        let l2_secret = SecureString::init_with(|| DEFAULT_EVM_PRIVATE_KEY.to_string());

        info!("Calling send_safe_withdrawal with precomputed data (no Bitcoin API)...");

        let mut tick = tokio::time::interval(Duration::from_secs(5));
        let mut wait = Box::pin(send_safe_withdrawal(
            safe_params,
            l2_secret,
            &precomputed_config,
        ));

        let res = loop {
            tokio::select! {
                r = &mut wait => break r,
                _ = tick.tick() => {
                    let _ = force_sequencer_to_commit(&sequencer).await;
                }
            }
        };

        let receipt = res.map_err(|e| {
            anyhow::anyhow!("Failed to send safeWithdraw with precomputed data: {}", e)
        })?;

        anyhow::ensure!(
            receipt.status(),
            "safeWithdraw with precomputed data reverted"
        );

        info!("safeWithdraw with precomputed data successful!");

        force_sequencer_to_commit(&sequencer).await?;

        let payout_txout = TxOut {
            value: config.optimistic_withdrawal_amount,
            script_pubkey: user_withdrawal_address.script_pubkey(),
        };

        let mut attempts = 0;
        let opt_payout = loop {
            attempts += 1;

            let input_outpoint = withdrawal_outpoint;
            let res = clementine_cluster
                .aggregator
                .client
                .optimistic_payout(OptimisticWithdrawParams {
                    withdrawal: WithdrawParams {
                        withdrawal_id: 0,
                        input_signature: optimistic_sig.serialize().to_vec(),
                        input_outpoint: ClementineOutpoint {
                            txid: ClementineTxid {
                                txid: bitcoin::consensus::encode::serialize(&input_outpoint.txid),
                            }
                            .into(),
                            vout: input_outpoint.vout,
                        }
                        .into(),
                        output_script_pubkey: payout_txout.script_pubkey.to_bytes(),
                        output_amount: payout_txout.value.to_sat(),
                    }
                    .into(),
                    verification_signature: None,
                })
                .await
                .context("optimistic_payout failed");

            match res {
                Ok(res) => break res,
                Err(_) => {
                    if attempts > 120 {
                        res.context(format!(
                            "Timeout waiting for optimistic payout after {} attempts",
                            attempts
                        ))?;
                    }
                    bitcoin_node.generate(DEFAULT_FINALITY_DEPTH).await?;
                    wait_until_all_state_managers_synced(
                        bitcoin_node.client(),
                        &mut clementine_cluster.aggregator,
                    )
                    .await?;
                }
            }
        };

        let opt_payout_tx = bitcoin::consensus::deserialize(&opt_payout.raw_tx)
            .context("Failed to deserialize optimistic payout transaction")?;

        bitcoin_node
            .client()
            .send_cpfp_tx(&opt_payout_tx, None)
            .await
            .context("Failed to send CPFP transaction")?;

        bitcoin_node
            .client()
            .mine_once_after_in_mempool(
                opt_payout_tx.compute_txid(),
                Some("Optimistic payout"),
                None,
            )
            .await
            .context("Failed to mine optimistic payout transaction")?;

        anyhow::ensure!(
            opt_payout_tx.output[0].script_pubkey == payout_txout.script_pubkey,
            "Output script pubkey mismatch"
        );
        anyhow::ensure!(
            opt_payout_tx.output[0].value == payout_txout.value,
            "Output value mismatch"
        );

        let final_citrea_balance =
            get_citrea_balance_u256(&sequencer, TEST_EVM_ADDRESS_WITHDRAW).await?;
        assert!(
            final_citrea_balance < balance_after_deposit,
            "Citrea balance did not decrease after withdrawal"
        );

        verify_withdrawal_completion(
            bitcoin_node.client(),
            &user_withdrawal_address,
            config.optimistic_withdrawal_amount,
        )
        .await?;

        info!("Precomputed withdrawal test successful!");

        Ok(())
    }
}

#[tokio::test]
async fn test_withdrawal_with_precomputed_data() -> Result<()> {
    let _ = tracing_subscriber::fmt().with_env_filter("info").try_init();
    unsafe { std::env::set_var("RISC0_DEV_MODE", "1") };
    TestCaseRunner::new(PrecomputedWithdrawalTest)
        .run()
        .await
}
