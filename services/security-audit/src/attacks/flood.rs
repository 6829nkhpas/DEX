//! Flood attack simulation.
//! Simulates high-rate traffic to assess the Token Bucket rate limit policy.

use std::time::Instant;

/// Simulated Token Bucket Rate Limiter as specified in §6.1 of the rate limit policy.
pub struct RateLimiter {
    pub capacity: f64,
    pub tokens: f64,
    pub refill_rate: f64,
    pub last_update: Instant,
}

impl RateLimiter {
    /// Create a new rate limiter with a defined rate (requests per second).
    /// The bucket capacity (burst allowance) is 2x the rate limit.
    pub fn new(rate_limit_per_second: f64, initial_time: Instant) -> Self {
        let capacity = rate_limit_per_second * 2.0;
        Self {
            capacity,
            tokens: capacity, // Start fully filled
            refill_rate: rate_limit_per_second,
            last_update: initial_time,
        }
    }

    /// Tests if a request is allowed at the given instant.
    pub fn allow_request(&mut self, now: Instant) -> bool {
        let elapsed = now.duration_since(self.last_update).as_secs_f64();
        
        // Refill tokens based on elapsed time
        self.tokens = f64::min(
            self.capacity,
            self.tokens + elapsed * self.refill_rate
        );
        self.last_update = now;
        
        // Consume token if available
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn test_flood_attack_mitigation() {
        // Standard Order Placement is 20 req/sec according to spec §3.1
        let base_rate = 20.0;
        let mut mock_time = Instant::now();
        let mut limiter = RateLimiter::new(base_rate, mock_time);
        
        // The bucket capacity is base_rate * 2.0 = 40.0 tokens.
        // Therefore, an attacker can theoretically burst 40 requests without time passing.
        
        let mut accepted = 0;
        let mut rejected = 0;

        // Attacker simulates a flood of 50 simultaneous requests (0 elapsed time)
        for _ in 0..50 {
            if limiter.allow_request(mock_time) {
                accepted += 1;
            } else {
                rejected += 1;
            }
        }

        // Expected: Exactly 40 go through, 10 are rejected (HTTP 429 equivalent).
        assert_eq!(accepted, 40);
        assert_eq!(rejected, 10);

        // After a simulated 0.5 seconds, we should get 10 more tokens.
        mock_time += Duration::from_millis(500);

        let mut next_accepted = 0;
        let mut next_rejected = 0;

        for _ in 0..15 {
            if limiter.allow_request(mock_time) {
                next_accepted += 1;
            } else {
                next_rejected += 1;
            }
        }

        // 0.5 sec * 20 req/sec = 10 tokens recovered.
        assert_eq!(next_accepted, 10);
        assert_eq!(next_rejected, 5);
    }
}
