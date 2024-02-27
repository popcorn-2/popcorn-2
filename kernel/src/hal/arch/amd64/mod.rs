use core::arch::{asm, global_asm};
use core::mem;
use core::mem::offset_of;
use log::{debug, info, warn};
use crate::hal::{Hal, SaveState, ThreadControlBlock};
use crate::hal::arch::amd64::idt::entry::Type;
use crate::hal::arch::amd64::idt::handler::InterruptStackFrame;
use crate::hal::arch::amd64::idt::Idt;
use crate::sprintln;

mod gdt;
mod tss;
mod idt;
mod serial;
mod port;
mod qemu;
mod paging2;
pub(crate) mod paging;
mod pic;

#[derive(Debug)]
#[repr(C)]
struct IrqData {
	num: u64,
	error: u64,
	rip: u64,
	cs: u64,
	flags: u64,
	rsp: u64,
	ss: u64
}

extern "C" fn amd64_handler2(data: &mut IrqData) {
	//

	if data.num as usize > Amd64Hal::MIN_IRQ_NUM {
		crate::hal::irq_handler(data.num as usize);
	} else {
		info!("IRQ: {data:#x?}");
		if data.num != 3 { loop {} }
	}
}

#[naked]
unsafe extern "C" fn amd64_global_irq_handler() {
	asm!(
		"push rax",
		"push rdi",
		"push rsi",
		"push rdx",
		"push rcx",
		"push r8",
		"push r9",
		"push r10",
		"push r11",
		"push 0", // alignment
		"mov rdi, rsp",
		"add rdi, 80",
		"call {}",
		"pop r11",
		"pop r11",
		"pop r10",
		"pop r9",
		"pop r8",
		"pop rcx",
		"pop rdx",
		"pop rsi",
		"pop rdi",
		"pop rax",
		"add rsp, 16",
		"iretq",
	sym amd64_handler2, options(noreturn));
}

macro_rules! irq_handler {
    ($num:literal error) => {
	    ::paste::paste! {
		    #[naked]
		    #[allow(dead_code)]
	        unsafe extern "C" fn [<amd64_irq_handler_ $num>]() {
				asm!(
					concat!("push ", stringify!($num)),
					"jmp {}", sym amd64_global_irq_handler,
				options(noreturn));
		    }
	    }
    };

    ($num:literal) => {
	    ::paste::paste! {
		    #[naked]
		    #[allow(dead_code)]
	        unsafe extern "C" fn [<amd64_irq_handler_ $num>]() {
				asm!(
					"push 0",
					concat!("push ", stringify!($num)),
					"jmp {}", sym amd64_global_irq_handler,
				options(noreturn));
		    }
	    }
    };
}

