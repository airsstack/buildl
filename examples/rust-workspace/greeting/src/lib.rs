//! A Rust library other crates of the workspace use.

/// Returns the greeting for `name`, upper-cased when the `shout` feature is on.
#[must_use]
pub fn greet(name: &str) -> String {
    let text = format!("hello, {name}!");
    if cfg!(feature = "shout") {
        text.to_uppercase()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::greet;

    #[test]
    fn greets_by_name() {
        let expected = if cfg!(feature = "shout") { "HELLO, BUILDL!" } else { "hello, buildl!" };
        assert_eq!(greet("buildl"), expected);
    }
}
