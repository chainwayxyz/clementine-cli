//! Mock server utilities for integration testing
//!
//! This module provides helpers for mocking external API calls using wiremock.

pub mod bridge_backend;
pub mod mempool_api;

use wiremock::MockServer;

/// Setup a mock server and return it
pub async fn start_mock_server() -> MockServer {
    MockServer::start().await
}
