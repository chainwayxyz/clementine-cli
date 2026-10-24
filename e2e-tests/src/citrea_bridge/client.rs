use alloy::{
    network::EthereumWallet,
    primitives::Address,
    providers::{
        ProviderBuilder, RootProvider,
        fillers::{
            BlobGasFiller, ChainIdFiller, FillProvider, GasFiller, JoinFill, NonceFiller,
            WalletFiller,
        },
    },
    signers::{Signer, local::PrivateKeySigner},
};
use anyhow::{Result, anyhow};
use std::str::FromStr;

use crate::citrea_bridge::BridgeContract::BridgeContractInstance;

use super::constants::*;
use super::contract::BridgeContract;

// Type alias for the Bridge contract with provider
type BridgeContractWithProvider = BridgeContractInstance<
    FillProvider<
        JoinFill<
            JoinFill<
                alloy::providers::Identity,
                JoinFill<GasFiller, JoinFill<BlobGasFiller, JoinFill<NonceFiller, ChainIdFiller>>>,
            >,
            WalletFiller<EthereumWallet>,
        >,
        RootProvider,
    >,
>;

/// Client for interacting with Citrea's Bridge contract
pub struct CitreaBridgeClient {
    contract: BridgeContractWithProvider,
    wallet_address: Address,
}

impl CitreaBridgeClient {
    /// Create a new CitreaBridgeClient
    ///
    /// # Arguments
    /// * `citrea_rpc_url` - The RPC URL for Citrea
    /// * `chain_id` - The chain ID (default: 5655 for Citrea)
    /// * `private_key` - Optional private key, if None uses the default test key
    pub fn new(
        citrea_rpc_url: String,
        chain_id: Option<u64>,
        private_key: Option<&str>,
    ) -> Result<Self> {
        let private_key = private_key.unwrap_or(DEFAULT_EVM_PRIVATE_KEY);

        let signer = PrivateKeySigner::from_str(private_key)
            .map_err(|e| anyhow!("Failed to create signer: {}", e))?;

        let chain_id = chain_id.unwrap_or(DEFAULT_CITREA_CHAIN_ID);
        let signer_with_chain = signer.with_chain_id(Some(chain_id));
        let wallet_address = signer_with_chain.address();
        let wallet = EthereumWallet::from(signer_with_chain);

        let citrea_rpc_url = citrea_rpc_url
            .parse()
            .map_err(|e| anyhow!("Failed to parse URL: {}", e))?;

        let provider = ProviderBuilder::new()
            .wallet(wallet)
            .connect_http(citrea_rpc_url);

        let contract = BridgeContract::new(
            BRIDGE_CONTRACT_ADDRESS
                .parse()
                .map_err(|e| anyhow!("Failed to parse bridge address: {}", e))?,
            provider,
        );

        Ok(Self {
            contract,
            wallet_address,
        })
    }

    /// Create a new CitreaBridgeClient from sequencer config
    pub fn from_sequencer_config(
        sequencer: &citrea_e2e::node::Node<citrea_e2e::config::SequencerConfig>,
        private_key: Option<&str>,
    ) -> Result<Self> {
        let citrea_rpc_url = format!(
            "http://{}:{}",
            sequencer.config.rollup.rpc.bind_host, sequencer.config.rollup.rpc.bind_port
        );

        Self::new(citrea_rpc_url, None, private_key)
    }

    /// Get the wallet address
    pub fn wallet_address(&self) -> Address {
        self.wallet_address
    }

    pub async fn get_deposit_amount(&self) -> Result<u64> {
        let result = self
            .contract
            .depositAmount()
            .call()
            .await
            .map_err(|e| anyhow!("Failed to get deposit amount: {}", e))?;

        Ok(result.to::<u64>())
    }
}
