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

use std::iter::Peekable;

pub trait ItersExtension: Iterator {
    fn strip_two_sides<TVal: Clone + Into<Self::Item> + PartialEq<Self::Item>>(
        &mut self,
        v: TVal,
    ) -> StrippedTwoSides<&mut Self, TVal> {
        StrippedTwoSides {
            iterator: self.peekable(),
            at_start: true,
            count: 0,
            val: v,
        }
    }
}

impl<T: Iterator> ItersExtension for T {}

pub struct StrippedTwoSides<It: Iterator, TVal> {
    iterator: Peekable<It>,
    at_start: bool,
    count: usize,
    val: TVal,
}

impl<It: Iterator, TVal: Clone + Into<It::Item> + PartialEq<It::Item>> Iterator
    for StrippedTwoSides<It, TVal>
{
    type Item = It::Item;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            let item = self.iterator.peek();
            match item {
                None => return None,
                Some(item) => {
                    if self.at_start && self.val.eq(item) {
                        // consume the first item
                        let _ = self.iterator.next();
                        continue;
                    } else {
                        if self.at_start {
                            self.at_start = false;
                        }

                        // if the item is the same as the one we expected, skip it but count the number of it
                        if self.val.eq(item) {
                            self.count += 1;
                            let _ = self.iterator.next();
                            continue;
                        } else {
                            // if the item is different from the one we expected, return n items
                            if self.count > 0 {
                                self.count -= 1;
                                return Some(self.val.clone().into());
                            } else {
                                return self.iterator.next();
                            }
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::iters::ItersExtension;

    #[test]
    fn test_strip_two_sides() {
        assert!(
            [1, 1, 2, 3, 4, 5, 9]
                .into_iter()
                .strip_two_sides(1)
                .collect::<Vec<_>>()
                == [2, 3, 4, 5, 9]
        );
    }

    #[test]
    fn test_strip_two_sides_2() {
        assert!(
            [1, 1, 2, 3, 4, 5, 9, 1]
                .into_iter()
                .strip_two_sides(1)
                .collect::<Vec<_>>()
                == [2, 3, 4, 5, 9]
        );
    }

    #[test]
    fn test_strip_two_sides_not_middle_items() {
        assert!(
            [1, 1, 2, 3, 1, 1, 4, 5, 1, 1, 9, 1, 1]
                .into_iter()
                .strip_two_sides(1)
                .collect::<Vec<_>>()
                == [2, 3, 1, 1, 4, 5, 1, 1, 9]
        );
    }
}
