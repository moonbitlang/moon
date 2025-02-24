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

use std::hash::{Hash, Hasher};

use twox_hash::xxh3;

/// A 64-bit stable hash of the given data.
pub fn short_hash(data: impl Hash) -> u64 {
    let mut hasher = xxh3::Hash64::with_seed(0);
    data.hash(&mut hasher);
    hasher.finish()
}

/// A 16-character hexadecimal representation of the hash of the given data.
pub fn short_hash_str(data: impl Hash) -> String {
    format!("{:016x}", short_hash(data))
}
