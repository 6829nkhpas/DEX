//! Severity classifier.
//! Evaluates identified vulnerabilities against the exchange's risk policy.

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Low,
    Medium,
    High,
    Critical,
}

pub struct Vulnerability {
    pub cvss_score: f32,
    pub affects_financial_invariant: bool,
    pub affects_authentication: bool,
}

pub struct SeverityClassifier;

impl SeverityClassifier {
    /// Classify severity based on business logic rules.
    pub fn classify(vuln: &Vulnerability) -> Severity {
        if vuln.affects_financial_invariant {
            return Severity::Critical;
        }
        if vuln.affects_authentication {
            return Severity::High;
        }
        if vuln.cvss_score >= 9.0 {
            return Severity::Critical;
        } else if vuln.cvss_score >= 7.0 {
            return Severity::High;
        } else if vuln.cvss_score >= 4.0 {
            return Severity::Medium;
        }
        
        Severity::Low
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_classification() {
        let financial_vuln = Vulnerability {
            cvss_score: 5.0,
            affects_financial_invariant: true,
            affects_authentication: false,
        };
        assert_eq!(SeverityClassifier::classify(&financial_vuln), Severity::Critical);

        let low_vuln = Vulnerability {
            cvss_score: 3.5,
            affects_financial_invariant: false,
            affects_authentication: false,
        };
        assert_eq!(SeverityClassifier::classify(&low_vuln), Severity::Low);
    }
}
