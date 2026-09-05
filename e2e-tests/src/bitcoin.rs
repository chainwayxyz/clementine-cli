use anyhow::{Context, Result, anyhow};
use async_trait::async_trait;
use bitcoin::hashes::Hash as _;
use bitcoin::secp256k1::XOnlyPublicKey;
use bitcoin::{
    BlockHash, OutPoint, ScriptBuf, Sequence, TxIn, TxOut, Txid, block::Block,
    blockdata::script::Instruction, consensus, transaction::Version,
};
use bitcoincore_rpc::PackageTransactionResult;
use bitcoincore_rpc::{
    RawTx, RpcApi,
    json::{SigHashType, SignRawTransactionInput},
};
use tracing::debug;

#[async_trait]
pub trait BitcoinRpcExt: RpcApi {
    async fn get_txout_from_outpoint(&self, outpoint: &OutPoint) -> Result<TxOut> {
        let tx = self
            .get_raw_transaction(&outpoint.txid, None)
            .await
            .context("Failed to get transaction")?;
        let txout = tx
            .output
            .get(outpoint.vout as usize)
            .ok_or(anyhow!(
                "No output at index {} for txid {}",
                outpoint.vout,
                outpoint.txid
            ))?
            .to_owned();

        Ok(txout)
    }

    // Override deprecated generate method.
    // Uses or lazy init gen_addr and forward to `generate_to_address`
    async fn generate_blocks(
        &self,
        block_num: u64,
        _maxtries: Option<u64>,
    ) -> bitcoincore_rpc::Result<Vec<bitcoin::BlockHash>> {
        let addr = self
            .get_new_address(None, Some(bitcoincore_rpc::json::AddressType::Bech32m))
            .await
            .expect("Failed to generate address")
            .assume_checked();

        self.generate_to_address(block_num, &addr).await
    }

