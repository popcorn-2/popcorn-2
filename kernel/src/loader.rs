use core::ops::Deref;
use itertools::Itertools;
use kernel_api::address_space::Userspace;
use kernel_api::mapping::{Mappable, Mapping};
use kernel_api::memory::VirtualAddress;
use kernel_api::ptr::User;

pub fn set_up_stack<'arg, 'env, 'handle, H, D: Deref<Target = str>, R: Mappable>(
	stack: &mut Mapping<R, Userspace>,
	arg: impl IntoIterator<Item = &'arg str>,
	env: impl IntoIterator<Item = &'env str>,
	handles: H
) -> VirtualAddress where for<'a> &'a H: IntoIterator<Item = (&'a D, &'a u32)> {
	let (arg, env, handles1, handles2) = (arg.into_iter(), env.into_iter(), (&handles).into_iter(), (&handles).into_iter());
	let mut stack_ptr_user = stack.as_mut_ptr_range().end;
	
	fn write_strings<'a>(stack_ptr: &mut User<'_, *mut u8>, strings: impl Iterator<Item = &'a str>) -> Vec<VirtualAddress> {
		let mut ptrs = Vec::with_capacity(strings.size_hint().0);
		for str in strings {
			// write null terminator
			*stack_ptr = unsafe { stack_ptr.offset(-1) };
			stack_ptr.write_other_address_space(0).expect("mapped stack should exist");

			// write string content
			*stack_ptr = unsafe { stack_ptr.sub(str.len()) };

			unsafe { stack_ptr.copy_from_other_address_space(str.as_bytes().as_ptr(), str.len()).expect("mapped stack should exist") };

			debug!("{:#p} = {str}", *stack_ptr);

			// store start ptr
			ptrs.push(stack_ptr.addr());
		}
		ptrs
	}

	let (handle_ids, handle_nums) = (handles1.map(|(s, _)| &**s), handles2.map(|(_, i)| VirtualAddress::new(*i as usize)));
	let arg_ptrs = write_strings(&mut stack_ptr_user, arg);
	let env_ptrs = write_strings(&mut stack_ptr_user, env);
	let handle_ptrs = write_strings(&mut stack_ptr_user, handle_ids);

	let align_offset = stack_ptr_user.align_offset(size_of::<VirtualAddress>());
	let mut stack_ptr_user = unsafe { stack_ptr_user.cast::<VirtualAddress>().byte_sub(size_of::<usize>() - align_offset) };

	let total_count = 1 // argc
			+ arg_ptrs.len()
			+ 1 // argv terminator
			+ env_ptrs.len()
			+ 1 // envp terminator
			+ 2 // auxv terminator
			+ 2 * handle_ptrs.len() // handle map
			+ 1; // handle terminator
	stack_ptr_user = unsafe { stack_ptr_user.sub(total_count) };
	if !stack_ptr_user.is_aligned_to(16) {
		stack_ptr_user = unsafe { stack_ptr_user.sub(1) };
	}

	for (offset, ptr) in core::iter::once(VirtualAddress::new(arg_ptrs.len())) // argc
			.chain(arg_ptrs.into_iter()) // argv
			.chain(core::iter::once(VirtualAddress::new(0))) // argv terminator
			.chain(env_ptrs.into_iter()) // env
			.chain(core::iter::once(VirtualAddress::new(0))) // env terminator
			.chain(core::iter::repeat_n(VirtualAddress::new(0), 2)) // auxv terminator
			.chain(handle_ptrs.into_iter().interleave_shortest(handle_nums)) // handle list
			.chain(core::iter::once(VirtualAddress::new(0))) // handle list terminator
			.enumerate()
	{
		unsafe { stack_ptr_user.add(offset).write_other_address_space(ptr).expect("mapped stack should exist"); }
		debug!("{:#p} = {ptr:#x}", unsafe { stack_ptr_user.add(offset) });
	}

	assert!(stack_ptr_user.is_aligned_to(16), "userspace stack pointer not 16-byte aligned");

	stack_ptr_user.addr()
}
