#[allow(unused_imports)] use crate::prelude::*;
use alloc::borrow::Cow;
use core::fmt::{Display, Formatter};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum Register { X, Y }

impl Display for Register {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::X => write!(f, "X"),
			Self::Y => write!(f, "Y"),
		}
	}
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Immediate(pub u64);

impl Display for Immediate {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "{:#x}", self.0)
	}
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Port(pub u16);

impl Display for Port {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "{:#x}", self.0)
	}
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct Jump {
	pub instruction_count: usize,
}

impl Display for Jump {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "+{}", self.instruction_count)
	}
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Opcode {
	Push(Register),
	Inb(Register, Port),
	Inw(Register, Port),
	Ind(Register, Port),
	Outb(Port, Register),
	Outw(Port, Register),
	Outd(Port, Register),
	Ret,
	Cli,
	Copy(Register),
	Imm(Register, Immediate),
	Ble(Register, Immediate, Jump),
}

impl Display for Opcode {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		match self {
			Opcode::Push(r) => write!(f, "push {r}"),
			Opcode::Inb(r, p) => write!(f, "inb {r} <- {p}"),
			Opcode::Inw(r, p) => write!(f, "inw {r} <- {p}"),
			Opcode::Ind(r, p) => write!(f, "ind {r} <- {p}"),
			Opcode::Outb(p, r) => write!(f, "outb {p} <- {r}"),
			Opcode::Outw(p, r) => write!(f, "outw {p} <- {r}"),
			Opcode::Outd(p, r) => write!(f, "outd {p} <- {r}"),
			Opcode::Ret => write!(f, "ret"),
			Opcode::Cli => write!(f, "cli"),
			Opcode::Copy(r) => write!(f, "copy {} <- {r}", match *r { Register::X => Register::Y, Register::Y => Register::X, }),
			Opcode::Imm(r, imm) => write!(f, "imm {r} <- {imm}"),
			Opcode::Ble(r, imm, j) => write!(f, "b {r} <= {imm}, {j}"),
		}
	}
}

pub struct Program {
	pub data: Cow<'static, [Opcode]>,
}

impl Display for Program {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		self.data.as_ref().into_iter().try_for_each(|line| {
			write!(f, "{line}\n")
		})
	}
}

macro _irq_program_inner {
	(push x) => { "push rax\ninc r12" },
	(push y) => { "push rdx\ninc r12" },
	(inbsl x <- $port:literal) => { concat!("shl rax, 8\n", "in al, ", stringify!($port)) },
	(inwsl x <- $port:literal) => { concat!("shl rax, 16\n", "in ax, ", stringify!($port)) },
	(indsl x <- $port:literal) => { concat!("shl rax, 32\n", "in eax, ", stringify!($port)) },
	(outbsl $port:literal <- x) => { concat!("out ", stringify!($port), ", al\n", "shr rax, 8") },
	(outwsl $port:literal <- x) => { concat!("out ", stringify!($port), ", ax\n", "shr rax, 16") },
	(outdsl $port:literal <- x) => { concat!("out ", stringify!($port), ", eax\n", "shr rax, 32") },
	(ret) => { "push rsi\nret" },
	(cli) => { "cli" },
	(copy y <- x) => { "mov rdx, rax" },
	(copy x <- y) => { "mov rax, rdx" },
	(imm x <- $imm:literal) => { concat!("mov rax, ", stringify!($imm)) },
	(imm y <- $imm:literal) => { concat!("mov rdx, ", stringify!($imm)) },
	(ble x <- $imm:literal, $jmp:literal) => { "ud2" },
	(ble y <- $imm:literal, $jmp:literal) => { "ud2" },
	(xchg) => { "xchg rax, rdx" }
}

pub macro irq_program($({$opcode:ident $($arg1:tt $(<- $arg2:tt $(, $arg3:literal)?)?)?})*) {{
	#[naked]
	unsafe extern "C" fn irq_program() {
		::core::arch::asm!("pop rsi", $(_irq_program_inner!($opcode $($arg1 $(<- $arg2 $(, $arg3)?)?)?)),*, "push rsi", "ret", options(noreturn))
	}

	unsafe { CompiledIrqProgram::new(irq_program) }
}}

/// # Calling convention
///
/// rax contains input value. r12 and rdx clear on entry. Clobbers rax, rsi, rdx, r12. r12 contains number of return args. Return args on stack, with last argument lowest.
pub struct CompiledIrqProgram(unsafe extern "C" fn());

impl CompiledIrqProgram {
	pub const unsafe fn new(f: unsafe extern "C" fn()) -> Self { Self(f) }

	pub fn call(&self, input: u64) -> Vec<u64> {
		extern "C" fn allocate_vec(size: usize) -> (*mut u64, usize) {
			let v = Vec::with_capacity(size);
			let (ptr, _, cap) = v.into_raw_parts();
			(ptr, cap)
		}

		let mut v = unsafe {
			let ptr;
			let cap;
			let len;
			core::arch::asm!(
				"mov r12, 0",
				"mov rdx, 0",
				"call {0}", // Call the VM function
				"mov rdi, r12", // Move return arg count into first arg register
				"push rbp",
				"mov rbp, rsp",
				"and rsp, 0xFFFFFFFFFFFFFFF0", // Save and align stack pointer
				"call {1}", // Allocate a vec with enough length for number of return args. rax = vec ptr, r12 = len, rdx = capacity, rbp = bottom of arg list - 8
				"mov rdi, rax", // dest = vec ptr
				"lea rsi, [rbp+8]", // src = arg pointer = rbp + 8
				"push rax",
				"push rdx",
				"lea rdx, [8*r12]",
				"call memcpy",
				"pop rdx",
				"pop rax",
				"mov rsp, rbp",
				"pop rbp",
				"lea rsp, [rsp + 8*r12]",
				in(reg) self.0,
				sym allocate_vec,
				inout("rax") input => ptr,
				out("r12") len,
				out("rdx") cap,
				out("rsi") _,
			);
			Vec::from_raw_parts(ptr, len, cap)
		};
		v.reverse();
		v
	}
}
