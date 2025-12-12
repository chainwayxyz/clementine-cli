//! Integration tests for multi-module interactions
//!
//! These tests verify that different components of the wallet system
//! work correctly together in end-to-end workflows.

pub mod backend_status_async;
pub mod deposit_async;
pub mod deposit_recovery;
pub mod deposit_workflow_scenarios;
pub mod error_handling_scenarios;
pub mod mempool_api_async;
pub mod wallet_backup;
pub mod wallet_creation;
pub mod wallet_import;
pub mod wallet_import_mnemonic;
pub mod wallet_workflow_scenarios;
pub mod withdrawal_signing;
pub mod withdrawal_workflow_scenarios;
