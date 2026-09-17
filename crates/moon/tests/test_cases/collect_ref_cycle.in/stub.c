#include "moonbit.h"

static int32_t finalized;

static void finalize(void *token) {
  (void)token;
  finalized++;
}

MOONBIT_FFI_EXPORT void *cycle_test_token_new(void) {
  return moonbit_make_external_object(finalize, sizeof(int32_t));
}

MOONBIT_FFI_EXPORT int32_t cycle_test_finalized_count(void) {
  return finalized;
}
