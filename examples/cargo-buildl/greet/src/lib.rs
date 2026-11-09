//! A Rust library the other crates of the workspace use.

/// Returns the greeting for `name`.
#[must_use]
pub fn greet(name: &str) -> String {
    format!("hello, {name}!")
}

#[cfg(test)]
mod tests {
    use super::greet;

    #[test]
    fn greets_by_name() {
        assert_eq!(greet("buildl"), "hello, buildl!");
    }
}
