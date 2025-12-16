//! gRPC client for connecting to xtrieved

use tonic::transport::Channel;
use crate::proto::xtrieve_client::XtrieveClient as GrpcClient;
use crate::proto::{BtrieveRequest, BtrieveResponse, StatusRequest, StatusResponse};
use xtrieve_engine::{BtrieveError, BtrieveResult, StatusCode};

/// Client for connecting to xtrieved daemon
pub struct XtrieveClient {
    client: GrpcClient<Channel>,
}

impl XtrieveClient {
    /// Connect to xtrieved at the given address
    pub async fn connect(addr: &str) -> Result<Self, tonic::transport::Error> {
        let client = GrpcClient::connect(addr.to_string()).await?;
        Ok(XtrieveClient { client })
    }

    /// Execute a raw Btrieve operation
    pub async fn execute(&mut self, request: BtrieveRequest) -> BtrieveResult<BtrieveResponse> {
        let response = self.client
            .execute(request)
            .await
            .map_err(|e| BtrieveError::Internal(e.to_string()))?
            .into_inner();

        if response.status_code == 0 {
            Ok(response)
        } else {
            Err(BtrieveError::Status(StatusCode::from_raw(response.status_code as u16)))
        }
    }

    /// Get server status
    pub async fn get_status(&mut self, include_files: bool, include_stats: bool) -> BtrieveResult<StatusResponse> {
        let request = StatusRequest {
            include_open_files: include_files,
            include_statistics: include_stats,
        };

        let response = self.client
            .get_status(request)
            .await
            .map_err(|e| BtrieveError::Internal(e.to_string()))?
            .into_inner();

        Ok(response)
    }

    /// Request server shutdown
    pub async fn shutdown(&mut self, graceful: bool) -> BtrieveResult<()> {
        let request = crate::proto::ShutdownRequest { graceful };

        self.client
            .shutdown(request)
            .await
            .map_err(|e| BtrieveError::Internal(e.to_string()))?;

        Ok(())
    }
}