    async fn send_cpfp_tx(
        &self,
        move_tx: &bitcoin::Transaction,
        fee_rate: Option<f64>,
    ) -> Result<()> {
        // Find P2A anchor output (script: 51024e73)
        let p2a_vout = move_tx
            .output
            .iter()
            .position(|output| {
                output.script_pubkey == ScriptBuf::from_hex("51024e73").expect("valid script")
            })
            .expect("P2A anchor output not found in move transaction");

        debug!("Found P2A anchor output at vout: {}", p2a_vout);

        let rpc = self;

        let temp_address = rpc
            .get_new_address(None, None)
            .await
            .expect("Failed to get new address");

        let fee_rate_sat_vb = fee_rate.unwrap_or(10.0) as u64;

        // Calculate package fee requirements
        let parent_weight = move_tx.weight();
        let estimated_child_weight = bitcoin::Weight::from_wu(500);
        let total_weight = parent_weight + estimated_child_weight;
        let required_fee_sats = (total_weight.to_wu() as f64 * fee_rate_sat_vb as f64 / 4.0) as u64;
        let required_fee = bitcoin::Amount::from_sat(required_fee_sats + 10000);

        tracing::debug!(
            "Parent weight: {}, estimated total: {}, required fee: {} sats",
            parent_weight,
            total_weight,
            required_fee.to_sat()
        );

        // Generate blocks to ensure fresh UTXOs for fees
        debug!("Generating blocks to create fresh UTXOs for CPFP");
        let blocks_generated = rpc
            .generate_to_address(1, &temp_address.clone().assume_checked())
            .await
            .expect("Failed to generate blocks");
        debug!("Generated {} block(s)", blocks_generated.len());

        let unspent = rpc
            .list_unspent(None, None, None, None, None)
            .await
            .expect("Failed to list unspent outputs");

        if unspent.is_empty() {
            debug!("No unspent outputs available for fee payment");
            return Err(anyhow!("No unspent outputs available for fee payment"));
        }

        let fee_payer_utxo = unspent.last().expect("Checked unspent is not empty");
        debug!(
            "Using UTXO {} for fees: {}",
            fee_payer_utxo.txid, fee_payer_utxo.amount
        );

        let child_input = TxIn {
            previous_output: OutPoint {
                txid: move_tx.compute_txid(),
                vout: p2a_vout as u32,
            },
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: bitcoin::Witness::new(),
        };

        let fee_payer_input = TxIn {
            previous_output: OutPoint {
                txid: fee_payer_utxo.txid,
                vout: fee_payer_utxo.vout,
            },
            script_sig: bitcoin::ScriptBuf::new(),
            sequence: Sequence::ENABLE_RBF_NO_LOCKTIME,
            witness: bitcoin::Witness::new(),
        };

        let total_input_value = bitcoin::Amount::from_sat(240) + fee_payer_utxo.amount;
        let change_amount = total_input_value
            .checked_sub(required_fee)
            .expect("Insufficient funds for required fee");

        let child_output = TxOut {
            value: change_amount,
            script_pubkey: temp_address.assume_checked().script_pubkey(),
        };

        let child_tx = bitcoin::Transaction {
            version: Version::non_standard(3),
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![child_input, fee_payer_input],
            output: vec![child_output],
        };

        debug!("Child transaction created: {}", child_tx.compute_txid());

        let signed_child_tx = rpc
            .sign_raw_transaction_with_wallet(
                &child_tx,
                Some(&[SignRawTransactionInput {
                    amount: Some(move_tx.output[p2a_vout].value),
                    script_pub_key: move_tx.output[p2a_vout].script_pubkey.clone(),
                    txid: move_tx.compute_txid(),
                    vout: p2a_vout as u32,
                    redeem_script: None,
                }]),
                Some(SigHashType::from(bitcoin::sighash::EcdsaSighashType::All)),
            )
            .await?;

        let signed_child_tx_parsed: bitcoin::Transaction =
            consensus::deserialize(&signed_child_tx.hex)?;

        debug!(
            "Signed child transaction: {}",
            signed_child_tx_parsed.compute_txid()
        );

        // Submit CPFP package
        let package = vec![move_tx, &signed_child_tx_parsed];
        debug!("Submitting CPFP package");

        match rpc
            .submit_package(&package, Some(bitcoin::Amount::ZERO), None)
            .await
        {
            Ok(result) => {
                // If tx_results is empty, it means the txs were already accepted by the network.
                if result.tx_results.is_empty() {
                    return Ok(());
                }

                let mut failed = false;
                for (txid, result) in &result.tx_results {
                    if let PackageTransactionResult::Failure { error, .. } = result {
                        tracing::error!("Error submitting package: {:?}, txid: {}", error, txid);
                        failed = true;
                    }
                }

                if failed {
                    tracing::warn!(
                        "Failed to submit CPFP package, package: {:?}",
                        package
                            .iter()
                            .map(|tx| hex::encode(bitcoin::consensus::serialize(tx)))
                            .collect::<Vec<_>>()
                    );
                    return Err(anyhow!("Failed to submit CPFP package"));
                }

                debug!("CPFP package submitted successfully");
                debug!("Package result: {:?}", result);
                debug!("Move transaction TXID: {}", move_tx.compute_txid());
                debug!("Child transaction TXID: {}", child_tx.compute_txid());
                Ok(())
            }
            Err(e) => {
                tracing::debug!("Failed to submit CPFP package: {}", e);
                tracing::debug!("Manual submission options:");
                tracing::debug!("Parent tx: {}", move_tx.raw_hex());
                tracing::debug!(
                    "Child tx: {}",
                    hex::encode(bitcoin::consensus::serialize(&child_tx))
                );
                Err(e).context("Failed to submit CPFP package")
            }
        }
    }

    /// Wait for a transaction to be in the mempool and then mines a block to make
    /// sure that it is included in the next block.
    ///
    /// # Parameters
    ///
    /// - `rpc`: The RPC client to use.
    /// - `txid`: The txid to wait for.
    /// - `tx_name`: The name of the transaction to wait for.
    /// - `timeout`: The timeout in seconds.
    async fn mine_once_after_in_mempool(
        &self,
        txid: Txid,
        tx_name: Option<&str>,
        timeout: Option<u64>,
    ) -> Result<usize> {
        let timeout = timeout.unwrap_or(60);
        let start = std::time::Instant::now();
        let tx_name = tx_name.unwrap_or("Unnamed tx");

        if self
            .get_transaction(&txid, None)
            .await
            .is_ok_and(|tx| tx.info.blockhash.is_some())
        {
            return Err(anyhow!("{} is already mined", tx_name));
        }

        loop {
            if start.elapsed() > std::time::Duration::from_secs(timeout) {
                return Err(anyhow!(
                    "{} didn't hit mempool within {} seconds",
                    tx_name,
                    timeout
                ));
            }

            if self.get_mempool_entry(&txid).await.is_ok() {
                break;
            };

            // mine if there are some txs in mempool
            if self.get_mempool_info().await?.size > 0 {
                self.generate_blocks(1, None).await?;
            }

            tracing::info!("Waiting for {} transaction to hit mempool...", tx_name);
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }

        self.generate_blocks(1, None).await?;

        let tx: bitcoincore_rpc::json::GetRawTransactionResult = self
            .get_raw_transaction_info(&txid, None)
            .await
            .map_err(|e| {
                anyhow!(
            "{} did not land onchain after in mempool and mining 1 block and rpc gave error: {}",
            tx_name,
            e
        )
            })?;

        if tx.blockhash.is_none() {
            tracing::error!(
                "{} did not land onchain after in mempool and mining 1 block",
                tx_name
            );

            return Err(anyhow!(
                "{} did not land onchain after in mempool and mining 1 block",
                tx_name
            ));
        }

        let tx_block_height = self
            .get_block_info(&tx.blockhash.unwrap())
            .await
            .context("Failed to get block info")?;

        Ok(tx_block_height.height)
    }

