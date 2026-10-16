//! Placeholder of the hub. The real hub arrives with the read-only UI phase;
//! until then this binary only gives the workspace something to build and test.

/// The version of this build, as printed by `rules-hub --version`.
fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

fn main() {
    println!("rules-hub {}", version());
}

#[cfg(test)]
mod tests {
    use super::version;

    #[test]
    fn version_is_semver_shaped() {
        assert_eq!(version().split('.').count(), 3);
    }
}
