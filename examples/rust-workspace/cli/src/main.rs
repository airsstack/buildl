//! A binary using an rlib (`greeting`) and a proc-macro (`answer`).

answer::answer!();

fn main() {
    println!("[rust-workspace] {} answer = {}", greeting::greet("buildl"), answer());
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_macro_expands_to_a_function() {
        assert_eq!(super::answer(), 42);
    }
}
