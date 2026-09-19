use alloc::sync::Arc;
use core::num::NonZero;
use kernel_api::mapping::{Config, Mmap, Ty};
use kernel_api::memory::{VirtualAddress, PAGE_SIZE};
use kernel_api::syscall;
use kernel_api::syscall::Handle;
use crate::ebr;
use crate::task::Task;

#[derive(Debug)]
#[repr(C)]
pub struct ProcInfo {
	magic: [u8; 7],
	version: u8,
	// version 0 fields
	argc: core::ffi::c_int,
	argv: VirtualAddress,
	named_handles: VirtualAddress,
	info_ty: usize,
	info_ptr: VirtualAddress,
}

impl ProcInfo {
	fn new_raw(
		argc: core::ffi::c_int,
		argv: VirtualAddress,
		named_handles: VirtualAddress,
		info_ty: usize,
		info_ptr: VirtualAddress,
	) -> Self {
		Self {
			magic: *b"POPCRN\0",
			version: 0,
			argc,
			argv,
			named_handles,
			info_ty,
			info_ptr,
		}
	}

	pub fn new_in(
		task: &Task,
		args: &[&str],
		handles: Vec<(&str, Arc<Handle>)>, // we need to transfer ownership of the popped handle
		info_ty: usize,
		info_ptr: VirtualAddress,
		ebr: &ebr::EpochGuard,
	) -> syscall::Result<VirtualAddress> {
		const {
			assert!(align_of_val("") == 1, "strings must be 1 byte aligned");
			assert!(align_of::<ProcInfo>() <= PAGE_SIZE, "proc info must be aligned to page size");
			assert!(align_of::<ProcInfo>() >= align_of::<NamedHandle>(), "NamedHandle must fit after proc info");
			assert!(align_of::<NamedHandle>() >= align_of::<*const core::ffi::c_char>(), "argv array must fit after handles");
		}

		let proc_info_size = size_of::<ProcInfo>();
		let handle_tab_len = size_of::<NamedHandle>() * (handles.len() + 1);
		let argv_len = size_of::<*const core::ffi::c_char>() * (args.len() + 1);
		let string_tab_len = args.iter().map(|str| str.len() + 1).sum::<usize>() +
			handles.iter().map(|(str, _)| str.len() + 1).sum::<usize>();

		let page_count = (proc_info_size + handle_tab_len + argv_len + string_tab_len).div_ceil(PAGE_SIZE);
		let mut mapping = Config::new(
			NonZero::new(page_count).expect("proc info should be non-zero size"),
			Ty::UNKNOWN,
		).protection(true, false, true)
			.map_in::<Mmap>("startup info".into(), &task.address_space)?;

		let ptr = mapping.mapping.as_mut_ptr().cast::<ProcInfo>();
		let named_handles = unsafe { ptr.add(1).cast::<NamedHandle>() };
		let argv_array = unsafe { named_handles.add(handles.len() + 1).cast::<VirtualAddress>() };
		let mut next_string = unsafe { argv_array.add(args.len() + 1).cast::<u8>() };

		// add handle list terminator
		unsafe {
			named_handles.add(handles.len())
				.write_other_address_space(NamedHandle::TERMINATOR)
				.expect("allocated memory should be valid");
		}

		// add argv array terminator
		unsafe {
			argv_array.add(args.len())
				.write_other_address_space(VirtualAddress::new(0))
				.expect("allocated memory should be valid");
		}

		// write handle list
		for (i, (name, handle)) in handles.into_iter().enumerate() {
			let handle_num = task.handles.push(handle, ebr)?;
			let handle = NamedHandle {
				name: next_string.addr(),
				handle_num,
			};
			unsafe {
				// write handle itself
				named_handles.add(i)
					.write_other_address_space(handle)
					.expect("allocated memory should be valid");
				// write handle name to strtab
				next_string.copy_from_other_address_space(
					name.as_ptr(),
					name.len()
				).expect("allocated memory should be valid");
				// add null terminator to string
				next_string.add(name.len())
					.write_other_address_space(0)
					.expect("allocated memory should be valid");
				next_string = next_string.add(name.len() + 1);
			}
		}

		for (i, arg) in args.iter().enumerate() {
			unsafe {
				// write pointer to string into argv array
				argv_array.add(i)
					.write_other_address_space(next_string.addr())
					.expect("allocated memory should be valid");
				// write arg itself to strtab
				next_string.copy_from_other_address_space(
					arg.as_ptr(),
					arg.len()
				).expect("allocated memory should be valid");
				// add null terminator to string
				next_string.add(arg.len())
					.write_other_address_space(0)
					.expect("allocated memory should be valid");
				next_string = next_string.add(arg.len() + 1);
			}
		}

		ptr.write_other_address_space(ProcInfo::new_raw(
			args.len().try_into().map_err(|_| syscall::Error::Overflow)?,
			argv_array.addr(),
			named_handles.addr(),
			info_ty,
			info_ptr,
		))?;

		Ok(ptr.addr())
	}
}

#[derive(Debug)]
#[repr(C)]
struct NamedHandle {
	name: VirtualAddress,
	handle_num: u32,
}

impl NamedHandle {
	const TERMINATOR: Self = Self {
		name: VirtualAddress::new(0),
		handle_num: 0,
	};
}
