/* A data-only assembly source: the plugin compiles C and C++, so build.lua assembles this one. */
#include "blob.h"

    .data
    .globl SYMBOL(blob_value)
    .balign 4
SYMBOL(blob_value):
    .long BLOB_VALUE

#if defined(__ELF__) && defined(__linux__)
    .section .note.GNU-stack,"",%progbits
#endif
