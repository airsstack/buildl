//! A static library for C: one function, `ffi_greeting_len`.

use std::ffi::{c_char, CStr};

/// Returns the length in bytes of the greeting for `name`, a C string; 0 when `name` is null or
/// not UTF-8.
///
/// # Safety
///
/// `name` is null, or points to a NUL-terminated string that stays valid for the call.
#[no_mangle]
pub unsafe extern "C" fn ffi_greeting_len(name: *const c_char) -> usize {
    if name.is_null() {
        return 0;
    }
    // SAFETY: the caller promises `name` is a valid NUL-terminated string.
    let name = unsafe { CStr::from_ptr(name) };
    name.to_str().map_or(0, |name| greet::greet(name).len())
}

#[cfg(test)]
mod tests {
    use super::ffi_greeting_len;

    #[test]
    fn measures_the_greeting() {
        let name = c"buildl";
        // SAFETY: `name` is a NUL-terminated string that outlives the call.
        assert_eq!(unsafe { ffi_greeting_len(name.as_ptr()) }, "hello, buildl!".len());
    }
}
