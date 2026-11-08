//! A Rust program calling a C library: `clamp_i32` of `c/include/clamp.h`.

extern "C" {
    fn clamp_i32(value: i32, lo: i32, hi: i32) -> i32;
}

/// Calls the C function; it reads only its arguments, so any values are sound.
fn clamp(value: i32, lo: i32, hi: i32) -> i32 {
    // SAFETY: clamp_i32 takes three integers by value and touches no memory.
    unsafe { clamp_i32(value, lo, hi) }
}

fn main() {
    println!(
        "[rust-ffi] rust calls C: clamp(15, 0, 10) = {}, clamp(-3, 0, 10) = {}",
        clamp(15, 0, 10),
        clamp(-3, 0, 10)
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clamps_through_c() {
        assert_eq!(clamp(15, 0, 10), 10);
        assert_eq!(clamp(-3, 0, 10), 0);
        assert_eq!(clamp(7, 0, 10), 7);
    }
}
