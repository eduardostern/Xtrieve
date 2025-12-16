//! Server utilities and helpers

use std::time::{Duration, Instant};

/// Connection statistics
#[derive(Debug, Default)]
pub struct ConnectionStats {
    pub total_requests: u64,
    pub total_errors: u64,
    pub connected_at: Option<Instant>,
}

impl ConnectionStats {
    pub fn new() -> Self {
        ConnectionStats {
            total_requests: 0,
            total_errors: 0,
            connected_at: Some(Instant::now()),
        }
    }

    pub fn record_request(&mut self, success: bool) {
        self.total_requests += 1;
        if !success {
            self.total_errors += 1;
        }
    }

    pub fn uptime(&self) -> Duration {
        self.connected_at
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO)
    }
}
