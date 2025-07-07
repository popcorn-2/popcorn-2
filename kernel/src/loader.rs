use crate::prelude::*;
use alloc::sync::Arc;
use core::cmp::min;
use core::num::NonZero;
use core::ops::Deref;
use core::ptr;
use itertools::Itertools;
use elf::header::program::SegmentType;
use kernel_api::memory::{mapping, Page, VirtualAddress};
use kernel_api::memory::mapping::{Location, new_stack_in, new_unsafe_mapping_in, Protection, Stack};
use kernel_api::memory::r#virtual::Userspace;
use crate::memory::r#virtual::AddressSpaceInner;

pub fn set_up_stack<'arg, 'env, 'handle, H, D: Deref<Target = str>>(
	stack: &Stack<Userspace>,
	arg: impl IntoIterator<Item = &'arg str>,
	env: impl IntoIterator<Item = &'env str>,
	handles: H
) -> VirtualAddress where for<'a> &'a H: IntoIterator<Item = (&'a D, &'a u32)> {
	let (arg, env, handles1, handles2) = (arg.into_iter(), env.into_iter(), (&handles).into_iter(), (&handles).into_iter());
	let mut stack_ptr_user = stack.virtual_valid_end().as_ptr();
	let mut stack_ptr_kernel = stack.physical_end() // fixme: make mmap return User<*>
	                                .unwrap()
	                                .to_page()
	                                .as_ptr();

	fn write_strings<'a>(stack_ptr_kernel: &mut *mut u8, stack_ptr_user: &mut *mut u8, strings: impl Iterator<Item = &'a str>) -> Vec<usize> {
		let mut ptrs = Vec::with_capacity(strings.size_hint().0);
		for str in strings {
			// write null terminator
			*stack_ptr_kernel = unsafe { stack_ptr_kernel.offset(-1) };
			*stack_ptr_user = unsafe { stack_ptr_user.offset(-1) };
			unsafe { stack_ptr_kernel.write(0) };

			// write string content
			*stack_ptr_kernel = unsafe { stack_ptr_kernel.sub(str.len()) };
			*stack_ptr_user = unsafe { stack_ptr_user.sub(str.len()) };
			unsafe { ptr::copy_nonoverlapping(str.as_bytes().as_ptr(), *stack_ptr_kernel, str.len()); }

			debug!("{:#p} = {str}", *stack_ptr_user);

			// store start ptr
			ptrs.push(stack_ptr_user.addr());
		}
		ptrs
	}

	let (handle_ids, handle_nums) = (handles1.map(|(s, _)| &**s), handles2.map(|(_, i)| *i as usize));
	let arg_ptrs = write_strings(&mut stack_ptr_kernel, &mut stack_ptr_user, arg);
	let env_ptrs = write_strings(&mut stack_ptr_kernel, &mut stack_ptr_user, env);
	let handle_ptrs = write_strings(&mut stack_ptr_kernel, &mut stack_ptr_user, handle_ids);

	let align_offset = stack_ptr_user.align_offset(size_of::<usize>());
	let mut stack_ptr_kernel = unsafe { stack_ptr_kernel.cast::<usize>().byte_sub(size_of::<usize>() - align_offset) };
	let mut stack_ptr_user = unsafe { stack_ptr_user.cast::<usize>().byte_sub(size_of::<usize>() - align_offset) };

	let total_count = 1 // argc
			+ arg_ptrs.len()
			+ 1 // argv terminator
			+ env_ptrs.len()
			+ 1 // envp terminator
			+ 2 // auxv terminator
			+ 2 * handle_ptrs.len() // handle map
			+ 1; // handle terminator
	stack_ptr_kernel = unsafe { stack_ptr_kernel.sub(total_count) };
	stack_ptr_user = unsafe { stack_ptr_user.sub(total_count) };
	if !stack_ptr_user.is_aligned_to(16) {
		stack_ptr_kernel = unsafe { stack_ptr_kernel.sub(1) };
		stack_ptr_user = unsafe { stack_ptr_user.sub(1) };
	}

	for (offset, ptr) in core::iter::once(arg_ptrs.len()) // argc
			.chain(arg_ptrs.into_iter()) // argv
			.chain(core::iter::once(0)) // argv terminator
			.chain(env_ptrs.into_iter()) // env
			.chain(core::iter::once(0)) // env terminator
			.chain(core::iter::repeat_n(0, 2)) // auxv terminator
			.chain(handle_ptrs.into_iter().interleave_shortest(handle_nums)) // handle list
			.chain(core::iter::once(0)) // handle list terminator
			.enumerate()
	{
		unsafe { stack_ptr_kernel.add(offset).write(ptr) };
		debug!("{:#p} = {ptr:#x}", unsafe { stack_ptr_kernel.add(offset) });
	}

	assert!(stack_ptr_user.is_aligned_to(16), "userspace stack pointer not 16-byte aligned");

	VirtualAddress::from(stack_ptr_user)
}
