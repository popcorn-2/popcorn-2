//! An ordered map based on a B-Tree, where each key can span a range of values.
//!
//! Values are inserted by associating them with a [`Range`], and are looked up in the map
//! by a point within the range.
//!
//! # Examples
//!
//! ```
//! use ranged_btree::RangedBTreeMap;
//! use std::net::Ipv4Addr;
//!
//! let mut routing_table = RangedBTreeMap::new();
//!
//! // add some entries to the routing table
//! routing_table.insert(Ipv4Addr::new(127, 0, 0, 0)..Ipv4Addr::new(127, 255, 255, 255), Ipv4Addr::new(172, 0, 0, 0));
//! routing_table.insert(Ipv4Addr::new(192, 168, 1, 0)..Ipv4Addr::new(192, 168, 1, 255), Ipv4Addr::new(192, 168, 1, 1));
//!
//! // check if we can route a specific IP
//! if routing_table.get_entry_at_point(Ipv4Addr::new(8, 8, 8, 8)).is_none() {
//!     println!("can't find route for 8.8.8.8");
//! }
//!
//! // iterate over all gateways
//! for (range, gateway) in &routing_table {
//!     println!("route {} through {} to {}", range.start, range.end, gateway);
//! }
//! ```

#![feature(map_try_insert)]
#![feature(impl_trait_in_assoc_type)]
#![feature(strict_provenance_lints)]
#![cfg_attr(doc, feature(rustdoc_missing_doc_code_examples))]
#![forbid(unsafe_code)]
#![no_std]

extern crate alloc;

use alloc::collections::BTreeMap;
use core::cmp::Ordering;
use core::fmt;
use core::ops::Range;

/// A B-Tree which uses ranges as keys and allows lookup by points.
///
/// See the [crate-level documentation](crate) for more information.
#[expect(rustdoc::missing_doc_code_examples, reason = "example in crate level docs")]
pub struct RangedBTreeMap<K, V> {
    inner: BTreeMap<KeyType<K>, V>
}

impl<K, V> RangedBTreeMap<K, V> {
    /// Creates a new, empty `RangedBTreeMap`.
    ///
    /// The map will not allocate until elements are inserted.
    ///
    /// # Examples
    ///
    /// ```
    /// use ranged_btree::RangedBTreeMap;
    ///
    /// let map = RangedBTreeMap::<u32, i32>::new();
    /// ```
    #[must_use]
    pub const fn new() -> Self {
        Self {
            inner: BTreeMap::new()
        }
    }

    /// Gets an iterator over the entries of thr map, sorted by key.
    ///
    /// # Examples
    ///
    /// ```
    /// use ranged_btree::RangedBTreeMap;
    ///
    /// let mut map = RangedBTreeMap::new();
    /// map.insert(100..1000, "large");
    /// map.insert(10..100, "medium");
    /// map.insert(0..10, "small");
    ///
    /// let (first_key, first_value) = map.iter().next().unwrap();
    /// assert_eq!(first_key, &(0..10));
    /// assert_eq!(first_value, &"small");
    /// ```
    pub fn iter(&self) -> impl Iterator<Item=(&Range<K>, &V)> {
        self.inner.iter()
            .map(|(k, v)| (k.as_range(), v))
    }

    /// Gets a mutable iterator over the entries of the map, sorted by key.
    ///
    /// # Examples
    ///
    /// ```
    /// use ranged_btree::RangedBTreeMap;
    ///
    /// let mut map = RangedBTreeMap::new();
    /// map.insert(0..10, String::from("small"));
    /// map.insert(10..100, String::from("medium"));
    /// map.insert(100..1000, String::from("large"));
    ///
    /// // make value uppercase if not "small"
    /// for (range, value) in map.iter_mut() {
    ///     if *value != "small" {
    ///         value.make_ascii_uppercase();
    ///     }
    /// }
    /// ```
    pub fn iter_mut(&mut self) -> impl Iterator<Item=(&Range<K>, &mut V)> {
        self.inner.iter_mut()
            .map(|(k, v)| (k.as_range(), v))
    }
}

