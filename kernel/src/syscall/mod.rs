use kernel_api::ptr::TaggedNonNull;
use kernel_api::syscall;
use crate::{arch, ebr, percpu};
use crate::task::TaskRefExt;
use crate::task::Task;

mod system;
pub use system::IntoKoid;

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
pub struct ExitParams {
	pub oid: u32,
	pub method: u16,
	pub interface: u64,
	pub integer_args: [usize; 3],
	pub oob_args: [usize; 2],
	pub stack_frame: arch::SyscallStackFrame,
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
		let this = caller.handles().get(params.handle_num, &ebr)?;

		let target = if this.is_system() {
			return system::entry(&ebr, this.koid(), caller, params);
		} else {
			this.target().get().ok_or(syscall::Error::DeadServer)?
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

		ExitParams {
			oid: this.oid(),
			method: params.method,
			interface: params.interface,
			integer_args: params.integer_args,
			oob_args: params.oob_args,
			stack_frame: arch::SyscallStackFrame::from(target),
		}
	};

	debug!("exit syscall: {exit_params:#x?}");
	Ok(exit_params)
}
