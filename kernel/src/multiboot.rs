use core::marker::PhantomData;
use crate::memory::VirtAddr;

pub struct Info(VirtAddr);

impl Info {
	pub unsafe fn load(addr: VirtAddr) -> Self {
		Self(addr)
	}
}

pub struct InfoIterator<'a> {
	current: *const Tag,
	end: *const Tag,
	phantom: PhantomData<&'a Info>
}

impl<'a> Iterator for InfoIterator<'a> {
	type Item = &'a Tag;
	fn next(&mut self) -> Option<Self::Item> {
		let next = unsafe { self.current.byte_add((*self.current).size as _) };
		if next >= self.end { None }
		else { unsafe { Some(&*next) } }
	}
}

#[repr(C, u32)]
pub enum Tag {
	Cli(tags::Cli) = 1
}

mod tags {
	pub struct Cli;
}