impl<K, V> RangedBTreeMap<K, V> where K: Ord {
    /// Inserts an item into the map, associating it with `range`.
    ///
    /// If `range` already exists in the map, it will not be overwritten
    /// and the value will *not* be modified.
    ///
    /// # Errors
    ///
    /// Returns [`InsertionError::OverlappingRange`] if `range` overlaps with a range
    /// already in the map. Returns [`InsertionError::AlreadyExists`] if `range` is already
    /// present in the map.
    ///
    /// # Examples
    ///
    /// ```
    /// # use core::assert_matches;
    /// use ranged_btree::{RangedBTreeMap, InsertionError};
    ///
    /// let mut map = RangedBTreeMap::new();
    /// assert_matches!(map.insert(1..37, "a"), Ok(_));
    /// assert_eq!(map.is_empty(), false);
    ///
    /// map.insert(37..39, "b");
    /// assert_matches!(map.insert(37..39, "c"), Err(InsertionError::AlreadyExists { .. }));
    /// assert_eq!(map.get_entry_at_point(38).unwrap(), &"b");
    /// ```
    pub fn insert(&mut self, range: Range<K>, value: V) -> Result<&mut V, InsertionError<V>> {
        let range = KeyType::Range(range);
        // the weird way to insert here is so we can hold onto ownership of `range` since the
        // entry API has no way to retrieve it if the map already contained an equivalent one
        if let Some((k, _)) = self.inner.get_key_value(&range) {
            if k.as_range() == range.as_range() {
                Err(InsertionError::AlreadyExists { attempted_value: value })
            } else {
                Err(InsertionError::OverlappingRange { attempted_value: value })
            }
        } else {
            #[expect(clippy::missing_panics_doc, reason = "infallible")]
            Ok(self.inner.try_insert(range, value).unwrap_or_else(|_| panic!("already checked if key is present")))
        }
    }

    /// Returns a reference to the value mapped to the range containing `point`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ranged_btree::RangedBTreeMap;
    ///
    /// let mut map = RangedBTreeMap::new();
    /// map.insert(1..2, "a");
    /// assert_eq!(map.get_entry_at_point(1), Some(&"a"));
    /// assert_eq!(map.get_entry_at_point(2), None);
    /// ```
    pub fn get_entry_at_point(&self, point: K) -> Option<&V> {
        self.inner.get(&KeyType::Point(point))
    }

    /// Returns a mutable reference to the value mapped to the range containing `point`.
    ///
    /// # Examples
    ///
    /// ```
    /// use ranged_btree::RangedBTreeMap;
    ///
    /// let mut map = RangedBTreeMap::new();
    /// map.insert(1..2, "a");
    /// if let Some(x) = map.get_entry_at_point_mut(1) {
    ///     *x = "b";
    /// }
    /// assert_eq!(map.get_entry_at_point(1).unwrap(), &"b");
    /// ```
    pub fn get_entry_at_point_mut(&mut self, point: K) -> Option<&mut V> {
        self.inner.get_mut(&KeyType::Point(point))
    }

