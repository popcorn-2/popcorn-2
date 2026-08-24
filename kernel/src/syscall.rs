use kernel_api::dbg;

#[cfg(target_arch = "x86_64")]
pub extern "rust-preserve-none" fn entry(
	r12: usize,
	_r13: usize,
	_r14: usize,
	_r15: usize,
	rdi: usize,
	rsi: usize,
	rdx: usize,
	_rcx: usize,
	r8: usize,
	r9: usize,
	_r11: usize,
	rax: u64,
) -> (usize, usize) {
	let this = rax.truncate::<u32>();
	let flags = (rax >> 32).truncate::<u16>();
	let method = (rax >> 48).truncate::<u16>();
	inner(
		r12,
		this,
		flags,
		method,
		rdi,
		rsi,
		rdx,
		r9,
		r8,
	)
}

#[inline(always)]
fn inner(
	interface: usize,
	this: u32,
	flags: u16,
	method: u16,
	arg1: usize,
	arg2: usize,
	arg3: usize,
	arg4: usize,
	arg5: usize
) -> (usize, usize) {
	dbg!(
		interface,
		this,
		flags,
		method,
		arg1,
		arg2,
		arg3,
		arg4,
		arg5
	);
	todo!()
}
