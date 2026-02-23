//! Invalid signature spam test.
//! Validates that the system immediately drops requests with invalid cryptographic signatures.

pub struct SignatureVerifier;

impl SignatureVerifier {
    /// Mock signature verification.
    /// In a production system, this would use Ed25519 or SECP256k1 to verify.
    pub fn verify(payload: &str, signature: &str, public_key: &str) -> bool {
        if signature == "invalid_sig" || signature.is_empty() {
            return false;
        }

        let expected = format!("sig_{}_{}", payload, public_key);
        signature == expected
    }
}

pub struct ApiGateway {
    pub dropped_requests: u64,
    pub accepted_requests: u64,
}

impl ApiGateway {
    pub fn new() -> Self {
        Self {
            dropped_requests: 0,
            accepted_requests: 0,
        }
    }

    pub fn handle_request(&mut self, payload: &str, signature: &str, public_key: &str) {
        if SignatureVerifier::verify(payload, signature, public_key) {
            self.accepted_requests += 1;
        } else {
            self.dropped_requests += 1;
        }
    }
}

impl Default for ApiGateway {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_invalid_signature_spam_mitigation() {
        let mut gateway = ApiGateway::new();

        // Attacker sends 1000 invalid signatures
        for _ in 0..1000 {
            gateway.handle_request("buy_100_btc", "invalid_sig", "pk_123");
        }

        // Legitimate user sends 1 valid request
        gateway.handle_request("sell_1_eth", "sig_sell_1_eth_pk_456", "pk_456");

        assert_eq!(gateway.dropped_requests, 1000);
        assert_eq!(gateway.accepted_requests, 1);
    }
}
