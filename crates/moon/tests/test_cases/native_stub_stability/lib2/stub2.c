#include <moonbit.h>

void say_hello_2_internal();

MOONBIT_FFI_EXPORT void say_hello_2() { say_hello_2_internal(); }
