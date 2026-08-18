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

//! Runtime-neutral access to one short-lived view of wasm linear memory.
//!
//! Runtime adapters are responsible for acquiring a fresh view for every host
//! call. This interface only defines checked access after that acquisition; it
//! does not retain a runtime memory object or make address zero mean null.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GuestMemoryError {
    OutOfBounds,
}

pub(crate) type GuestMemoryResult<T> = Result<T, GuestMemoryError>;

pub(crate) trait GuestMemory {
    fn bytes(&self) -> &[u8];

    fn bytes_mut(&mut self) -> &mut [u8];

    fn read_exact(&self, offset: u32, len: u32) -> GuestMemoryResult<&[u8]> {
        let (offset, end) = guest_bounds(offset, len)?;
        self.bytes()
            .get(offset..end)
            .ok_or(GuestMemoryError::OutOfBounds)
    }

    fn read_exact_mut(&mut self, offset: u32, len: u32) -> GuestMemoryResult<&mut [u8]> {
        let (offset, end) = guest_bounds(offset, len)?;
        self.bytes_mut()
            .get_mut(offset..end)
            .ok_or(GuestMemoryError::OutOfBounds)
    }

    fn write_exact(&mut self, offset: u32, data: &[u8]) -> GuestMemoryResult<()> {
        let len = u32::try_from(data.len()).map_err(|_| GuestMemoryError::OutOfBounds)?;
        let dst = self.read_exact_mut(offset, len)?;
        dst.copy_from_slice(data);
        Ok(())
    }

    fn write_with_capacity(
        &mut self,
        offset: u32,
        capacity: u32,
        data: &[u8],
    ) -> GuestMemoryResult<()> {
        let data_len = u32::try_from(data.len()).map_err(|_| GuestMemoryError::OutOfBounds)?;
        if data_len > capacity {
            return Err(GuestMemoryError::OutOfBounds);
        }
        let dst = self.read_exact_mut(offset, capacity)?;
        dst[..data.len()].copy_from_slice(data);
        Ok(())
    }

    fn write_u64_le(&mut self, offset: u32, value: u64) -> GuestMemoryResult<()> {
        self.write_exact(offset, &value.to_le_bytes())
    }
}

fn guest_bounds(offset: u32, len: u32) -> GuestMemoryResult<(usize, usize)> {
    let offset = usize::try_from(offset).map_err(|_| GuestMemoryError::OutOfBounds)?;
    let len = usize::try_from(len).map_err(|_| GuestMemoryError::OutOfBounds)?;
    let end = offset
        .checked_add(len)
        .ok_or(GuestMemoryError::OutOfBounds)?;
    Ok((offset, end))
}

impl GuestMemory for [u8] {
    fn bytes(&self) -> &[u8] {
        self
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        self
    }
}

impl<const N: usize> GuestMemory for [u8; N] {
    fn bytes(&self) -> &[u8] {
        self.as_slice()
    }

    fn bytes_mut(&mut self) -> &mut [u8] {
        self.as_mut_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn access_checks_ranges_without_reserving_address_zero() {
        let mut memory = [0_u8; 8];

        memory.write_exact(0, &[1, 2]).unwrap();
        memory.read_exact_mut(2, 2).unwrap().fill(3);

        assert_eq!(memory.read_exact(0, 2).unwrap(), &[1, 2]);
        assert_eq!(memory.read_exact(2, 2).unwrap(), &[3, 3]);
        assert_eq!(
            memory.read_exact(u32::MAX, 1),
            Err(GuestMemoryError::OutOfBounds)
        );
        assert_eq!(
            memory.write_exact(7, &[1, 2]),
            Err(GuestMemoryError::OutOfBounds)
        );
    }

    #[test]
    fn capacity_write_validates_the_whole_output_range() {
        let mut memory = [0_u8; 8];

        memory.write_with_capacity(2, 4, &[1, 2]).unwrap();

        assert_eq!(&memory, &[0, 0, 1, 2, 0, 0, 0, 0]);
        assert_eq!(
            memory.write_with_capacity(6, 4, &[1, 2]),
            Err(GuestMemoryError::OutOfBounds)
        );
    }

    #[test]
    fn fixed_width_writes_use_little_endian() {
        let mut memory = [0_u8; 16];

        memory.write_u64_le(2, 0x1020_3040_5060_7080).unwrap();

        assert_eq!(
            &memory[2..10],
            &[0x80, 0x70, 0x60, 0x50, 0x40, 0x30, 0x20, 0x10]
        );
        assert_eq!(
            memory.write_u64_le(10, 1),
            Err(GuestMemoryError::OutOfBounds)
        );
    }
}
