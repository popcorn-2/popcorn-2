//! A singly linked list used for storing metadata of each allocation.
//!
//! The allocation to the backing allocator is a page sized multiple, which
//! is then split up into individual linked list nodes.

use core::cmp::Ordering;
use core::fmt::{Debug, Formatter};
use core::num::NonZero;
use core::ops::Range;
use log::debug;
use kernel_api::allocator::{highmem, AllocError};
use kernel_api::memory::{Frames, VirtualAddress, PAGE_SIZE};
use kernel_api::memory::RawPage;

#[cfg(test)]
mod mock {
	use alloc::boxed::Box;
	use alloc::vec;
	use core::fmt::{Debug, Formatter};
	use core::num::NonZero;
	use kernel_api::memory::PAGE_SIZE;
	use super::Node;

	pub struct FrameMock {
		inner: Box<[Node]>
	}
	
	impl Debug for FrameMock {
		fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
			write!(f, "FrameMock {{ .. }}")
		}
	}

	impl FrameMock {
		pub fn new(page_count: NonZero<usize>) -> Self {
			Self {
				inner: vec![Node::zeroed(); page_count.get() * PAGE_SIZE / size_of::<Node>()].into_boxed_slice()
			}
		}

		pub fn get(&self) -> &[Node] {
			&*self.inner
		}

		pub fn get_mut(&mut self) -> &mut [Node] {
			&mut *self.inner
		}
	}
}

#[derive(Debug)]
pub enum InsertError {
	AllocError,
	AddressOverlap,
}

impl From<AllocError> for InsertError {
	fn from(_: AllocError) -> Self { Self::AllocError }
}

#[derive(Clone, Debug)]
pub struct Node {
	/// Whether the node contains valid metadata.
	valid: bool,
	/// The index of the next node in the linked list.
	next: Option<usize>,
	/// The range of pages covered by this node.
	addr: Range<RawPage>,
	meta: Meta,
}

impl Node {
	const fn zeroed() -> Self {
		Self {
			valid: false,
			next: None,
			addr: RawPage::new(0)..RawPage::new(0),
			meta: Meta { len: 0 },
		}
	}
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Meta {
	/// The number of pages in this allocation.
	pub len: usize,
}

pub struct LinkedList {
	root: Option<usize>,
	#[cfg(not(test))] backing: Frames<true, Node>,
	#[cfg(test)] backing: mock::FrameMock,
}

impl Debug for LinkedList {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		let mut debug = f.debug_list();
		let mut current = self.root;
		while let Some(node) = current {
			let node = self.get_node(node);
			debug.entry(node);
			current = node.next;
		}
		debug.finish()
	}
}

impl LinkedList {
	pub fn new(allocation_count: NonZero<usize>) -> Result<Self, AllocError> {
		const { assert!(PAGE_SIZE >= size_of::<Node>(), "`Node` must fit in a single page"); }

		let page_count = {
			#[expect(clippy::missing_panics_doc, reason = "infallible")]
			let allocs_per_page = NonZero::new(PAGE_SIZE.div_floor(size_of::<Node>()))
					.expect("Node should not be larger than a page");
			allocation_count.div_ceil(allocs_per_page)
		};
		debug!("allocate {page_count:#x} pages for LinkedListAllocator");
		#[cfg(not(test))] let backing = {
			let backing = highmem().allocate(page_count)?;
			backing.cast::<Node>().into_filed(Node::zeroed())
		};
		#[cfg(test)] let backing = mock::FrameMock::new(page_count);

