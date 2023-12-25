#![feature(map_try_insert)]
#![no_std]

extern crate alloc;
use alloc::collections::BTreeMap;
use core::cmp::Ordering;
use core::ops::Range;

pub struct RangedBTreeMap<K, V> {
    inner: BTreeMap<KeyType<K>, V>
}

impl<K, V> RangedBTreeMap<K, V> {
    pub fn new() -> Self {
        Self {
            inner: BTreeMap::new()
        }
    }
}

impl<K, V> RangedBTreeMap<K, V> where K: Ord {
    fn insert(&mut self, range: Range<K>, value: V) -> Result<(), ()> {
        let res = self.inner.try_insert(KeyType::Range(range), value);
        res.map(|_| ()).map_err(|_| ())
    }

    fn get_entry_at_point(&self, point: K) -> Option<&V> {
        self.inner.get(&KeyType::Point(point))
    }
}

enum KeyType<T> {
    Range(Range<T>),
    Point(T)
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
        match (self, other) {
            (KeyType::Point(a), KeyType::Point(b)) => a.cmp(b),
            (KeyType::Point(a), KeyType::Range(b)) => {
                if b.start > *a { Ordering::Less }
                else if b.end <= *a { Ordering::Greater }
                else { Ordering::Equal }
            },
            (KeyType::Range(_), KeyType::Point(_)) => other.cmp(self).reverse(),
            (KeyType::Range(a), KeyType::Range(b)) => {
                if a.start >= b.end { Ordering::Greater }
                else if a.end <= b.start { Ordering::Less }
                else { Ordering::Equal }
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
        b.insert(0u8..5, "foo").unwrap();
        b.insert(5u8..u8::MAX, "bar").unwrap();
        assert_eq!(*b.get_entry_at_point(3).unwrap(), "foo");
        assert_eq!(*b.get_entry_at_point(5).unwrap(), "bar");
        assert_eq!(*b.get_entry_at_point(58).unwrap(), "bar");
    }

    #[test]
    fn cannot_insert_overlapping_range() {
        let mut b = RangedBTreeMap::new();
        b.insert(0u8..5, "foo").unwrap();
        b.insert(3u8..u8::MAX, "bar").unwrap_err();
    }

    #[test]
    fn cannot_retrieve_outside_range() {
        let mut b = RangedBTreeMap::new();
        b.insert(6u8..34, "foo").unwrap();
        assert_eq!(b.get_entry_at_point(3), None);
        assert_eq!(b.get_entry_at_point(34), None);
        assert_eq!(b.get_entry_at_point(u8::MAX), None);
    }
}