    /// Returns an iterator over all the ranges occupied in the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use ranged_btree::RangedBTreeMap;
    ///
    /// let mut map = RangedBTreeMap::new();
    /// map.insert(1..5, "a");
    /// map.insert(8..10, "b");
    ///
    /// let mut regions = map.used_regions();
    /// assert_eq!(regions.next(), Some(1..5).as_ref());
    /// assert_eq!(regions.next(), Some(8..10).as_ref());
    /// assert_eq!(regions.next(), None);
    /// ```
    pub fn used_regions(&self) -> impl Iterator<Item = &Range<K>> + '_ {
        self.inner.keys()
                .map(KeyType::as_range)
    }

    /// Returns the start of the first range in the map, or [`None`] if the map is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use ranged_btree::RangedBTreeMap;
    ///
    /// let mut map = RangedBTreeMap::new();
    /// assert_eq!(map.first_key(), None);
    ///
    /// map.insert(8..10, "b");
    /// assert_eq!(map.first_key(), Some(8).as_ref());
    ///
    /// map.insert(1..5, "a");
    /// assert_eq!(map.first_key(), Some(1).as_ref());
    /// ```
    #[must_use]
    pub fn first_key(&self) -> Option<&K> {
        self.inner.first_key_value()
            .map(|(k, _)| &k.as_range().start)
    }

    /// Returns the end of the last range in the map, or [`None`] if the map is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use ranged_btree::RangedBTreeMap;
    ///
    /// let mut map = RangedBTreeMap::new();
    /// assert_eq!(map.last_key(), None);
    ///
    /// map.insert(1..5, "a");
    /// assert_eq!(map.last_key(), Some(5).as_ref());
    ///
    /// map.insert(8..10, "b");
    /// assert_eq!(map.last_key(), Some(10).as_ref());
    /// ```
    #[must_use]
    pub fn last_key(&self) -> Option<&K> {
        self.inner.last_key_value()
                .map(|(k, _)| &k.as_range().end)
    }

    /// Returns `true` if the map is empty.
    ///
    /// # Examples
    ///
    /// ```
    /// use ranged_btree::RangedBTreeMap;
    ///
    /// let mut map = RangedBTreeMap::new();
    /// assert_eq!(map.is_empty(), true);
    ///
    /// map.insert(1..5, "a");
    /// assert_eq!(map.is_empty(), false);
    /// ```
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Removes the key with the range containing `point`, returning the removed value if it exists.
    ///
    /// # Examples
    ///
    /// ```
    /// use ranged_btree::RangedBTreeMap;
    ///
    /// let mut map = RangedBTreeMap::new();
    /// map.insert(1..5, "a");
    ///
    /// assert_eq!(map.remove(8), None);
    /// assert_eq!(map.remove(3), Some("a"));
    /// ```
    pub fn remove(&mut self, point: K) -> Option<V> {
        self.inner.remove(&KeyType::Point(point))
    }
}

impl<K: fmt::Debug, V: fmt::Debug> fmt::Debug for RangedBTreeMap<K, V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_map()
         .entries(self)
         .finish()
    }
}

impl<K, V> IntoIterator for RangedBTreeMap<K, V> {
    type Item = (Range<K>, V);
    type IntoIter = impl Iterator<Item=(Range<K>, V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.inner.into_iter()
                .map(|(k, v)| (k.into_range(), v))
    }
}

impl<'a, K, V> IntoIterator for &'a RangedBTreeMap<K, V> {
    type Item = (&'a Range<K>, &'a V);
    type IntoIter = impl Iterator<Item=(&'a Range<K>, &'a V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, K, V> IntoIterator for &'a mut RangedBTreeMap<K, V> {
    type Item = (&'a Range<K>, &'a mut V);
    type IntoIter = impl Iterator<Item=(&'a Range<K>, &'a mut V)>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

#[derive(Debug)]
enum KeyType<T> {
    Range(Range<T>),
    Point(T)
}

impl<T> KeyType<T> {
    /// # Panics
    ///
    /// If the `KeyType` is not a `Range`.
    fn as_range(&self) -> &Range<T> {
        match self {
            Self::Range(range) => range,
            Self::Point(_) => panic!("called `as_range` on a point variant"),
        }
    }

    /// # Panics
    ///
    /// If the `KeyType` is not a `Range`.
    fn into_range(self) -> Range<T> {
        match self {
            Self::Range(range) => range,
            Self::Point(_) => panic!("called `into_range` on a point variant"),
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
        match (self, other) {
            (Self::Point(lhs), Self::Point(rhs)) => lhs.cmp(rhs),
            (Self::Point(lhs), Self::Range(rhs)) => {
                if rhs.start > *lhs { Ordering::Less }
                else if rhs.end <= *lhs { Ordering::Greater }
                else { Ordering::Equal }
            },
            (Self::Range(_), Self::Point(_)) => other.cmp(self).reverse(),
            (Self::Range(lhs), Self::Range(rhs)) => {
                if lhs.start >= rhs.end { Ordering::Greater }
                else if lhs.end <= rhs.start { Ordering::Less }
                else { Ordering::Equal }
            }
        }
    }
}

impl<K, V> Default for RangedBTreeMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

/// The error type returned when an item couldn't be inserted.
#[expect(rustdoc::missing_doc_code_examples, reason = "doesn't make sense to have example")]
#[derive(Debug)]
pub enum InsertionError<V> {
    /// The range already existed in the map.
    AlreadyExists {
        /// The value that was passed to the `insert()` call.
        attempted_value: V,
    },
    /// The range overlapped with another range in the map.
    OverlappingRange {
        /// The value that was passed to the `insert()` call.
        attempted_value: V,
    },
}

impl<V> fmt::Display for InsertionError<V> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyExists { .. } => write!(f, "range already exists in map"),
            Self::OverlappingRange { .. } => write!(f, "range overlaps range already in map"),
        }
    }
}

