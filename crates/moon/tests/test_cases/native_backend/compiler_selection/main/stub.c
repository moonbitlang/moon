// Also require the discovered Windows SDK's C headers.
#include <stdio.h>

int uses_clang_with_static_msvc_crt(void) {
#if defined(__clang__) && defined(_MSC_VER) && defined(_MT) && !defined(_DLL)
  return 1;
#else
  return 0;
#endif
}
