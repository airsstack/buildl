/* A C library Rust calls: c/lib/clamp.c, built by c.library into out/c/libclamp.a. */
#ifndef CLAMP_H
#define CLAMP_H

#include <stdint.h>

/* Returns value limited to the range lo..hi. */
int32_t clamp_i32(int32_t value, int32_t lo, int32_t hi);

#endif
