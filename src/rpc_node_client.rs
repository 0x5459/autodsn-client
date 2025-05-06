use std::sync::Arc;

use jsonrpsee::core::client::{ClientT, Error as JsonError};
use jsonrpsee::rpc_params;
use jsonrpsee::ws_client::{WsClient, WsClientBuilder};
use subspace_rpc_primitives::FarmerAppInfo;

/// Node client implementation that connects to node via RPC (WebSockets).
///
/// This implementation is supposed to be used on local network and not via public Internet due to
/// sensitive contents.
#[derive(Debug, Clone)]
pub struct RpcNodeClient {
    client: Arc<WsClient>,
}

impl RpcNodeClient {
    /// Create a new instance of [`NodeClient`].
    pub async fn new(url: &str) -> Result<Self, JsonError> {
        let client = Arc::new(
            WsClientBuilder::default()
                .max_request_size(20 * 1024 * 1024)
                .build(url)
                .await?,
        );
        Ok(Self { client })
    }

    pub async fn farmer_app_info(&self) -> anyhow::Result<FarmerAppInfo> {
        Ok(self
            .client
            .request("subspace_getFarmerAppInfo", rpc_params![])
            .await?)
    }
}
