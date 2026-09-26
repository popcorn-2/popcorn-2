use kernel_api::num::ufat;
use kernel_api::ptr::TaggedNonNull;
use kernel_api::syscall;
use crate::{arch, ebr, percpu};
use crate::task::TaskRefExt;
use crate::task::Task;

mod system;
pub use system::{IntoKoid, new_koid_serial};

#[derive(Debug)]
pub struct EntryParams {
	pub handle_num: u32,
	pub method: u16,
	pub interface: u64,
	pub integer_args: [usize; 3],
	pub oob_args: [usize; 2],
	pub stack_frame: arch::SyscallStackFrame,
}

#[derive(Debug)]
pub enum ExitParams {
	SwitchTask(EntryParams),
	Return(ufat),
}

#[inline(always)]
pub unsafe fn entry(params: EntryParams) -> syscall::Result<ExitParams> {
	debug!("enter syscall: {params:#x?}");
	let caller = {
		let task_ref = percpu!(current_task).get();
		let task = task_ref.get();
		// SAFETY: syscalls can only be made from a running thread
		unsafe { task.unwrap_unchecked() }
	};

	let exit_params = {
		let ebr = percpu!(epoch).pin();
		let this = if params.handle_num.cast_signed() < 0 {
			// real handle numbers are >=0
			// between -1 and -4096 implies an error from a previous syscall has been passed straight in
			// -4097 and low are pseudo-handles
			Err(params.handle_num.cast_signed().unsigned_abs())
		} else {
			Ok(caller.handles().get(params.handle_num, &ebr)?)
		};

		let (this, target) = match this {
			// `current address space` pseudo-handle
			Err(4098) => return system::address_space_koid_entry(&ebr, &caller.address_space, caller, params).map(ExitParams::Return),
			Err(_) => return Err(syscall::Error::InvalidHandle),

			Ok(handle) if handle.is_system() => return system::entry(&ebr, handle.koid(), caller, params).map(ExitParams::Return),
			Ok(this) => (this, this.target().get().ok_or(syscall::Error::DeadServer)?),
		};

		todo!("check target is blocked waiting for syscall");

		let oob_bytes = params.oob_args.iter().sum::<usize>();

		let caller_trampoline = caller.syscall_trampoline_page();
		let target_trampoline = target.syscall_trampoline_page();
		unsafe {
			core::arch::asm!(
			"rep movsb",
			in("rdi") target_trampoline,
			in("rsi") caller_trampoline,
			inout("rcx") oob_bytes => _,
			)
		}

		params.stack_frame.store(caller);
		percpu!(current_task).set(Some(this.target()));

		ExitParams::SwitchTask(EntryParams {
			handle_num: this.oid(),
			method: params.method,
			interface: params.interface,
			integer_args: params.integer_args,
			oob_args: params.oob_args,
			stack_frame: arch::SyscallStackFrame::from(target),
		})
	};

	debug!("exit syscall: {exit_params:#x?}");
	Ok(exit_params)
}
