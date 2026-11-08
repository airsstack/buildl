//! A shared library for C: the function of `c/include/greeter.h`.

use std::ffi::{c_char, CStr};

const GREETING: &CStr = c"hello from a Rust shared library";

/// Returns a greeting, a C string the library owns; the caller must not free it.
#[no_mangle]
pub extern "C" fn greeter_hello() -> *const c_char {
    GREETING.as_ptr()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_greeting_is_a_c_string() {
        // SAFETY: greeter_hello returns a pointer to a static, NUL-terminated string.
        let text = unsafe { CStr::from_ptr(greeter_hello()) };
        assert_eq!(text.to_str(), Ok("hello from a Rust shared library"));
    }
}
