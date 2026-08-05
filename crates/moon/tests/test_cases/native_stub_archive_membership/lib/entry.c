#include <moonbit.h>

int stub_impl(void);

MOONBIT_FFI_EXPORT int stub_value(void) { return stub_impl(); }
