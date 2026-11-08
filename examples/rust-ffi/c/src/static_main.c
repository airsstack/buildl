#include <stdio.h>

#include "mathlib.h"

int main(void) {
    printf("[rust-ffi] static: mathlib_add(2, 3) = %d, mathlib_mul(6, 7) = %d\n",
           mathlib_add(2, 3), mathlib_mul(6, 7));
    return 0;
}