irq_handler!(0);
irq_handler!(1);
irq_handler!(2);
irq_handler!(3);
irq_handler!(4);
irq_handler!(5);
irq_handler!(6);
irq_handler!(7);
irq_handler!(8 error);
irq_handler!(9);
irq_handler!(10 error);
irq_handler!(11 error);
irq_handler!(12 error);
irq_handler!(13 error);
irq_handler!(14 error);
irq_handler!(15);
irq_handler!(16);
irq_handler!(17 error);
irq_handler!(18);
irq_handler!(19);
irq_handler!(20);
irq_handler!(21 error);
irq_handler!(22);
irq_handler!(23);
irq_handler!(24);
irq_handler!(25);
irq_handler!(26);
irq_handler!(27);
irq_handler!(28);
irq_handler!(29 error);
irq_handler!(30 error);
irq_handler!(31);
irq_handler!(32);
irq_handler!(33);
irq_handler!(34);
irq_handler!(35);
irq_handler!(36);
irq_handler!(37);
irq_handler!(38);
irq_handler!(39);
irq_handler!(40);
irq_handler!(41);
irq_handler!(42);
irq_handler!(43);
irq_handler!(44);
irq_handler!(45);
irq_handler!(46);
irq_handler!(47);
irq_handler!(48);
irq_handler!(49);
irq_handler!(50);
irq_handler!(51);
irq_handler!(52);
irq_handler!(53);
irq_handler!(54);
irq_handler!(55);
irq_handler!(56);
irq_handler!(57);
irq_handler!(58);
irq_handler!(59);
irq_handler!(60);
irq_handler!(61);
irq_handler!(62);
irq_handler!(63);
irq_handler!(64);
irq_handler!(65);
irq_handler!(66);
irq_handler!(67);
irq_handler!(68);
irq_handler!(69);
irq_handler!(70);
irq_handler!(71);
irq_handler!(72);
irq_handler!(73);
irq_handler!(74);
irq_handler!(75);
irq_handler!(76);
irq_handler!(77);
irq_handler!(78);
irq_handler!(79);
irq_handler!(80);
irq_handler!(81);
irq_handler!(82);
irq_handler!(83);
irq_handler!(84);
irq_handler!(85);
irq_handler!(86);
irq_handler!(87);
irq_handler!(88);
irq_handler!(89);
irq_handler!(90);
irq_handler!(91);
irq_handler!(92);
irq_handler!(93);
irq_handler!(94);
irq_handler!(95);
irq_handler!(96);
irq_handler!(97);
irq_handler!(98);
irq_handler!(99);
irq_handler!(100);
irq_handler!(101);
irq_handler!(102);
irq_handler!(103);
irq_handler!(104);
irq_handler!(105);
irq_handler!(106);
irq_handler!(107);
irq_handler!(108);
irq_handler!(109);
irq_handler!(110);
irq_handler!(111);
irq_handler!(112);
irq_handler!(113);
irq_handler!(114);
irq_handler!(115);
irq_handler!(116);
irq_handler!(117);
irq_handler!(118);
irq_handler!(119);
irq_handler!(120);
irq_handler!(121);
irq_handler!(122);
irq_handler!(123);
irq_handler!(124);
irq_handler!(125);
irq_handler!(126);
irq_handler!(127);
irq_handler!(128);
irq_handler!(129);
irq_handler!(130);
irq_handler!(131);
irq_handler!(132);
irq_handler!(133);
irq_handler!(134);
irq_handler!(135);
irq_handler!(136);
irq_handler!(137);
irq_handler!(138);
irq_handler!(139);
irq_handler!(140);
irq_handler!(141);
irq_handler!(142);
irq_handler!(143);
irq_handler!(144);
irq_handler!(145);
irq_handler!(146);
irq_handler!(147);
irq_handler!(148);
irq_handler!(149);
irq_handler!(150);
irq_handler!(151);
irq_handler!(152);
irq_handler!(153);
irq_handler!(154);
irq_handler!(155);
irq_handler!(156);
irq_handler!(157);
irq_handler!(158);
irq_handler!(159);
irq_handler!(160);
irq_handler!(161);
irq_handler!(162);
irq_handler!(163);
irq_handler!(164);
irq_handler!(165);
irq_handler!(166);
irq_handler!(167);
irq_handler!(168);
irq_handler!(169);
irq_handler!(170);
irq_handler!(171);
irq_handler!(172);
irq_handler!(173);
irq_handler!(174);
irq_handler!(175);
irq_handler!(176);
irq_handler!(177);
irq_handler!(178);
irq_handler!(179);
irq_handler!(180);
irq_handler!(181);
irq_handler!(182);
irq_handler!(183);
irq_handler!(184);
irq_handler!(185);
irq_handler!(186);
irq_handler!(187);
irq_handler!(188);
irq_handler!(189);
irq_handler!(190);
irq_handler!(191);
irq_handler!(192);
irq_handler!(193);
irq_handler!(194);
irq_handler!(195);
irq_handler!(196);
irq_handler!(197);
irq_handler!(198);
irq_handler!(199);
irq_handler!(200);
irq_handler!(201);
irq_handler!(202);
irq_handler!(203);
irq_handler!(204);
irq_handler!(205);
irq_handler!(206);
irq_handler!(207);
irq_handler!(208);
irq_handler!(209);
irq_handler!(210);
irq_handler!(211);
irq_handler!(212);
irq_handler!(213);
irq_handler!(214);
irq_handler!(215);
irq_handler!(216);
irq_handler!(217);
irq_handler!(218);
irq_handler!(219);
irq_handler!(220);
irq_handler!(221);
irq_handler!(222);
irq_handler!(223);
irq_handler!(224);
irq_handler!(225);
irq_handler!(226);
irq_handler!(227);
irq_handler!(228);
irq_handler!(229);
irq_handler!(230);
irq_handler!(231);
irq_handler!(232);
irq_handler!(233);
irq_handler!(234);
irq_handler!(235);
irq_handler!(236);
irq_handler!(237);
irq_handler!(238);
irq_handler!(239);
irq_handler!(240);
irq_handler!(241);
irq_handler!(242);
irq_handler!(243);
irq_handler!(244);
irq_handler!(245);
irq_handler!(246);
irq_handler!(247);
irq_handler!(248);
irq_handler!(249);
irq_handler!(250);
irq_handler!(251);
irq_handler!(252);
irq_handler!(253);
irq_handler!(254);
irq_handler!(255);


