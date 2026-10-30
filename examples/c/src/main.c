#include <stdio.h>

#include "banner.h"
#include "config.h"
#include "util.h"

/* Defined in src/blob.S. */
extern int blob_value;

int main(void) {
    printf("%s %s: util_add(%d, %d) = %d, blob = %d\n", BANNER, GREETING, LEFT, RIGHT,
           util_add(LEFT, RIGHT), blob_value);
    return 0;
}