    /// Waits for a kickoff transaction to appear on-chain by scanning blocks for an OP_RETURN
    /// output that matches `move_txid.to_byte_array() + operator_xonly_pk.serialize()`.
    ///
    /// - `move_txid`: The move-to-vault txid for the related deposit
    /// - `operator_candidates`: List of operator x-only public keys to match against
    /// - `start_height`: Optional start height to begin scanning (inclusive). If None, starts from current tip + 1
    /// - `timeout`: How long to wait before giving up
    async fn wait_for_kickoff_tx(
        &self,
        move_txid: Txid,
        operator_candidates: Vec<XOnlyPublicKey>,
        start_height: Option<u64>,
        timeout: std::time::Duration,
    ) -> Result<Txid> {
        let start_time = std::time::Instant::now();

        // Pre-compute payloads for all operator candidates
        let target_payloads: Vec<[u8; 64]> = operator_candidates
            .into_iter()
            .map(|xonly| {
                let mut buf = [0u8; 64];
                let tx_bytes = move_txid.to_byte_array();
                buf[0..32].copy_from_slice(&tx_bytes);
                buf[32..64].copy_from_slice(&xonly.serialize());
                buf
            })
            .collect();

        // Determine initial scanning height
        let mut next_height = match start_height {
            Some(h) => h,
            None => self.get_blockchain_info().await?.blocks + 1,
        };

        loop {
            if start_time.elapsed() > timeout {
                return Err(anyhow!("Timeout waiting for kickoff transaction"));
            }

            // Scan up to current tip
            let tip = self.get_blockchain_info().await?.blocks;
            while next_height <= tip {
                if let Some(found) = self
                    .find_kickoff_in_block_by_height(next_height, &target_payloads)
                    .await?
                {
                    return Ok(found);
                }
                next_height += 1;
            }

            // Sleep briefly before re-checking tip
            tokio::time::sleep(std::time::Duration::from_millis(3000)).await;
        }
    }

    /// Scan a single block (by height) to locate a kickoff transaction by matching OP_RETURN data
    async fn find_kickoff_in_block_by_height(
        &self,
        height: u64,
        target_payloads: &[[u8; 64]],
    ) -> Result<Option<Txid>> {
        let block_hash = self.get_block_hash(height).await?;
        self.find_kickoff_in_block(&block_hash, target_payloads)
            .await
    }

    /// Scan a single block (by hash) to locate a kickoff transaction by matching OP_RETURN data
    async fn find_kickoff_in_block(
        &self,
        block_hash: &BlockHash,
        target_payloads: &[[u8; 64]],
    ) -> Result<Option<Txid>> {
        let block: Block = self.get_block(block_hash).await?;
        for tx in block.txdata {
            for out in &tx.output {
                if !out.script_pubkey.is_op_return() {
                    continue;
                }

                // Expect OP_RETURN <data>
                if let Some(Ok(Instruction::PushBytes(data))) =
                    out.script_pubkey.instructions().last()
                {
                    let bytes = data.as_bytes();
                    if bytes.len() == 64 && target_payloads.iter().any(|p| p.as_slice() == bytes) {
                        return Ok(Some(tx.compute_txid()));
                    }
                }
            }
        }
        Ok(None)
    }

    /// Returns true if the given outpoint appears to be spent (no unspent txout available)
    async fn is_utxo_spent(&self, outpoint: &OutPoint) -> Result<bool> {
        let res = self
            .get_tx_out(&outpoint.txid, outpoint.vout, Some(false))
            .await?;
        Ok(res.is_none())
    }

    /// Wait until an outpoint is spent, or time out
    async fn ensure_outpoint_spent(
        &self,
        outpoint: &OutPoint,
        timeout: Option<std::time::Duration>,
    ) -> Result<()> {
        let start = std::time::Instant::now();
        let timeout = timeout.unwrap_or_else(|| std::time::Duration::from_secs(300));
        loop {
            if self.is_utxo_spent(outpoint).await? {
                return Ok(());
            }
            if start.elapsed() > timeout {
                return Err(anyhow!(
                    "Timeout waiting for outpoint {}:{} to be spent",
                    outpoint.txid,
                    outpoint.vout
                ));
            }
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
    }
}

impl<T: RpcApi> BitcoinRpcExt for T {}
