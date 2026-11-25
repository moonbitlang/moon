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

use std::path::PathBuf;

use crate::async_host::{AsyncHostError, AsyncHostResult, read_u16};
use crate::guest_memory::GuestMemory;
use crate::runtime::null_handle;
use crate::tls::{TlsFileType, TlsTrust};

use super::context::ImportContext;
use super::provenance::ported_imports;

#[cfg(unix)]
fn error_message_buffer(message: String) -> Box<[u8]> {
    let mut bytes = message.into_bytes();
    bytes.push(0);
    bytes.into_boxed_slice()
}

#[cfg(windows)]
fn error_message_buffer(message: String) -> Box<[u8]> {
    let mut bytes = Vec::with_capacity((message.len() + 1) * std::mem::size_of::<u16>());
    for unit in message.encode_utf16() {
        bytes.extend_from_slice(&unit.to_le_bytes());
    }
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.into_boxed_slice()
}

ported_imports! {
#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/new")]
pub(super) fn new(context: &mut ImportContext<'_>) -> u64 {
    context.host.tls().new_connection()
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/set_client")]
pub(super) fn set_client(
    context: &mut ImportContext<'_>,
    tls: u64,
    host: u32,
    host_len: u32,
    sni: i32,
    trust_abi: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host_state, memory| {
        let hostname = read_guest_string(memory, host, host_len)?;
        let trust = TlsTrust::from_abi(trust_abi)?;
        host_state.tls().set_client(tls, &hostname, sni != 0, trust)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/add_root_certificate")]
pub(super) fn add_root_certificate(
    context: &mut ImportContext<'_>,
    tls: u64,
    root: u32,
    root_len: u32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let root = memory.read_exact(root, root_len)?;
        host.tls().add_root_certificate(tls, root)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/set_server_files")]
#[allow(clippy::too_many_arguments)]
pub(super) fn set_server_files(
    context: &mut ImportContext<'_>,
    tls: u64,
    private_key_file: u32,
    private_key_file_len: u32,
    private_key_type_abi: i32,
    certificate_file: u32,
    certificate_file_len: u32,
    certificate_type_abi: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let private_key_file = read_guest_os_path(memory, private_key_file, private_key_file_len)?;
        let certificate_file = read_guest_os_path(memory, certificate_file, certificate_file_len)?;
        host.tls().set_server_files(
            tls,
            private_key_file,
            TlsFileType::from_abi(private_key_type_abi)?,
            certificate_file,
            TlsFileType::from_abi(certificate_type_abi)?,
        )
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/set_server_pfx")]
pub(super) fn set_server_pfx(
    context: &mut ImportContext<'_>,
    tls: u64,
    pfx_content: u32,
    pfx_content_len: u32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        host.tls().set_server_pfx(
            tls,
            memory.read_exact(pfx_content, pfx_content_len)?.to_vec(),
        )
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/free")]
pub(super) fn free(context: &mut ImportContext<'_>, tls: u64) -> AsyncHostResult<()> {
    context.host.tls().free(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/take_error")]
pub(super) fn take_error(context: &mut ImportContext<'_>, tls: u64) -> AsyncHostResult<u64> {
    let message = context.host.tls().take_error(tls)?
        .unwrap_or_else(|| "unknown TLS error".to_string());
    Ok(context.host.insert_c_buffer(error_message_buffer(message)))
}

#[compat(
    source = "src/tls/tls.wasm.mbt",
    original = "tls/error/take_global",
    upstream_pr = 611,
    replacement = "errors are owned by the live TLS connection",
    api_only = true
)]
pub(super) fn take_global_error(context: &mut ImportContext<'_>) -> u64 {
    context.host.insert_c_buffer(error_message_buffer("unknown TLS error".to_string()))
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/read_plain")]
#[allow(clippy::too_many_arguments)]
pub(super) fn read_plain(
    context: &mut ImportContext<'_>,
    tls: u64,
    in_buffer: u32,
    in_buffer_offset: u32,
    in_buffer_len: u32,
    out_buffer: u32,
    out_buffer_offset: u32,
    out_buffer_len: u32,
    plain_buffer: u32,
    plain_buffer_offset: u32,
    plain_buffer_len: u32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let (input, output, plain) = read_guest_buffers3_mut(
            memory,
            guest_offset(in_buffer, in_buffer_offset)?,
            in_buffer_len,
            guest_offset(out_buffer, out_buffer_offset)?,
            out_buffer_len,
            guest_offset(plain_buffer, plain_buffer_offset)?,
            plain_buffer_len,
        )?;
        host.tls().read_plain(tls, input, plain, output)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/write_plain")]
#[allow(clippy::too_many_arguments)]
pub(super) fn write_plain(
    context: &mut ImportContext<'_>,
    tls: u64,
    in_buffer: u32,
    in_buffer_offset: u32,
    in_buffer_len: u32,
    out_buffer: u32,
    out_buffer_offset: u32,
    out_buffer_len: u32,
    plain_buffer: u32,
    plain_buffer_offset: u32,
    plain_buffer_len: u32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let (input, output, plain) = read_guest_buffers3_mut(
            memory,
            guest_offset(in_buffer, in_buffer_offset)?,
            in_buffer_len,
            guest_offset(out_buffer, out_buffer_offset)?,
            out_buffer_len,
            guest_offset(plain_buffer, plain_buffer_offset)?,
            plain_buffer_len,
        )?;
        host.tls().write_plain(tls, input, plain, output)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/connect")]
#[allow(clippy::too_many_arguments)]
pub(super) fn connect(
    context: &mut ImportContext<'_>,
    tls: u64,
    in_buffer: u32,
    in_buffer_offset: u32,
    in_buffer_len: u32,
    out_buffer: u32,
    out_buffer_offset: u32,
    out_buffer_len: u32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let (input, output) = read_guest_input_output_mut(
            memory,
            guest_offset(in_buffer, in_buffer_offset)?,
            in_buffer_len,
            guest_offset(out_buffer, out_buffer_offset)?,
            out_buffer_len,
        )?;
        host.tls().connect(tls, input, output)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/accept")]
#[allow(clippy::too_many_arguments)]
pub(super) fn accept(
    context: &mut ImportContext<'_>,
    tls: u64,
    in_buffer: u32,
    in_buffer_offset: u32,
    in_buffer_len: u32,
    out_buffer: u32,
    out_buffer_offset: u32,
    out_buffer_len: u32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let (input, output) = read_guest_input_output_mut(
            memory,
            guest_offset(in_buffer, in_buffer_offset)?,
            in_buffer_len,
            guest_offset(out_buffer, out_buffer_offset)?,
            out_buffer_len,
        )?;
        host.tls().accept(tls, input, output)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/bytes_read")]
pub(super) fn bytes_read(context: &mut ImportContext<'_>, tls: u64) -> AsyncHostResult<u32> {
    context.host.tls().bytes_read(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/bytes_to_write")]
pub(super) fn bytes_to_write(
    context: &mut ImportContext<'_>,
    tls: u64,
) -> AsyncHostResult<u32> {
    context.host.tls().bytes_to_write(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/wants_read")]
pub(super) fn wants_read(context: &mut ImportContext<'_>, tls: u64) -> AsyncHostResult<i32> {
    context.host.tls().wants_read(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/wants_write")]
pub(super) fn wants_write(context: &mut ImportContext<'_>, tls: u64) -> AsyncHostResult<i32> {
    context.host.tls().wants_write(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/shutdown")]
pub(super) fn shutdown(
    context: &mut ImportContext<'_>,
    tls: u64,
) -> AsyncHostResult<i32> {
    context.host.tls().shutdown(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/peer_certificate")]
pub(super) fn peer_certificate(
    context: &mut ImportContext<'_>,
    tls: u64,
) -> AsyncHostResult<u64> {
    match context.host.tls().peer_certificate(tls)? {
        Ok(Some(buffer)) => Ok(context.host.insert_c_buffer(buffer.into_boxed_slice())),
        // The guest distinguishes an absent certificate (an empty buffer) from
        // a TLS error (null). The connection retains the error diagnostic.
        Ok(None) => Ok(context.host.insert_c_buffer(Box::default())),
        Err(()) => Ok(null_handle()),
    }
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/unique_channel_binding")]
pub(super) fn unique_channel_binding(
    context: &mut ImportContext<'_>,
    tls: u64,
) -> AsyncHostResult<u64> {
    match context.host.tls().unique_channel_binding(tls)? {
        Ok(Some(buffer)) => Ok(context.host.insert_c_buffer(buffer.into_boxed_slice())),
        Ok(None) | Err(()) => Ok(null_handle()),
    }
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/server_endpoint_channel_binding")]
pub(super) fn server_endpoint_channel_binding(
    context: &mut ImportContext<'_>,
    tls: u64,
) -> AsyncHostResult<u64> {
    match context.host.tls().server_endpoint_channel_binding(tls)? {
        Ok(Some(buffer)) => Ok(context.host.insert_c_buffer(buffer.into_boxed_slice())),
        Ok(None) | Err(()) => Ok(null_handle()),
    }
}
}

fn guest_offset(ptr: u32, offset: u32) -> AsyncHostResult<usize> {
    let ptr = usize::try_from(ptr).map_err(|_| AsyncHostError::Fault)?;
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    ptr.checked_add(offset).ok_or(AsyncHostError::Fault)
}

fn read_guest_input_output_mut(
    memory: &mut (impl GuestMemory + ?Sized),
    input_offset: usize,
    input_len: u32,
    output_offset: usize,
    output_len: u32,
) -> AsyncHostResult<(&mut [u8], &mut [u8])> {
    let input = guest_range(memory.bytes().len(), input_offset, input_len)?;
    let output = guest_range(memory.bytes().len(), output_offset, output_len)?;
    if input.end <= output.start {
        let (before_output, output_and_after) = memory.bytes_mut().split_at_mut(output.start);
        Ok((
            &mut before_output[input],
            &mut output_and_after[..output_len as usize],
        ))
    } else if output.end <= input.start {
        let (before_input, input_and_after) = memory.bytes_mut().split_at_mut(input.start);
        Ok((
            &mut input_and_after[..input_len as usize],
            &mut before_input[output],
        ))
    } else {
        Err(AsyncHostError::Fault)
    }
}

fn read_guest_buffers3_mut(
    memory: &mut (impl GuestMemory + ?Sized),
    first_offset: usize,
    first_len: u32,
    second_offset: usize,
    second_len: u32,
    third_offset: usize,
    third_len: u32,
) -> AsyncHostResult<(&mut [u8], &mut [u8], &mut [u8])> {
    let memory_len = memory.bytes().len();
    let first = guest_range(memory_len, first_offset, first_len)?;
    let second = guest_range(memory_len, second_offset, second_len)?;
    let third = guest_range(memory_len, third_offset, third_len)?;
    if ranges_overlap(&first, &second)
        || ranges_overlap(&first, &third)
        || ranges_overlap(&second, &third)
    {
        return Err(AsyncHostError::Fault);
    }

    let bytes = memory.bytes_mut();
    let ptr = bytes.as_mut_ptr();
    // SAFETY: all three ranges were bounds-checked against `bytes` and
    // pairwise overlap was rejected, so the returned mutable slices are
    // disjoint views into the same guest memory allocation.
    unsafe {
        Ok((
            std::slice::from_raw_parts_mut(ptr.add(first.start), first.len()),
            std::slice::from_raw_parts_mut(ptr.add(second.start), second.len()),
            std::slice::from_raw_parts_mut(ptr.add(third.start), third.len()),
        ))
    }
}

fn ranges_overlap(first: &std::ops::Range<usize>, second: &std::ops::Range<usize>) -> bool {
    first.start < second.end && second.start < first.end
}

fn guest_range(
    memory_len: usize,
    offset: usize,
    len: u32,
) -> AsyncHostResult<std::ops::Range<usize>> {
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
    if end > memory_len {
        return Err(AsyncHostError::Fault);
    }
    Ok(offset..end)
}

fn read_guest_string(memory: &[u8], ptr: u32, len: u32) -> AsyncHostResult<String> {
    let units = read_u16(memory, ptr, len)?;
    Ok(std::char::decode_utf16(units)
        .map(|unit| unit.unwrap_or(std::char::REPLACEMENT_CHARACTER))
        .collect())
}

fn read_guest_os_path(memory: &[u8], ptr: u32, len: u32) -> AsyncHostResult<PathBuf> {
    let units = read_u16(memory, ptr, len)?;

    #[cfg(unix)]
    {
        use std::ffi::OsString;
        use std::os::unix::ffi::OsStringExt;

        let path = std::char::decode_utf16(units)
            .map(|unit| unit.unwrap_or(std::char::REPLACEMENT_CHARACTER))
            .collect::<String>();
        Ok(PathBuf::from(OsString::from_vec(path.into_bytes())))
    }

    #[cfg(windows)]
    {
        use std::ffi::OsString;
        use std::os::windows::ffi::OsStringExt;

        Ok(PathBuf::from(OsString::from_wide(&units)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_buffers_preserve_error_and_null_conventions() {
        use crate::runtime::{Executable, Runtime, RuntimeConfig};

        let runtime = Runtime::new(
            RuntimeConfig::default(),
            Executable::unavailable(),
            crate::signal_channel().1,
        )
        .unwrap();
        let termination_request = crate::run_termination::TerminationRequest::default();
        let host = runtime.async_host();
        let mut context = ImportContext {
            host,
            filesystem: runtime.filesystem(),
            memory: Err(AsyncHostError::Fault),
            termination_request: &termination_request,
        };
        let tls = new(&mut context);

        // An unconfigured connection reports a TLS error through null and
        // keeps the diagnostic until the guest takes it.
        assert_eq!(peer_certificate(&mut context, tls), Ok(null_handle()));
        assert_eq!(unique_channel_binding(&mut context, tls), Ok(null_handle()));
        assert_eq!(
            server_endpoint_channel_binding(&mut context, tls),
            Ok(null_handle())
        );
        let error = take_error(&mut context, tls).unwrap();
        host.with_c_buffer(error, |buffer| {
            assert_eq!(
                buffer,
                &*error_message_buffer("TLS handle is not configured".to_string())
            );
            Ok(())
        })
        .unwrap();
        host.free_c_buffer(error).unwrap();

        // Taking an empty diagnostic and the legacy global-error import both
        // retain the ABI's fallback, with separately owned buffers.
        for error in [
            take_error(&mut context, tls).unwrap(),
            take_global_error(&mut context),
        ] {
            host.with_c_buffer(error, |buffer| {
                assert_eq!(
                    buffer,
                    &*error_message_buffer("unknown TLS error".to_string())
                );
                Ok(())
            })
            .unwrap();
            host.free_c_buffer(error).unwrap();
        }

        // A configured connection without a peer certificate succeeds with
        // a valid empty buffer; absent channel bindings still return null.
        assert_eq!(
            host.tls()
                .set_client(tls, "localhost", false, TlsTrust::NoVerification),
            Ok(0)
        );
        let certificate = peer_certificate(&mut context, tls).unwrap();
        assert_ne!(certificate, null_handle());
        host.with_c_buffer(certificate, |buffer| {
            assert!(buffer.is_empty());
            Ok(())
        })
        .unwrap();
        host.free_c_buffer(certificate).unwrap();
        assert_eq!(unique_channel_binding(&mut context, tls), Ok(null_handle()));
        assert_eq!(
            server_endpoint_channel_binding(&mut context, tls),
            Ok(null_handle())
        );
        free(&mut context, tls).unwrap();
        free(&mut context, null_handle()).unwrap();
        assert_eq!(free(&mut context, 0), Err(AsyncHostError::Badf));
        assert!(host.leak_summary().is_none());
    }

    #[cfg(unix)]
    #[test]
    fn error_message_buffer_uses_native_string_encoding_on_unix() {
        assert_eq!(error_message_buffer("ab".to_string()).as_ref(), b"ab\0");
    }

    #[cfg(windows)]
    #[test]
    fn error_message_buffer_uses_native_string_encoding_on_windows() {
        assert_eq!(
            error_message_buffer("ab".to_string()).as_ref(),
            &[b'a', 0, b'b', 0, 0, 0]
        );
    }

    #[test]
    fn input_output_split_allows_input_before_output() {
        let mut memory = [0, 1, 2, 3, 4, 5];
        {
            let (input, output) = read_guest_input_output_mut(&mut memory, 1, 2, 4, 2).unwrap();
            assert_eq!(input, &[1, 2]);
            output.copy_from_slice(&[9, 8]);
        }
        assert_eq!(memory, [0, 1, 2, 3, 9, 8]);
    }

    #[test]
    fn input_output_split_allows_output_before_input() {
        let mut memory = [0, 1, 2, 3, 4, 5];
        {
            let (input, output) = read_guest_input_output_mut(&mut memory, 4, 2, 1, 2).unwrap();
            assert_eq!(input, &[4, 5]);
            output.copy_from_slice(&[9, 8]);
        }
        assert_eq!(memory, [0, 9, 8, 3, 4, 5]);
    }

    #[test]
    fn input_output_split_rejects_overlapping_ranges() {
        let mut memory = [0, 1, 2, 3, 4, 5];
        assert_eq!(
            read_guest_input_output_mut(&mut memory, 1, 3, 2, 2).unwrap_err(),
            AsyncHostError::Fault,
        );
    }

    #[test]
    fn buffers3_split_allows_disjoint_ranges() {
        let mut memory = [0, 1, 2, 3, 4, 5, 6, 7];
        {
            let (first, second, third) =
                read_guest_buffers3_mut(&mut memory, 0, 2, 3, 2, 6, 2).unwrap();
            assert_eq!(first, &[0, 1]);
            second.copy_from_slice(&[9, 8]);
            third.copy_from_slice(&[7, 6]);
        }
        assert_eq!(memory, [0, 1, 2, 9, 8, 5, 7, 6]);
    }

    #[test]
    fn buffers3_split_rejects_overlap() {
        let mut memory = [0, 1, 2, 3, 4, 5];
        assert_eq!(
            read_guest_buffers3_mut(&mut memory, 0, 3, 3, 2, 2, 2).unwrap_err(),
            AsyncHostError::Fault,
        );
    }
}
