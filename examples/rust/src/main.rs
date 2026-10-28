/// The line the program prints.
fn greeting() -> &'static str {
    "hello from rust"
}

fn main() {
    println!("{}", greeting());
}

#[cfg(test)]
mod tests {
    #[test]
    fn greets() {
        assert_eq!(super::greeting(), "hello from rust");
    }
}
