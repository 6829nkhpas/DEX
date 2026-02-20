use crate::rate_limit::RateLimiter;
use reqwest::Client;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub rate_limiter: Arc<RateLimiter>,
    pub http_client: Client,
    pub internal_services_url: String, // Mock base URL for the internal dummy gRPC/HTTP endpoints
}

impl AppState {
    pub fn new(service_url: String) -> Self {
        Self {
            rate_limiter: Arc::new(RateLimiter::new()),
            http_client: Client::new(),
            internal_services_url: service_url,
        }
    }
}
