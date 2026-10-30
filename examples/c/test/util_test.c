#include <stdio.h>

#include "util.h"

int main(void) {
    if (util_add(2, 3) != 5) {
        fprintf(stderr, "util_add(2, 3) is not 5\n");
        return 1;
    }
    return 0;
}
