//! A static library for C: the functions of `c/include/mathlib.h`.

/// Returns `a + b`, wrapping on overflow.
#[no_mangle]
pub extern "C" fn mathlib_add(a: i32, b: i32) -> i32 {
    a.wrapping_add(b)
}

/// Returns `a * b`, wrapping on overflow.
#[no_mangle]
pub extern "C" fn mathlib_mul(a: i32, b: i32) -> i32 {
    a.wrapping_mul(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_and_multiplies() {
        assert_eq!(mathlib_add(2, 3), 5);
        assert_eq!(mathlib_mul(6, 7), 42);
        assert_eq!(mathlib_add(i32::MAX, 1), i32::MIN);
    }
}
