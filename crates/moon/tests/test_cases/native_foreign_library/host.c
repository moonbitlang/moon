#include "moonbit.h"
#include <stdio.h>

extern int32_t add(int32_t a, int32_t b);

int main(void) {
  moonbit_init();
  printf("add(40, 2) = %d\n", add(40, 2));
  return 0;
}
