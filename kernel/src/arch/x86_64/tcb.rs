use core::alloc::Layout;
use core::arch::x86_64::{CpuidResult, __cpuid_count};
use core::ptr::NonNull;
use core::sync::atomic::{AtomicU64, Ordering};
use kernel_api::memory::VirtualAddress;

#[derive(Debug)]
pub struct SavedRegisters {
	pub(super) rax: AtomicU64,
	pub(super) rbx: AtomicU64,
	pub(super) rcx: AtomicU64,
	pub(super) rdx: AtomicU64,
	pub(super) rsi: AtomicU64,
	pub(super) rdi: AtomicU64,
	pub(super) rsp: AtomicU64,
	pub(super) rbp: AtomicU64,
	pub(super) r8: AtomicU64,
	pub(super) r9: AtomicU64,
	pub(super) r10: AtomicU64,
	pub(super) r11: AtomicU64,
	pub(super) r12: AtomicU64,
	pub(super) r13: AtomicU64,
	pub(super) r14: AtomicU64,
	pub(super) r15: AtomicU64,
	pub(super) rip: AtomicU64,
	pub(super) rflags: AtomicU64,
	pub(super) fs_base: AtomicU64,
	pub(super) gs_base: AtomicU64,
	pub(super) xsave: Xsave,
}

impl SavedRegisters {
	pub fn set_syscall_trampoline(&self, addr: VirtualAddress) {
		self.gs_base.store(addr.addr as u64, Ordering::Relaxed);
	}
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
