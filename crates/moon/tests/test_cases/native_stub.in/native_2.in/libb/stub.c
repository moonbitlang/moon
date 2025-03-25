#include <stdio.h>
#include <moonbit.h>

MOONBIT_FFI_EXPORT void say_hello_2() {
    printf("Hello world from native_2/libb/stub.c!!!\n");
}