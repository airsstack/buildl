#include <stdio.h>

#include "greeter.h"

int main(void) {
    printf("[rust-ffi] shared: %s\n", greeter_hello());
    return 0;
}
