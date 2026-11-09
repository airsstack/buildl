//! A binary using the `greet` library.

/// The line the binary prints.
fn line() -> String {
    format!("[cargo-buildl] {}", greet::greet("buildl"))
}

fn main() {
    println!("{}", line());
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_line_starts_with_the_tag() {
        assert_eq!(super::line(), "[cargo-buildl] hello, buildl!");
    }
}
