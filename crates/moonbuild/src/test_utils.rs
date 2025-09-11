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

//! Test utilities for handling test indices and ranges

/// Convert a sorted list of indices into efficient ranges
/// Example: [0, 1, 2, 5, 6, 10] -> [(0..3), (5..7), (10..11)]
///
/// This function is used mainly for fixing the problem where `moon test` don't
/// have the correct indices set if the test numbering is non-contiguous,
/// possible because some tests are skipped.
pub fn indices_to_ranges(mut indices: Vec<u32>) -> Vec<std::ops::Range<u32>> {
    if indices.is_empty() {
        return vec![];
    }

    indices.sort_unstable();
    let mut ranges = vec![];
    let mut start = indices[0];
    let mut end = indices[0] + 1;

    for &index in &indices[1..] {
        if index == end {
            // Consecutive index, extend current range
            end = index + 1;
        } else {
            // Gap found, close current range and start new one
            ranges.push(start..end);
            start = index;
            end = index + 1;
        }
    }

    // Push the final range
    ranges.push(start..end);
    ranges
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_indices_to_ranges() {
        // Test empty input
        assert_eq!(indices_to_ranges(vec![]), vec![]);

        // Test single index
        assert_eq!(indices_to_ranges(vec![5]), vec![5..6]);

        // Test contiguous range
        assert_eq!(indices_to_ranges(vec![0, 1, 2, 3]), vec![0..4]);

        // Test non-contiguous indices (the main problem we're solving)
        assert_eq!(
            indices_to_ranges(vec![0, 2, 5, 6, 7, 10]),
            vec![0..1, 2..3, 5..8, 10..11]
        );

        // Test unsorted input (should still work)
        assert_eq!(
            indices_to_ranges(vec![10, 0, 7, 5, 6, 2]),
            vec![0..1, 2..3, 5..8, 10..11]
        );

        // Test large gap
        assert_eq!(
            indices_to_ranges(vec![1, 100, 101, 1000]),
            vec![1..2, 100..102, 1000..1001]
        );
    }
}
