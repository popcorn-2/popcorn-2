use alloc::boxed::Box;
use core::fmt::{Debug, Formatter};
use core::marker::PhantomData;
use core::ptr::NonNull;

/// A singly linked list that owns its items
pub struct LinkedList<T> {
    pub first: Option<NonNull<Node<T>>>
}

impl<T: Debug> Debug for LinkedList<T> {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        let mut builder = f.debug_list();
        for item in self.into_iter() {
            builder.entry(item);
        }
        builder.finish()
    }
}

#[derive(Debug)]
pub struct Node<T> {
    pub next: Option<NonNull<Self>>,
    pub data: T
}

impl<T> LinkedList<T> {
    /// Creates a new empty list
    pub const fn new() -> Self {
        Self { first: None }
    }

    /// Adds a new item onto the front of the list
    pub fn push_front(&mut self, val: T) {
        self.push_front_impl(val);
    }

    fn push_front_impl(&mut self, val: T) -> NonNull<Node<T>> {
        let node = Node::new_boxed(val, self.first.take());
        self.first = Some(node);
        node
    }

    /// Removes and returns the first item from the list
    pub fn pop_front(&mut self) -> Option<T> {
        let element = self.pop_front_in_place()?;
        let node = *unsafe { Box::from_raw(element.as_ptr()) };
        Some(node.data)
    }

    pub fn pop_front_in_place(&mut self) -> Option<NonNull<Node<T>>> {
        let element = self.first.take()?;
        // SAFETY: pointer originally came from a Box so must be aligned and dereferenceable
        // Mutable reference to linked list required to be able to call `pop_front` so cannot
        // currently have any other references to nodes
        self.first = unsafe { element.as_ref().next };
        Some(element)
    }

    fn last(&mut self) -> Option<NonNull<Node<T>>> {
        let mut node = self.first?;
        while let Some(next_node) = unsafe { node.as_ref().next } {
            node = next_node;
        }
        Some(node)
    }
}

impl<T> Default for LinkedList<T> {
    /// Creates a new empty list
    fn default() -> Self {
        Self::new()
    }
}

impl<T> FromIterator<T> for LinkedList<T> {
    /// Creates a linked list from the provided iterator, preserving the order of the iterator.
    /// This means that calling [`LinkedList::pop_front()`] will return the same item as would be returned by [`Iterator::next()`].
    fn from_iter<I: IntoIterator<Item=T>>(iter: I) -> Self {
        let mut list = Self::new();
        list.extend(iter);
        list
    }
}

impl<T> Extend<T> for LinkedList<T> {
    /// Appends the provided iterator to the back of the linked list, preserving the order of the iterator
    fn extend<I: IntoIterator<Item=T>>(&mut self, iter: I) {
        let mut iter = iter.into_iter();

        let mut last_node = match (iter.next(), self.last()) {
            (None, _) => return,
            (Some(item), None) => {
                // No items in list yet so pushing to front is same as pushing to back
                self.push_front_impl(item)
            },
            (Some(item), Some(mut last_node)) => {
                unsafe { last_node.as_mut() }.append(item)
            }
        };

        for item in iter {
            last_node = unsafe { last_node.as_mut() }.append(item);
        }
    }
}

impl<T> Drop for LinkedList<T> {
    fn drop(&mut self) {
        while self.pop_front().is_some() {}
    }
}

impl<T> From<Option<NonNull<Node<T>>>> for LinkedList<T> {
    fn from(value: Option<NonNull<Node<T>>>) -> Self {
        Self {
            first: value
        }
    }
}

impl<T> Node<T> {
    pub const fn new(data: T, next: Option<NonNull<Self>>) -> Self {
        Self {
            next,
            data
        }
    }

    fn new_boxed(data: T, next: Option<NonNull<Self>>) -> NonNull<Self> {
        let node = Node::new(data, next);
        let node = Box::into_raw(Box::new(node));
        // SAFETY: Box can't return a NULL pointer
        unsafe { NonNull::new_unchecked(node) }
    }

    fn append(&mut self, data: T) -> NonNull<Self> {
        let node = Node::new_boxed(data, None);
        self.next = Some(node);
        node
    }
}

pub struct Iter<'list, T: 'list> {
    current_element: Option<NonNull<Node<T>>>,
    _phantom: PhantomData<&'list Node<T>>
}

pub struct IterMut<'list, T: 'list> {
    current_element: Option<NonNull<Node<T>>>,
    _phantom: PhantomData<&'list mut Node<T>>
}

impl<'list, T: 'list> Iterator for Iter<'list, T> {
    type Item = &'list T;

    fn next(&mut self) -> Option<Self::Item> {
        let (data, next_item) = self.current_element.map(|elem| unsafe {
            // SAFETY: Same as `pop_front` but with shared instead of mutable references
            let elem = elem.as_ref();
            (&elem.data, elem.next)
        })?;

        self.current_element = next_item;

        Some(data)
    }
}