impl<V: fmt::Debug> core::error::Error for InsertionError<V> {}

#[cfg(test)]
mod tests {
    use super::*;
    use core::assert_matches;

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
        assert_matches!(b.insert(3u8..u8::MAX, "bar"), Err(InsertionError::OverlappingRange { .. }));
    }

    #[test]
    fn cannot_insert_identical_range() {
        let mut b = RangedBTreeMap::new();
        b.insert(0u8..5, "foo").unwrap();
        assert_matches!(b.insert(0u8..5, "bar"), Err(InsertionError::AlreadyExists { .. }));
    }

    #[test]
    fn cannot_retrieve_outside_range() {
        let mut b = RangedBTreeMap::new();
        b.insert(6u8..34, "foo").unwrap();
        assert_eq!(b.get_entry_at_point(3), None);
        assert_eq!(b.get_entry_at_point(34), None);
        assert_eq!(b.get_entry_at_point(u8::MAX), None);
    }

    #[test]
    fn point_keys_are_equal() {
        assert_eq!(KeyType::Point(5), KeyType::Point(5));
        assert_eq!(KeyType::Point(u8::MAX), KeyType::Point(u8::MAX));
        assert_ne!(KeyType::Point(u8::MAX), KeyType::Point(u8::MIN));
    }

    #[test]
    fn range_keys_are_equal_with_contained_points() {
        assert_eq!(KeyType::Range(1..10), KeyType::Point(5));
        assert_eq!(KeyType::Point(5), KeyType::Range(1..10));
        assert_ne!(KeyType::Range(1..10), KeyType::Point(10));
    }

    #[test]
    fn overlapping_ranges_are_equal() {
        assert_eq!(KeyType::Range(1..10), KeyType::Range(8..10));
        assert_eq!(KeyType::Range(1..10), KeyType::Range(8..50));
        assert_ne!(KeyType::Range(1..10), KeyType::Range(10..10));
        assert_ne!(KeyType::Range(1..10), KeyType::Range(50..10));
        assert_ne!(KeyType::Range(1..10), KeyType::Range(50..80));
    }

    #[test]
    fn point_cmp_range() {
        assert!(KeyType::Point(5) < KeyType::Range(6..10));
        assert!(KeyType::Point(8) > KeyType::Range(2..8));
        assert!(KeyType::Point(6) >= KeyType::Range(2..8));
    }

    #[test]
    fn range_cmp_range() {
        assert!(KeyType::Range(1..6) < KeyType::Range(6..10));
        assert!(KeyType::Range(1..4) < KeyType::Range(6..10));
        assert!(KeyType::Range(50..10) > KeyType::Range(6..10));
        assert!(KeyType::Range(50..100) > KeyType::Range(1..49));
    }
}