#[derive(Hal)]
struct Amd64Hal;

unsafe impl Hal for Amd64Hal {
	type SerialOut = serial::HalWriter;
	type KTableTy = paging2::Amd64KTable;
	type TTableTy = paging2::Amd64TTable;
	type SaveState = Amd64SaveState;

	fn breakpoint() { unsafe { asm!("int3"); } }

	fn exit(result: crate::hal::Result) -> ! {
		qemu::debug_exit(result)
	}

	fn debug_output(data: &[u8]) -> Result<(), ()> {
		qemu::debug_con_write(data);
		Ok(())
	}

	fn early_init() {
		let tss = tss::TSS.get_or_init(|| {
			// TODO: actually load stacks
			tss::Tss::new()
		});

		let gdt = gdt::GDT.get_or_init(|| {
			use gdt::{Entry, EntryTy, Privilege};

			let mut gdt = gdt::Gdt::new();
			gdt.add_entry(EntryTy::KernelCode, Entry::new(Privilege::Ring0, true, true));
			gdt.add_entry(EntryTy::KernelData, Entry::new(Privilege::Ring0, false, true));
			gdt.add_entry(EntryTy::UserLongCode, Entry::new(Privilege::Ring3, true, true));
			gdt.add_entry(EntryTy::UserData, Entry::new(Privilege::Ring3, false, true));
			gdt.add_tss(tss);
			gdt
		});

		gdt.load();
		gdt.load_tss();

		pic::init();

		Self::enable_interrupts();
	}

	fn init_idt() {
		let idt = idt::IDT.get_or_init(|| {
			macro_rules! idt_entry {
			    ($t:ident, $num:literal) => {
					$t[$num] = idt::entry::Entry::new_ptr(::paste::paste!([<amd64_irq_handler_ $num>]), None, 0, Type::InterruptGate);
			    };
			}

			let mut table = Idt::new();

			// Reserved exception numbers
			idt_entry!(table, 0);
			idt_entry!(table, 1);
			idt_entry!(table, 2);
			table[3] = idt::entry::Entry::new_ptr(amd64_irq_handler_3, None, 3, Type::InterruptGate);
			idt_entry!(table, 4);
			idt_entry!(table, 5);
			idt_entry!(table, 6);
			idt_entry!(table, 7);
			idt_entry!(table, 8);
			idt_entry!(table, 9);
			idt_entry!(table, 10);
			idt_entry!(table, 11);
			idt_entry!(table, 12);
			idt_entry!(table, 13);
			idt_entry!(table, 14);
			idt_entry!(table, 15);
			idt_entry!(table, 16);
			idt_entry!(table, 17);
			idt_entry!(table, 18);
			idt_entry!(table, 19);
			idt_entry!(table, 20);
			idt_entry!(table, 21);
			idt_entry!(table, 22);
			idt_entry!(table, 23);
			idt_entry!(table, 24);
			idt_entry!(table, 25);
			idt_entry!(table, 26);
			idt_entry!(table, 27);
			idt_entry!(table, 28);
			idt_entry!(table, 29);
			idt_entry!(table, 30);
			idt_entry!(table, 31);
			idt_entry!(table, 32);

			// lapic timer test
			idt_entry!(table, 33);

			// apic spurious irq
			idt_entry!(table, 255);

			table
		});
		idt.load();
	}

