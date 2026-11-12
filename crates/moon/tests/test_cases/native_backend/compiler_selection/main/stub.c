// Also require the discovered Windows SDK's C headers.
#include <stdio.h>

int compiler_selection_is_clang_cl(void) {
#if defined(__clang__) && defined(_MSC_VER)
  return 1;
#else
  return 0;
#endif
}
