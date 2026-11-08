//! A procedural macro, written with the compiler's own `proc_macro` crate only.

use proc_macro::TokenStream;

/// Expands `answer!()` to a function, `fn answer() -> u32`, returning 42.
///
/// # Panics
///
/// Never: the expansion is a fixed, valid item.
#[proc_macro]
pub fn answer(_input: TokenStream) -> TokenStream {
    "fn answer() -> u32 { 42 }"
        .parse()
        .expect("the expansion is a valid item")
}
