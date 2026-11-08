/* Calls the Rust static library from C, as a C program would. */
#include <stdio.h>

#include "mathlib.h"

int main(void) {
    if (mathlib_add(2, 3) != 5 || mathlib_mul(-4, 5) != -20) {
        fprintf(stderr, "mathlib_test: wrong result\n");
        return 1;
    }
    puts("mathlib_test: ok");
    return 0;
}
