// moon: The build system and package manager for MoonBit.
// Copyright (C) 2026 International Digital Economy Academy
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND,
// either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

//! Legacy stdio, random, time, and process imports. Follow the Wasmtime
//! adapter's value semantics while keeping stdio and termination in Runtime.

use super::{Bytes, Kind, StoreData, Value, api, bind, host_error, owned, reference};
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::cell::RefCell;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct Random(RefCell<StdRng>);

pub(super) fn define<'s>(
    store: &'s api::Store<StoreData>,
    import: &api::Import,
) -> api::Result<Option<api::HostFunc<'s, StoreData>>> {
    let (params, results): (&[Kind], &[Kind]) = match (import.module.as_str(), import.name.as_str())
    {
        ("__moonbit_io_unstable", "read_bytes_from_stdin") => (&[], &[Kind::ExternRef]),
        ("__moonbit_io_unstable" | "spectest", "read_char") => (&[], &[Kind::I32]),
        ("__moonbit_io_unstable", "write_char") => (&[Kind::I32, Kind::I32], &[]),
        ("__moonbit_io_unstable", "flush") => (&[Kind::I32], &[]),
        ("__moonbit_rand_unstable", "stdrng_seed_from_u64") => (&[Kind::I32], &[Kind::ExternRef]),
        ("__moonbit_rand_unstable", "stdrng_gen_range") => {
            (&[Kind::ExternRef, Kind::I32], &[Kind::I32])
        }
        ("__moonbit_time_unstable", "instant_now") => (&[], &[Kind::ExternRef]),
        ("__moonbit_time_unstable", "instant_elapsed_as_secs_f64") => {
            (&[Kind::ExternRef], &[Kind::F64])
        }
        ("__moonbit_time_unstable", "now") => (&[], &[Kind::I64]),
        ("__moonbit_sys_unstable", "is_windows") => (&[], &[Kind::I32]),
        ("__moonbit_sys_unstable", "exit") => (&[Kind::I32], &[]),
        _ => return Ok(None),
    };
    let name = import.name.clone();
    bind(store, import, params, results, move |caller, args| {
        let result = match name.as_str() {
            "read_bytes_from_stdin" => {
                let mut bytes = Vec::new();
                caller
                    .with_data(|state| {
                        state
                            .runtime
                            .stdio()
                            .with_stdin(|stdin| stdin.read_to_end(&mut bytes))
                    })
                    .map_err(host_error)?;
                owned(&caller, Bytes(bytes))?
            }
            "read_char" => Value::I32(
                caller
                    .with_data(|state| state.runtime.stdio().read_utf8_char())
                    .ok()
                    .flatten()
                    .map_or(-1, |value| value as i32),
            ),
            "write_char" => {
                let (Value::I32(fd), Value::I32(value)) = (&args[0], &args[1]) else {
                    unreachable!("checked signature")
                };
                let value =
                    char::from_u32(*value as u32).ok_or_else(|| host_error("invalid character"))?;
                caller
                    .with_data(|state| match fd {
                        1 => state
                            .runtime
                            .stdio()
                            .with_stdout(|stdout| write!(stdout, "{value}")),
                        2 => state
                            .runtime
                            .stdio()
                            .with_stderr(|stderr| write!(stderr, "{value}")),
                        _ => Ok(()),
                    })
                    .map_err(host_error)?;
                return Ok(Vec::new());
            }
            "flush" => {
                let Value::I32(fd) = args[0] else {
                    unreachable!("checked signature")
                };
                caller
                    .with_data(|state| match fd {
                        1 => state.runtime.stdio().with_stdout(|stdout| stdout.flush()),
                        2 => state.runtime.stdio().with_stderr(|stderr| stderr.flush()),
                        _ => Ok(()),
                    })
                    .map_err(host_error)?;
                return Ok(Vec::new());
            }
            "stdrng_seed_from_u64" => {
                let Value::I32(seed) = args[0] else {
                    unreachable!("checked signature")
                };
                owned(
                    &caller,
                    Random(RefCell::new(StdRng::seed_from_u64(seed as u64))),
                )?
            }
            "stdrng_gen_range" => {
                let Value::I32(upper) = args[1] else {
                    unreachable!("checked signature")
                };
                if upper <= 0 {
                    return Err(host_error("random range upper bound must be positive"));
                }
                Value::I32(
                    reference::<Random>(&args[0])?
                        .0
                        .borrow_mut()
                        .gen_range(0..upper),
                )
            }
            "instant_now" => owned(&caller, Instant::now())?,
            "instant_elapsed_as_secs_f64" => {
                Value::F64(reference::<Instant>(&args[0])?.elapsed().as_secs_f64())
            }
            "now" => Value::I64(
                u64::try_from(
                    SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .map_err(host_error)?
                        .as_millis(),
                )
                .map_err(host_error)? as i64,
            ),
            "is_windows" => Value::I32(i32::from(cfg!(windows))),
            "exit" => {
                let Value::I32(code) = args[0] else {
                    unreachable!("checked signature")
                };
                caller.with_data(|state| {
                    state
                        .termination
                        .request(crate::run_termination::RunTermination::Exit(code))
                });
                return Err(host_error("run termination requested"));
            }
            _ => unreachable!("checked import"),
        };
        Ok(vec![result])
    })
}
