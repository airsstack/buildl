/* The function of the Rust shared library greeter (rust/greeter/src/lib.rs). */
#ifndef GREETER_H
#define GREETER_H

/* A greeting the library owns; do not free it. */
const char *greeter_hello(void);

#endif