		Ok(Self {
			root: None,
			backing,
		})
	}
	
	fn get_node_mut(
		&mut self,
		idx: usize,
	) -> &mut Node {
		&mut self.backing.get_mut()[idx]
	}

	fn get_node(
		&self,
		idx: usize,
	) -> &Node {
		&self.backing.get()[idx]
	}

	fn allocate_node(
		&mut self,
		addr: Range<RawPage>,
		meta: Meta,
	) -> Result<(&mut Node, usize), AllocError> {
		let mem = self.backing.get_mut();

		let mut iter = mem.iter_mut().enumerate();
		let (i, node) = loop {
			let Some((i, node)) = iter.next() else { return Err(AllocError::vmm()); };

			if !node.valid { break (i, node); }
		};

		*node = Node {
			valid: true,
			next: None,
			addr,
			meta,
		};

		Ok((node, i))
	}

	pub fn insert(&mut self, addr: Range<RawPage>, meta: Meta) -> Result<(), InsertError> {
		let Some(root_idx) = self.root else {
			// no root node
			let (_, idx) = self.allocate_node(addr, meta)?;
			self.root = Some(idx);
			return Ok(());
		};

		// check if smaller than root node
		{
			let root = self.get_node_mut(root_idx);
			match compare_range(&addr, &root.addr) {
				None => return Err(InsertError::AddressOverlap),
				Some(Ordering::Less) => {
					let (node, idx) = self.allocate_node(addr, meta)?;
					node.next = Some(root_idx);
					self.root = Some(idx);
					return Ok(());
				},
				Some(Ordering::Greater) => {},
				_ => unreachable!("addr should not compare equal"),
			}
		}

		let mut prev_idx = root_idx;
		loop {
			// we use raw pointers here so we can hold both prev_node and next_node at the same time
			// without running into borrow checker issues, as we know they'll be distinct objects
			let prev_node = self.get_node_mut(prev_idx) as *mut Node;
			let Some(next_idx) = (unsafe { (*prev_node).next }) else {
				let (_, new_idx) = self.allocate_node(addr, meta)?;
				unsafe { (*prev_node).next = Some(new_idx) };
				return Ok(());
			};
			debug_assert_ne!(prev_idx, next_idx, "node should not point to itself");
			let next_node = self.get_node(next_idx) as *const Node;
			match compare_range(&addr, unsafe { &(*next_node).addr }) {
				None => return Err(InsertError::AddressOverlap),
				Some(Ordering::Less) => {
					let (new_node, new_idx) = self.allocate_node(addr, meta)?;
					new_node.next = Some(next_idx);
					unsafe { (*prev_node).next = Some(new_idx) };
					return Ok(());
				},
				Some(Ordering::Greater) => {
					prev_idx = next_idx;
				},
				_ => unreachable!("addr should not compare equal"),
			}
		}
	}

	pub fn remove(&mut self, addr: VirtualAddress) -> Option<Meta> {
		let Some(prev_idx) = self.root else { return None; };
		let mut prev = self.get_node_mut(prev_idx) as *mut Node;

		if unsafe { (*prev).addr.contains(&addr.align_down_to_page()) } {
			self.root = unsafe { (*prev).next };
			unsafe { (*prev).valid = false };
			return Some(unsafe { (*prev).meta });
		}

		while let Some(next_idx) = unsafe { (*prev).next } {
			let next = self.get_node_mut(next_idx);
			if next.addr.contains(&addr.align_down_to_page()) {
				unsafe { (*prev).next = next.next };
				next.valid = false;
				return Some(next.meta);
			}

			prev = next;
		}

		None
	}

	pub gen fn iter_gaps(&self, clamp: Range<RawPage>) -> Range<RawPage> {
		let mut next = self.root;
		let mut prev_end = clamp.start;
		while let Some(idx) = next {
			let node = self.get_node(idx);
			let gap = prev_end..node.addr.start;
			if !gap.is_empty() {
				yield gap;
			}
			prev_end = node.addr.end;
			next = node.next;
		}
		yield prev_end..clamp.end;
	}
}

fn compare_range(lhs: &Range<RawPage>, rhs: &Range<RawPage>) -> Option<Ordering> {
	if lhs.start > lhs.end || rhs.start > rhs.end { return None; }

	if lhs.end <= rhs.start { Some(Ordering::Less) }
	else if rhs.end <= lhs.start { Some(Ordering::Greater) }
	else { None }
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn empty_iter_yields_clamp() {
		let list = LinkedList::new(const { NonZero::new(1).unwrap() }).unwrap();
		let mut iter = list.iter_gaps(RawPage::new(5*4096)..RawPage::new(10*4096));
		assert_eq!(iter.next(), Some(RawPage::new(5*4096)..RawPage::new(10*4096)));
		assert_eq!(iter.next(), None);
	}

	#[test]
	fn with_nodes() {
		let mut list = LinkedList::new(const { NonZero::new(1).unwrap() }).unwrap();
		list.insert(RawPage::new(6*4096)..RawPage::new(8*4096), Meta { len: 2 }).unwrap();
		list.insert(RawPage::new(8*4096)..RawPage::new(15*4096), Meta { len: 7 }).unwrap();
		list.insert(RawPage::new(17*4096)..RawPage::new(18*4096), Meta { len: 1 }).unwrap();
		let mut iter = list.iter_gaps(RawPage::new(5*4096)..RawPage::new(50*4096));
		assert_eq!(iter.next(), Some(RawPage::new(5*4096)..RawPage::new(6*4096)));
		assert_eq!(iter.next(), Some(RawPage::new(15*4096)..RawPage::new(17*4096)));
		assert_eq!(iter.next(), Some(RawPage::new(18*4096)..RawPage::new(50*4096)));
		assert_eq!(iter.next(), None);
	}

	#[test]
	fn removal() {
		let mut list = LinkedList::new(const { NonZero::new(1).unwrap() }).unwrap();
		list.insert(RawPage::new(6*4096)..RawPage::new(8*4096), Meta { len: 2 }).unwrap();
		assert_eq!(list.remove(VirtualAddress::new(10*4096 + 482)), None);
		assert_eq!(list.remove(VirtualAddress::new(7*4096 + 183)), Some(Meta { len: 2 }));
		assert_eq!(list.remove(VirtualAddress::new(7*4096 + 134)), None);
	}

	#[test]
	#[should_panic = "`Err` value: AddressOverlap"]
	fn range_overlap() {
		let mut list = LinkedList::new(const { NonZero::new(1).unwrap() }).unwrap();
		list.insert(RawPage::new(6*4096)..RawPage::new(8*4096), Meta { len: 2 }).unwrap();
		list.insert(RawPage::new(7*4096)..RawPage::new(9*4096), Meta { len: 2 }).unwrap();
	}
}