	fn enable_interrupts() {
		unsafe { asm!("sti", options(preserves_flags, nomem)); }
	}

	fn get_and_disable_interrupts() -> usize {
		let flags: usize;
		unsafe {
			asm!("
			pushf
			pop {}
			cli
		", out(reg) flags, options(preserves_flags, nomem))
		}

		flags & 0x0200
	}

	fn set_interrupts(old_state: usize) {
		if old_state != 0 {
			unsafe { asm!("sti", options(preserves_flags, nomem)); }
		}
	}

	unsafe fn load_tls(ptr: *mut u8) {
		let tls_self_ptr_low = ptr as usize as u32;
		let tls_self_ptr_high = ((ptr as usize) >> 32) as u32;
		unsafe {
			asm!(
				"mov ecx, 0xc0000100", // ecx = FSBase MSR
				"wrmsr",
				in("edx") tls_self_ptr_high, in("eax") tls_self_ptr_low, out("ecx") _
			);
		}
	}

	unsafe fn construct_tables() -> (Self::KTableTy, Self::TTableTy) {
		paging2::construct_tables()
	}

	#[naked]
	unsafe extern "C" fn switch_thread(from: &mut ThreadControlBlock, to: &ThreadControlBlock) {
		asm!(
			"mov [rdi + {0}], rbx",
			"mov [rdi + {1}], rsp",
			"mov [rdi + {2}], rbp",
			"mov [rdi + {3}], r12",
			"mov [rdi + {4}], r13",
			"mov [rdi + {5}], r14",
			"mov [rdi + {6}], r15",

			"mov rax, [rsi + {7}]",
			"mov rcx, cr3",
			"cmp rax, rcx",
			"je 1f",
			"mov cr3, rax",
			"1:",

			// todo: adjust RSP0 in TSS
			"mov rbx, [rsi + {0}]",
			"mov rsp, [rsi + {1}]",
			"mov rbp, [rsi + {2}]",
			"mov r12, [rsi + {3}]",
			"mov r13, [rsi + {4}]",
			"mov r14, [rsi + {5}]",
			"mov r15, [rsi + {6}]",

			"ret",

			const offset_of!(Amd64SaveState, rbx) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, rsp) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, rbp) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, r12) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, r13) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, r14) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(Amd64SaveState, r15) + offset_of!(ThreadControlBlock, save_state),
			const offset_of!(paging2::Amd64TTable, pml4) + offset_of!(ThreadControlBlock, ttable),
			options(noreturn)
		);
	}

	const MIN_IRQ_NUM: usize = 32;
	const MAX_IRQ_NUM: usize = 256;

	fn set_irq_handler(handler: extern "C" fn(usize)) {
		todo!()
	}
}

#[derive(Debug, Default)]
struct Amd64SaveState {
	pub rbx: usize,
	pub rsp: usize,
	pub rbp: usize,
	pub r12: usize,
	pub r13: usize,
	pub r14: usize,
	pub r15: usize
}

impl SaveState for Amd64SaveState {
	fn new(tcb: &mut ThreadControlBlock, init: fn(), main: fn() -> !) -> Self {
		let stack = &mut tcb.kernel_stack;
		let stack_start = unsafe {
			let stack_top = stack.virtual_end().start().as_ptr().cast::<usize>();
			stack_top.sub(1).write(0xdeadbeef);
			stack_top.sub(2).write(0);
			stack_top.sub(3).write(main as usize);
			stack_top.sub(4).write(init as usize);
			stack_top.sub(4)
		};

		Self {
			rsp: stack_start as usize,
			.. Self::default()
		}
	}
}

extern "x86-interrupt" fn breakpoint(frame: InterruptStackFrame) {
	warn!("BREAKPOINT: {frame:#x?}");
}