impl<'list, T: 'list> Iterator for IterMut<'list, T> {
    type Item = &'list mut T;

    fn next(&mut self) -> Option<Self::Item> {
        let (data, next_item) = self.current_element.map(|mut elem| unsafe {
            // SAFETY: same as `pop_front`
            let elem = elem.as_mut();
            (&mut elem.data, elem.next)
        })?;

        self.current_element = next_item;

        Some(data)
    }
}

impl<'list, T> IntoIterator for &'list LinkedList<T> {
        type Item = &'list T;
        type IntoIter = Iter<'list, T>;

        fn into_iter(self) -> Self::IntoIter {
        Iter {
            current_element: self.first,
            _phantom: PhantomData
        }
    }
}

impl<'list, T> IntoIterator for &'list mut LinkedList<T> {
        type Item = &'list mut T;
        type IntoIter = IterMut<'list, T>;

        fn into_iter(self) -> Self::IntoIter {
        IterMut {
            current_element: self.first,
            _phantom: PhantomData
        }
    }
}

#[cfg(test)]
mod tests {
    use super::LinkedList;

    #[test]
    fn pop_empty_list() {
        let mut list = LinkedList::<u8>::default();
        assert_eq!(list.pop_front(), None);
    }

    #[test]
    fn push_then_pop() {
        let mut list = LinkedList::<u8>::default();
        list.push_front(6);
        assert_eq!(list.pop_front(), Some(6));
        assert_eq!(list.pop_front(), None);
    }

    #[test]
    fn stress() {
        let mut list = LinkedList::<u8>::default();
        list.push_front(7);
        list.push_front(10);
        list.push_front(16);
        list.push_front(13);
        list.push_front(19);
        assert_eq!(list.pop_front(), Some(19));
        assert_eq!(list.pop_front(), Some(13));
        assert_eq!(list.pop_front(), Some(16));
        list.push_front(98);
        list.push_front(3);
        list.push_front(1);
        assert_eq!(list.pop_front(), Some(1));
        assert_eq!(list.pop_front(), Some(3));
        list.push_front(5);
        assert_eq!(list.pop_front(), Some(5));
        assert_eq!(list.pop_front(), Some(98));
        assert_eq!(list.pop_front(), Some(10));
        assert_eq!(list.pop_front(), Some(7));
        assert_eq!(list.pop_front(), None);
    }

    #[test]
    fn into_iter() {
        let mut list = LinkedList::default();
        list.push_front(7);
        list.push_front(10);
        list.push_front(16);
        list.push_front(13);
        list.push_front(19);
        let mut iterator = list.into_iter();
        assert_eq!(iterator.next().copied(), Some(19));
        assert_eq!(iterator.next().copied(), Some(13));
        assert_eq!(iterator.next().copied(), Some(16));
        assert_eq!(iterator.next().copied(), Some(10));
        assert_eq!(iterator.next().copied(), Some(7));
        assert_eq!(iterator.next().copied(), None);
    }

    #[test]
    fn mutate() {
        let mut list = LinkedList::default();
        list.push_front(7);
        list.push_front(10);
        list.push_front(16);
        list.push_front(13);
        list.push_front(19);

        let i = IntoIterator::into_iter(&mut list).nth(1).unwrap();
        assert_eq!(*i, 13);
        *i = 98;

        assert_eq!(list.pop_front(), Some(19));
        assert_eq!(list.pop_front(), Some(98));
        assert_eq!(list.pop_front(), Some(16));
        assert_eq!(list.pop_front(), Some(10));
        assert_eq!(list.pop_front(), Some(7));
        assert_eq!(list.pop_front(), None);
    }

    #[test]
    fn from_iterator() {
        let data = [1, 2, 3, 4, 5];
        let mut list = data.into_iter().collect::<LinkedList<_>>();
        assert_eq!(list.pop_front(), Some(1));
        assert_eq!(list.pop_front(), Some(2));
        assert_eq!(list.pop_front(), Some(3));
        assert_eq!(list.pop_front(), Some(4));
        assert_eq!(list.pop_front(), Some(5));
        assert_eq!(list.pop_front(), None);
    }

    #[test]
    fn extend_from_empty_iter() {
        let data = [];
        let mut list = LinkedList::default();
        list.push_front(3);
        list.extend(data.into_iter());
        assert_eq!(list.pop_front(), Some(3));
        assert_eq!(list.pop_front(), None);
    }

    #[test]
    fn extend_non_empty_list() {
        let data = [1, 2, 3];
        let mut list = LinkedList::default();
        list.push_front(5);
        list.push_front(6);
        list.push_front(7);
        list.extend(data.into_iter());
        assert_eq!(list.pop_front(), Some(7));
        assert_eq!(list.pop_front(), Some(6));
        assert_eq!(list.pop_front(), Some(5));
        assert_eq!(list.pop_front(), Some(1));
        assert_eq!(list.pop_front(), Some(2));
        assert_eq!(list.pop_front(), Some(3));
        assert_eq!(list.pop_front(), None);
    }
}
