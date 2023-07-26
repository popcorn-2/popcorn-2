#![no_std]

extern crate alloc;
use alloc::collections::BTreeMap;
use core::cmp::Ordering;
use core::ops;

pub struct RangedBTreeMap<K, V> where K: Ord {
    inner: BTreeMap<KeyType<K>, V>
}

impl<K, V> RangedBTreeMap<K, V> where K: Ord {
    pub fn new() -> Self {
        Self {
            inner: BTreeMap::new()
        }
    }

    fn insert(&mut self, range: impl Into<Range<K>>, value: V) {
        self.inner.insert(KeyType::Range(range.into()), value);
    }

    fn get_entry_at_point(&self, point: K) -> &V {
        self.inner.get(&KeyType::Point(point));
        todo!()
    }
}

enum KeyType<T> where T: Ord {
    Range(Range<T>),
    Point(T)
}

enum Bound<T> {
    Inclusive(T),
    Exclusive(T),
    Unbounded
}

struct Range<T>  {
    start: Bound<T>,
    end: Bound<T>
}

impl<T> Range<T> where T: Ord {
    fn compare_point(&self, rhs: &T) -> Ordering {
        let above_start = match &self.start {
            Bound::Inclusive(v) => rhs >= v,
            Bound::Exclusive(v) => rhs > v,
            Bound::Unbounded => true
        };

        let below_end = match &self.end {
            Bound::Inclusive(v) => rhs <= v,
            Bound::Exclusive(v) => rhs < v,
            Bound::Unbounded => true
        };

        match (above_start, below_end) {
            (true, true) => Ordering::Equal,
            (false, _) => Ordering::Less,
            (_, false) => Ordering::Greater
        }
    }
}

impl<T> From<ops::Range<T>> for Range<T> {
    fn from(value: ops::Range<T>) -> Self {
        Self {
            start: Bound::Inclusive(value.start),
            end: Bound::Exclusive(value.end)
        }
    }
}

impl<T> From<ops::RangeFrom<T>> for Range<T> {
    fn from(value: ops::RangeFrom<T>) -> Self {
        Self {
            start: Bound::Inclusive(value.start),
            end: Bound::Unbounded
        }
    }
}

impl<T> From<ops::RangeTo<T>> for Range<T> {
    fn from(value: ops::RangeTo<T>) -> Self {
        Self {
            start: Bound::Unbounded,
            end: Bound::Exclusive(value.end)
        }
    }
}

impl<T> PartialEq for KeyType<T> where T: Ord {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl<T> PartialOrd for KeyType<T> where T: Ord {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl<T> Eq for KeyType<T> where T: Ord {}

impl<T> Ord for KeyType<T> where T: Ord {
    fn cmp(&self, other: &Self) -> Ordering {
        assert_ne!(core::mem::discriminant(self), core::mem::discriminant(other));
        match self {
            KeyType::Range(_) => other.cmp(self),
            KeyType::Point(val) => {
                let KeyType::Range(cmp_range) = other else { unreachable!() };
                cmp_range.compare_point(val)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_and_retrieve() {
        let mut b = RangedBTreeMap::new();
        b.insert(0u8..5, "foo");
        b.insert(5u8.., "bar");
        assert_eq!(*b.get_entry_at_point(3), "foo");
        assert_eq!(*b.get_entry_at_point(58), "bar");
    }
}
