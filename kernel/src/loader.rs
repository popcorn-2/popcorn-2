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
	let mut stack_ptr = stack.virtual_valid_end().as_ptr();

	fn write_strings<'a>(stack_ptr: &mut *mut u8, strings: impl Iterator<Item = &'a str>) -> Vec<usize> {
		let mut ptrs = Vec::with_capacity(strings.size_hint().0);
		for str in strings {
			// write null terminator
			*stack_ptr = unsafe { stack_ptr.offset(-1) };
			unsafe { stack_ptr.write(0) };

			// write string content
			*stack_ptr = unsafe { stack_ptr.sub(str.len()) };
			unsafe { ptr::copy_nonoverlapping(str.as_bytes().as_ptr(), *stack_ptr, str.len()); }

			debug!("{:#p} = {str}", *stack_ptr);

			// store start ptr
			ptrs.push(stack_ptr.addr());
		}
		ptrs
	}

	let (handle_ids, handle_nums) = (handles1.map(|(s, _)| &**s), handles2.map(|(_, i)| *i as usize));
	let arg_ptrs = write_strings(&mut stack_ptr, arg);
	let env_ptrs = write_strings(&mut stack_ptr, env);
	let handle_ptrs = write_strings(&mut stack_ptr, handle_ids);

	let align_offset = stack_ptr.align_offset(size_of::<usize>());
	let mut stack_ptr = unsafe { stack_ptr.cast::<usize>().byte_sub(size_of::<usize>() - align_offset) };

	let total_count = 1 // argc
			+ arg_ptrs.len()
			+ 1 // argv terminator
			+ env_ptrs.len()
			+ 1 // envp terminator
			+ 2 // auxv terminator
			+ 2 * handle_ptrs.len() // handle map
			+ 1; // handle terminator
	stack_ptr = unsafe { stack_ptr.sub(total_count) };
	if !stack_ptr.is_aligned_to(16) { stack_ptr = unsafe { stack_ptr.sub(1) }; }

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
		unsafe { stack_ptr.add(offset).write(ptr) };
		debug!("{:#p} = {ptr:#x}", unsafe { stack_ptr.add(offset) });
	}

	assert!(stack_ptr.is_aligned_to(16), "userspace stack pointer not 16-byte aligned");

	VirtualAddress::from(stack_ptr)
}
