// moon: The build system and package manager for MoonBit.
// Copyright (C) 2024 International Digital Economy Academy
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU Affero General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU Affero General Public License for more details.
//
// You should have received a copy of the GNU Affero General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// For inquiries, you can contact us via e-mail at jichuruanjian@idea.edu.cn.

use std::path::PathBuf;

use crate::async_host::{
    AsyncHostError, AsyncHostResult, GuestMemory, read_u16,
    tls::{TlsFileType, TlsTrust},
};

use super::context::ImportContext;
use super::provenance::ported_imports;

ported_imports! {
#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/new")]
pub(super) fn new(context: &mut ImportContext<'_, '_>) -> u64 {
    context.host.tls_new()
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/set_client")]
pub(super) fn set_client(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    host: i32,
    host_len: i32,
    sni: i32,
    trust_abi: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host_state, memory| {
        let hostname = read_guest_string(memory, host, host_len)?;
        let trust = TlsTrust::from_abi(trust_abi)?;
        host_state.tls_set_client(tls, hostname, sni != 0, trust)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/add_root_certificate")]
pub(super) fn add_root_certificate(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    root: i32,
    root_len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let root = memory.read_exact(root, root_len)?;
        host.tls_add_root_certificate(tls, root)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/set_server_files")]
#[allow(clippy::too_many_arguments)]
pub(super) fn set_server_files(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    private_key_file: i32,
    private_key_file_len: i32,
    private_key_type_abi: i32,
    certificate_file: i32,
    certificate_file_len: i32,
    certificate_type_abi: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let private_key_file = read_guest_os_path(memory, private_key_file, private_key_file_len)?;
        let certificate_file = read_guest_os_path(memory, certificate_file, certificate_file_len)?;
        host.tls_set_server_files(
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
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    pfx_content: i32,
    pfx_content_len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        host.tls_set_server_pfx(
            tls,
            memory.read_exact(pfx_content, pfx_content_len)?.to_vec(),
        )
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/free")]
pub(super) fn free(context: &mut ImportContext<'_, '_>, tls: u64) -> AsyncHostResult<()> {
    context.host.tls_free(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/take_error")]
pub(super) fn take_error(context: &mut ImportContext<'_, '_>, tls: u64) -> AsyncHostResult<u64> {
    context.host.tls_take_error(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/error/take_global")]
pub(super) fn take_global_error(context: &mut ImportContext<'_, '_>) -> u64 {
    context.host.tls_take_global_error()
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/read_plain")]
#[allow(clippy::too_many_arguments)]
pub(super) fn read_plain(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    in_buffer: i32,
    in_buffer_offset: i32,
    in_buffer_len: i32,
    out_buffer: i32,
    out_buffer_offset: i32,
    out_buffer_len: i32,
    plain_buffer: i32,
    plain_buffer_offset: i32,
    plain_buffer_len: i32,
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
        host.tls_read_plain(tls, input, plain, output)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/write_plain")]
#[allow(clippy::too_many_arguments)]
pub(super) fn write_plain(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    in_buffer: i32,
    in_buffer_offset: i32,
    in_buffer_len: i32,
    out_buffer: i32,
    out_buffer_offset: i32,
    out_buffer_len: i32,
    plain_buffer: i32,
    plain_buffer_offset: i32,
    plain_buffer_len: i32,
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
        host.tls_write_plain(tls, input, plain, output)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/connect")]
#[allow(clippy::too_many_arguments)]
pub(super) fn connect(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    in_buffer: i32,
    in_buffer_offset: i32,
    in_buffer_len: i32,
    out_buffer: i32,
    out_buffer_offset: i32,
    out_buffer_len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let (input, output) = read_guest_input_output_mut(
            memory,
            guest_offset(in_buffer, in_buffer_offset)?,
            in_buffer_len,
            guest_offset(out_buffer, out_buffer_offset)?,
            out_buffer_len,
        )?;
        host.tls_connect(tls, input, output)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/accept")]
#[allow(clippy::too_many_arguments)]
pub(super) fn accept(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
    in_buffer: i32,
    in_buffer_offset: i32,
    in_buffer_len: i32,
    out_buffer: i32,
    out_buffer_offset: i32,
    out_buffer_len: i32,
) -> AsyncHostResult<i32> {
    context.with_host_and_memory_mut(|host, memory| {
        let (input, output) = read_guest_input_output_mut(
            memory,
            guest_offset(in_buffer, in_buffer_offset)?,
            in_buffer_len,
            guest_offset(out_buffer, out_buffer_offset)?,
            out_buffer_len,
        )?;
        host.tls_accept(tls, input, output)
    })
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/bytes_read")]
pub(super) fn bytes_read(context: &mut ImportContext<'_, '_>, tls: u64) -> AsyncHostResult<i32> {
    context.host.tls_bytes_read(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/bytes_to_write")]
pub(super) fn bytes_to_write(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
) -> AsyncHostResult<i32> {
    context.host.tls_bytes_to_write(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/wants_read")]
pub(super) fn wants_read(context: &mut ImportContext<'_, '_>, tls: u64) -> AsyncHostResult<i32> {
    context.host.tls_wants_read(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/wants_write")]
pub(super) fn wants_write(context: &mut ImportContext<'_, '_>, tls: u64) -> AsyncHostResult<i32> {
    context.host.tls_wants_write(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/shutdown")]
pub(super) fn shutdown(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
) -> AsyncHostResult<i32> {
    context.host.tls_shutdown(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/peer_certificate")]
pub(super) fn peer_certificate(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
) -> AsyncHostResult<u64> {
    context.host.tls_peer_certificate(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/unique_channel_binding")]
pub(super) fn unique_channel_binding(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
) -> AsyncHostResult<u64> {
    context.host.tls_unique_channel_binding(tls)
}

#[ported(source = "src/tls/tls.wasm.mbt", original = "tls/connection/server_endpoint_channel_binding")]
pub(super) fn server_endpoint_channel_binding(
    context: &mut ImportContext<'_, '_>,
    tls: u64,
) -> AsyncHostResult<u64> {
    context.host.tls_server_endpoint_channel_binding(tls)
}
}

fn guest_offset(ptr: i32, offset: i32) -> AsyncHostResult<i32> {
    if offset < 0 {
        return Err(AsyncHostError::Fault);
    }
    ptr.checked_add(offset).ok_or(AsyncHostError::Fault)
}

fn read_guest_input_output_mut(
    memory: &mut (impl GuestMemory + ?Sized),
    input_offset: i32,
    input_len: i32,
    output_offset: i32,
    output_len: i32,
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
    first_offset: i32,
    first_len: i32,
    second_offset: i32,
    second_len: i32,
    third_offset: i32,
    third_len: i32,
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
    offset: i32,
    len: i32,
) -> AsyncHostResult<std::ops::Range<usize>> {
    let offset = usize::try_from(offset).map_err(|_| AsyncHostError::Fault)?;
    let len = usize::try_from(len).map_err(|_| AsyncHostError::Fault)?;
    let end = offset.checked_add(len).ok_or(AsyncHostError::Fault)?;
    if end > memory_len {
        return Err(AsyncHostError::Fault);
    }
    Ok(offset..end)
}

fn read_guest_string(memory: &[u8], ptr: i32, len: i32) -> AsyncHostResult<String> {
    let units = read_u16(memory, ptr, len)?;
    Ok(std::char::decode_utf16(units)
        .map(|unit| unit.unwrap_or(std::char::REPLACEMENT_CHARACTER))
        .collect())
}

fn read_guest_os_path(memory: &[u8], ptr: i32, len: i32) -> AsyncHostResult<PathBuf> {
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
pub(super) fn ported_symbols() -> Vec<crate::async_sys::PortedSymbol> {
    PORTED_IMPORTS
        .iter()
        .flat_map(|import| {
            import
                .sources
                .iter()
                .map(move |source| crate::async_sys::PortedSymbol {
                    rust_module: import.rust_module,
                    rust_symbol: import.rust_symbol,
                    native_symbol: import
                        .native_symbol
                        .expect("TLS ABI provenance uses explicit wasm import symbols"),
                    source: source.path,
                })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
