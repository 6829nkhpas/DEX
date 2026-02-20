use crate::error::AppError;
use std::time::Instant;
use dashmap::DashMap;

#[derive(Clone)]
struct Bucket {
    capacity: u32,
    tokens: f64,
    refill_rate: f64,
    last_update: Instant,
}

impl Bucket {
    fn new(capacity: u32, refill_rate: f64) -> Self {
        Self {
            capacity,
            tokens: capacity as f64,
            refill_rate,
            last_update: Instant::now(),
        }
    }

    fn allow_request(&mut self, tokens: u32) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_update).as_secs_f64();

        // Refill tokens
        self.tokens = f64::min(
            self.capacity as f64,
            self.tokens + elapsed * self.refill_rate,
        );
        self.last_update = now;

        // Consume token
        if self.tokens >= tokens as f64 {
            self.tokens -= tokens as f64;
            true
        } else {
            false // Rate limited
        }
    }
}

pub struct RateLimiter {
    // Maps unique keys e.g., "user_id:endpoint" to Bucket
    buckets: DashMap<String, Bucket>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self {
            buckets: DashMap::new(),
        }
    }

    pub fn check_rate_limit(&self, key: &str, capacity: u32, refill_rate: f64) -> Result<(), AppError> {
        let mut bucket = self.buckets.entry(key.to_string()).or_insert_with(|| {
            Bucket::new(capacity, refill_rate)
        });

        if bucket.allow_request(1) {
            Ok(())
        } else {
            Err(AppError::RateLimitExceeded(format!("Rate limit for {}", key)))
        }
    }
}
