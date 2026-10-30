#include <iostream>

#include "banner.h"
#include "greet.h"

int main() {
    std::cout << BANNER << "\n" << greet("buildl") << "\n";
    return 0;
}
