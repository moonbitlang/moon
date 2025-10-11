#include <moonbit.h>

void say_hello_1_internal();

MOONBIT_FFI_EXPORT void say_hello_1() {
    say_hello_1_internal();
}
