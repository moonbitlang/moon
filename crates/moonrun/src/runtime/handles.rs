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

use std::cell::RefCell;
use std::rc::Rc;

use slotmap::SecondaryMap;

use super::{HostKey, HostKeys, HostResourceKind};

/// Owns one family's entries and their registrations in the shared namespace.
///
/// Values can represent domain reservations, such as a buffer owned by a Job.
/// Removing an entry retires guest reachability and returns the owned value so
/// callers can release table borrows before performing domain cleanup.
pub(crate) struct Handles<T> {
    keys: Rc<RefCell<HostKeys>>,
    kind: HostResourceKind,
    entries: SecondaryMap<HostKey, T>,
}

impl<T> Handles<T> {
    pub(crate) fn new(keys: Rc<RefCell<HostKeys>>, kind: HostResourceKind) -> Self {
        Self {
            keys,
            kind,
            entries: SecondaryMap::new(),
        }
    }

    pub(crate) fn insert(&mut self, value: T) -> HostKey {
        let key = self.keys.borrow_mut().insert(self.kind);
        let replaced = self.entries.insert(key, value);
        debug_assert!(replaced.is_none());
        key
    }

    pub(crate) fn get(&self, key: HostKey) -> Option<&T> {
        // Families with separate payload tables also mutate HostKeys. Validate
        // central registration as well as membership in this owning table.
        if self.keys.borrow().kind(key) != Some(self.kind) {
            return None;
        }
        self.entries.get(key)
    }

    pub(crate) fn get_mut(&mut self, key: HostKey) -> Option<&mut T> {
        if self.keys.borrow().kind(key) != Some(self.kind) {
            return None;
        }
        self.entries.get_mut(key)
    }

    pub(crate) fn remove(&mut self, key: HostKey) -> Option<T> {
        let mut keys = self.keys.borrow_mut();
        if keys.kind(key) != Some(self.kind) {
            return None;
        }
        let value = self.entries.remove(key)?;
        let removed = keys.remove(key);
        debug_assert_eq!(removed, Some(self.kind));
        Some(value)
    }

    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

impl<T> Drop for Handles<T> {
    fn drop(&mut self) {
        let mut keys = self.keys.borrow_mut();
        for key in self.entries.keys() {
            keys.remove(key);
        }
        // Release the allocator borrow before Rust drops the entry values.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slotmap::Key;
    use std::cell::Cell;

    #[test]
    fn lookup_and_removal_require_membership_in_the_owning_table() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let mut first = Handles::new(keys.clone(), HostResourceKind::CBuffer);
        let mut second = Handles::new(keys.clone(), HostResourceKind::CBuffer);
        let mut other = Handles::new(keys.clone(), HostResourceKind::AddrInfo);
        let a = first.insert(1);
        let b = second.insert(2);
        let c = other.insert(3);

        for key in [HostKey::null(), b, c] {
            assert_eq!(first.get(key), None);
            assert_eq!(first.get_mut(key), None);
            assert_eq!(first.remove(key), None);
        }
        assert_eq!(first.get(a), Some(&1));
        assert_eq!(second.get(b), Some(&2));
        assert_eq!(other.get(c), Some(&3));

        assert_eq!(first.remove(a), Some(1));
        assert_eq!(keys.borrow().kind(a), None);
        let replacement = other.insert(4);
        assert_ne!(a, replacement);
        assert_eq!(first.get(a), None);
        assert_eq!(other.get(a), None);
        assert_eq!(other.remove(a), None);
        assert_eq!(other.get(replacement), Some(&4));
    }

    #[test]
    fn central_invalidation_rejects_an_entry_left_in_the_secondary_map() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let mut entries = Handles::new(keys.clone(), HostResourceKind::CBuffer);
        let key = entries.insert(1);
        keys.borrow_mut().remove(key);

        assert_eq!(entries.get(key), None);
        assert_eq!(entries.get_mut(key), None);
        assert_eq!(entries.remove(key), None);
        let replacement = keys.borrow_mut().insert(HostResourceKind::AddrInfo);
        drop(entries);
        assert_eq!(
            keys.borrow().kind(replacement),
            Some(HostResourceKind::AddrInfo)
        );
    }

    struct ObserveDrop {
        keys: Rc<RefCell<HostKeys>>,
        retired: Rc<RefCell<Vec<HostKey>>>,
        count: Rc<Cell<usize>>,
    }

    impl Drop for ObserveDrop {
        fn drop(&mut self) {
            let mut keys = self.keys.borrow_mut();
            for key in self.retired.borrow().iter() {
                assert_eq!(keys.kind(*key), None);
            }
            let key = keys.insert(HostResourceKind::Job);
            keys.remove(key);
            self.count.set(self.count.get() + 1);
        }
    }

    #[test]
    fn removal_returns_the_value_before_running_its_destructor() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let retired = Rc::new(RefCell::new(Vec::new()));
        let count = Rc::new(Cell::new(0));
        let entries = RefCell::new(Handles::new(keys.clone(), HostResourceKind::CBuffer));
        let key = entries.borrow_mut().insert(ObserveDrop {
            keys,
            retired: retired.clone(),
            count: count.clone(),
        });
        retired.borrow_mut().push(key);

        let value = entries.borrow_mut().remove(key).unwrap();
        assert!(entries.borrow().get(key).is_none());
        assert_eq!(count.get(), 0);
        drop(value);
        assert_eq!(count.get(), 1);
    }

    #[test]
    fn table_drop_retires_all_keys_before_dropping_values() {
        let keys = Rc::new(RefCell::new(HostKeys::default()));
        let retired = Rc::new(RefCell::new(Vec::new()));
        let count = Rc::new(Cell::new(0));
        let mut entries = Handles::new(keys.clone(), HostResourceKind::CBuffer);
        for _ in 0..2 {
            let key = entries.insert(ObserveDrop {
                keys: keys.clone(),
                retired: retired.clone(),
                count: count.clone(),
            });
            retired.borrow_mut().push(key);
        }
        drop(entries);
        assert_eq!(count.get(), 2);
    }
}
