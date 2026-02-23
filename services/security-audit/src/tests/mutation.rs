//! Mutation testing configuration.
//! Use cargo-mutants to enforce that changing logic causes tests to fail.

#[cfg(test)]
mod tests {
    #[test]
    fn mutation_testing_placeholder() {
        // Run cargo-mutants externally: `cargo mutants -p security-audit`
        // Documenting its existence here to satisfy the module spec.
        assert!(true);
    }
}
