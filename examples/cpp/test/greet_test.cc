#include <iostream>

#include "greet.h"

int main() {
    if (greet("world") != "hello, world") {
        std::cerr << "greet(\"world\") returned " << greet("world") << "\n";
        return 1;
    }
    return 0;
}
