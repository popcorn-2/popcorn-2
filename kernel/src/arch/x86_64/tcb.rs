use core::alloc::Layout;
use core::arch::x86_64::{CpuidResult, __cpuid_count};
use core::ptr::NonNull;
use core::sync::atomic::AtomicU64;

#[derive(Debug)]
pub struct SavedRegisters {
	pub rax: AtomicU64,
	pub rbx: AtomicU64,
	pub rcx: AtomicU64,
	pub rdx: AtomicU64,
	pub rsi: AtomicU64,
	pub rdi: AtomicU64,
	pub rsp: AtomicU64,
	pub rbp: AtomicU64,
	pub r8: AtomicU64,
	pub r9: AtomicU64,
	pub r10: AtomicU64,
	pub r11: AtomicU64,
	pub r12: AtomicU64,
	pub r13: AtomicU64,
	pub r14: AtomicU64,
	pub r15: AtomicU64,
	pub rip: AtomicU64,
	pub rflags: AtomicU64,
	pub fs_base: AtomicU64,
	pub gs_base: AtomicU64,
	pub tss_scratch: AtomicU64,
	pub xsave: Xsave,
}

impl Default for SavedRegisters {
	fn default() -> Self {
		Self {
			rax: AtomicU64::new(0),
			rbx: AtomicU64::new(0),
			rcx: AtomicU64::new(0),
			rdx: AtomicU64::new(0),
			rsi: AtomicU64::new(0),
			rdi: AtomicU64::new(0),
			rsp: AtomicU64::new(0),
			rbp: AtomicU64::new(0),
			r8: AtomicU64::new(0),
			r9: AtomicU64::new(0),
			r10: AtomicU64::new(0),
			r11: AtomicU64::new(0),
			r12: AtomicU64::new(0),
			r13: AtomicU64::new(0),
			r14: AtomicU64::new(0),
			r15: AtomicU64::new(0),
			rip: AtomicU64::new(0),
			rflags: AtomicU64::new(0),
			fs_base: AtomicU64::new(0),
			gs_base: AtomicU64::new(0),
			tss_scratch: AtomicU64::new(0),
			xsave: Xsave::new(),
		}
	}
}

#[derive(Debug)]
struct Xsave {
	layout: Layout,
	allocation: NonNull<u8>,
}

impl Xsave {
	fn new() -> Self {
		let CpuidResult { ebx: xsave_size, .. } = __cpuid_count(0xd, 0);
		let layout = Layout::from_size_align(
			xsave_size as usize,
			64,
		).expect("invalid layout for xsave region");
		// SAFETY: at least x87 and SSE are enabled in xcr0, so `xsave_size` cannot be 0
		let allocation = unsafe {
			NonNull::new(alloc::alloc::alloc_zeroed(layout)) // we want the header to be zeroed, and it's easier to just zero the whole thing
				.expect("failed to allocate xsave region")
		};

		Self {
			layout,
			allocation,
		}
	}
}

unsafe impl Send for Xsave {}
unsafe impl Sync for Xsave {}

impl Drop for Xsave {
	fn drop(&mut self) {
		// SAFETY: layout is stored from original allocation, and allocation comes from call to `alloc`
		unsafe {
			alloc::alloc::dealloc(self.allocation.as_ptr(), self.layout);
		}
	}
}
