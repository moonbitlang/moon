#include <moonbit.h>

void say_hello_3_internal();

MOONBIT_FFI_EXPORT void say_hello_3() { say_hello_3_internal(); }
