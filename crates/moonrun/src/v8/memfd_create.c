/*
 * moon: The build system and package manager for MoonBit.
 * Copyright (C) 2026 International Digital Economy Academy
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *     http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
 * either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 *
 * For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.
 */

#define _DEFAULT_SOURCE 1
#include <features.h>

#if !__GLIBC_PREREQ(2, 27)
#include <sys/syscall.h>
#include <unistd.h>

// rusty_v8 137.3.0 calls memfd_create from its Wasm memory-control code.
// Older glibc lacks the wrapper for this Linux syscall. Keep the definition
// private to the linked binary so shared libraries use their own libc.
__attribute__((visibility("hidden")))
int memfd_create(const char *name, unsigned int flags) {
    return (int)syscall(SYS_memfd_create, name, flags);
}
#endif
