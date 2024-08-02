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
