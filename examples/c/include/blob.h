#ifndef BLOB_H
#define BLOB_H

/* Mach-O C symbols carry a leading underscore; ELF ones do not. */
#ifdef __APPLE__
#define SYMBOL(name) _##name
#else
#define SYMBOL(name) name
#endif

#define BLOB_VALUE 42

#endif
