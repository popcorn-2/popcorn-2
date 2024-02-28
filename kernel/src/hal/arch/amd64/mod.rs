use core::arch::{asm, global_asm};
use core::mem;
use core::mem::offset_of;
use log::warn;
use crate::hal::{Hal, SaveState, ThreadControlBlock};
use crate::hal::arch::amd64::idt::entry::Type;
use crate::hal::arch::amd64::idt::handler::InterruptStackFrame;
use crate::hal::arch::amd64::idt::Idt;
use crate::hal::exception::{DebugTy, Exception, PageFault, Ty};
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
	const MIN_IRQ: u8 = Amd64Hal::MIN_IRQ_NUM as u8;
	const MAX_IRQ: u8 = Amd64Hal::MAX_IRQ_NUM as u8;

	let exception_payload = match data.num as u8 {
		0 | 16 | 19 => Exception {
			ty: Ty::FloatingPoint,
			at_instruction: data.rip as usize,
		},
		1 | 3 => Exception {
			ty: Ty::Debug(DebugTy::Breakpoint),
			at_instruction: data.rip as usize,
		},
		6 => Exception {
			ty: Ty::IllegalInstruction,
			at_instruction: data.rip as usize,
		},
		14 => {
			let cr2: usize;
			unsafe { asm!("mov {}, cr2", out(reg) cr2); }
			Exception {
				ty: Ty::PageFault(PageFault { access_addr: cr2 }),
				at_instruction: data.rip as usize,
			}
		},
		7 | 17 => Exception {
			ty: Ty::BusFault,
			at_instruction: data.rip as usize,
		},
		2 => Exception {
			ty: Ty::Nmi,
			at_instruction: data.rip as usize,
		},
		8 => Exception {
			ty: Ty::Panic,
			at_instruction: data.rip as usize,
		},
		e @ (4 | 5 | 9..= 13 | 15 | 18 | 21..=27 | 31) => {
			let reason = match e {
				4 => "Overflow check",
				5 => "Bound check",
				9 | 15 | 22..=27 | 31 => "Reserved",
				10 => "Invalid TSS",
				11 => "Segment not present",
				12 => "Stack segment fault",
				13 => "General protection fault",
				18 => "Machine check",
				21 => "Control protection exception",
				_ => unreachable!(),
			};
			Exception {
				ty: Ty::Generic(reason),
				at_instruction: data.rip as usize,
			}
		},
		e @ (20 | 28..=30) => {
			let reason = match e {
				20 => "Virtualization exception",
				28 => "Hypervisor injection",
				29 => "VMM communication exception",
				30 => "Security exception",
				_ => unreachable!(),
			};
			Exception {
				ty: Ty::Unknown(reason),
				at_instruction: data.rip as usize,
			}
		},
		e @ 32..48 => {
			warn!("Spurious PIC irq - vector {}", e - 32);
			return;
		},
		MIN_IRQ..MAX_IRQ=> {
			crate::irq_handler(data.num as usize);
			return;
		},
		255 => {
			warn!("Spurious APIC irq");
			return;
		},
	};
	crate::exception_handler(exception_payload);
}

#[naked]
unsafe extern "C" fn amd64_syscall_handler() {
	asm!("ud2", options(noreturn));
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
		"pop rax",
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

impl Amd64Hal {
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
			idt_entry!(table, 33);
			idt_entry!(table, 34);
			idt_entry!(table, 35);
			idt_entry!(table, 36);
			idt_entry!(table, 37);
			idt_entry!(table, 38);
			idt_entry!(table, 39);
			idt_entry!(table, 40);
			idt_entry!(table, 41);
			idt_entry!(table, 42);
			idt_entry!(table, 43);
			idt_entry!(table, 44);
			idt_entry!(table, 45);
			idt_entry!(table, 46);
			idt_entry!(table, 47);
			idt_entry!(table, 48);
			idt_entry!(table, 49);
			idt_entry!(table, 50);
			idt_entry!(table, 51);
			idt_entry!(table, 52);
			idt_entry!(table, 53);
			idt_entry!(table, 54);
			idt_entry!(table, 55);
			idt_entry!(table, 56);
			idt_entry!(table, 57);
			idt_entry!(table, 58);
			idt_entry!(table, 59);
			idt_entry!(table, 60);
			idt_entry!(table, 61);
			idt_entry!(table, 62);
			idt_entry!(table, 63);
			idt_entry!(table, 64);
			idt_entry!(table, 65);
			idt_entry!(table, 66);
			idt_entry!(table, 67);
			idt_entry!(table, 68);
			idt_entry!(table, 69);
			idt_entry!(table, 70);
			idt_entry!(table, 71);
			idt_entry!(table, 72);
			idt_entry!(table, 73);
			idt_entry!(table, 74);
			idt_entry!(table, 75);
			idt_entry!(table, 76);
			idt_entry!(table, 77);
			idt_entry!(table, 78);
			idt_entry!(table, 79);
			idt_entry!(table, 80);
			idt_entry!(table, 81);
			idt_entry!(table, 82);
			idt_entry!(table, 83);
			idt_entry!(table, 84);
			idt_entry!(table, 85);
			idt_entry!(table, 86);
			idt_entry!(table, 87);
			idt_entry!(table, 88);
			idt_entry!(table, 89);
			idt_entry!(table, 90);
			idt_entry!(table, 91);
			idt_entry!(table, 92);
			idt_entry!(table, 93);
			idt_entry!(table, 94);
			idt_entry!(table, 95);
			idt_entry!(table, 96);
			idt_entry!(table, 97);
			idt_entry!(table, 98);
			idt_entry!(table, 99);
			idt_entry!(table, 100);
			idt_entry!(table, 101);
			idt_entry!(table, 102);
			idt_entry!(table, 103);
			idt_entry!(table, 104);
			idt_entry!(table, 105);
			idt_entry!(table, 106);
			idt_entry!(table, 107);
			idt_entry!(table, 108);
			idt_entry!(table, 109);
			idt_entry!(table, 110);
			idt_entry!(table, 111);
			idt_entry!(table, 112);
			idt_entry!(table, 113);
			idt_entry!(table, 114);
			idt_entry!(table, 115);
			idt_entry!(table, 116);
			idt_entry!(table, 117);
			idt_entry!(table, 118);
			idt_entry!(table, 119);
			idt_entry!(table, 120);
			idt_entry!(table, 121);
			idt_entry!(table, 122);
			idt_entry!(table, 123);
			idt_entry!(table, 124);
			idt_entry!(table, 125);
			idt_entry!(table, 126);
			idt_entry!(table, 127);
			idt_entry!(table, 128);
			idt_entry!(table, 129);
			idt_entry!(table, 130);
			idt_entry!(table, 131);
			idt_entry!(table, 132);
			idt_entry!(table, 133);
			idt_entry!(table, 134);
			idt_entry!(table, 135);
			idt_entry!(table, 136);
			idt_entry!(table, 137);
			idt_entry!(table, 138);
			idt_entry!(table, 139);
			idt_entry!(table, 140);
			idt_entry!(table, 141);
			idt_entry!(table, 142);
			idt_entry!(table, 143);
			idt_entry!(table, 144);
			idt_entry!(table, 145);
			idt_entry!(table, 146);
			idt_entry!(table, 147);
			idt_entry!(table, 148);
			idt_entry!(table, 149);
			idt_entry!(table, 150);
			idt_entry!(table, 151);
			idt_entry!(table, 152);
			idt_entry!(table, 153);
			idt_entry!(table, 154);
			idt_entry!(table, 155);
			idt_entry!(table, 156);
			idt_entry!(table, 157);
			idt_entry!(table, 158);
			idt_entry!(table, 159);
			idt_entry!(table, 160);
			idt_entry!(table, 161);
			idt_entry!(table, 162);
			idt_entry!(table, 163);
			idt_entry!(table, 164);
			idt_entry!(table, 165);
			idt_entry!(table, 166);
			idt_entry!(table, 167);
			idt_entry!(table, 168);
			idt_entry!(table, 169);
			idt_entry!(table, 170);
			idt_entry!(table, 171);
			idt_entry!(table, 172);
			idt_entry!(table, 173);
			idt_entry!(table, 174);
			idt_entry!(table, 175);
			idt_entry!(table, 176);
			idt_entry!(table, 177);
			idt_entry!(table, 178);
			idt_entry!(table, 179);
			idt_entry!(table, 180);
			idt_entry!(table, 181);
			idt_entry!(table, 182);
			idt_entry!(table, 183);
			idt_entry!(table, 184);
			idt_entry!(table, 185);
			idt_entry!(table, 186);
			idt_entry!(table, 187);
			idt_entry!(table, 188);
			idt_entry!(table, 189);
			idt_entry!(table, 190);
			idt_entry!(table, 191);
			idt_entry!(table, 192);
			idt_entry!(table, 193);
			idt_entry!(table, 194);
			idt_entry!(table, 195);
			idt_entry!(table, 196);
			idt_entry!(table, 197);
			idt_entry!(table, 198);
			idt_entry!(table, 199);
			idt_entry!(table, 200);
			idt_entry!(table, 201);
			idt_entry!(table, 202);
			idt_entry!(table, 203);
			idt_entry!(table, 204);
			idt_entry!(table, 205);
			idt_entry!(table, 206);
			idt_entry!(table, 207);
			idt_entry!(table, 208);
			idt_entry!(table, 209);
			idt_entry!(table, 210);
			idt_entry!(table, 211);
			idt_entry!(table, 212);
			idt_entry!(table, 213);
			idt_entry!(table, 214);
			idt_entry!(table, 215);
			idt_entry!(table, 216);
			idt_entry!(table, 217);
			idt_entry!(table, 218);
			idt_entry!(table, 219);
			idt_entry!(table, 220);
			idt_entry!(table, 221);
			idt_entry!(table, 222);
			idt_entry!(table, 223);
			idt_entry!(table, 224);
			idt_entry!(table, 225);
			idt_entry!(table, 226);
			idt_entry!(table, 227);
			idt_entry!(table, 228);
			idt_entry!(table, 229);
			idt_entry!(table, 230);
			idt_entry!(table, 231);
			idt_entry!(table, 232);
			idt_entry!(table, 233);
			idt_entry!(table, 234);
			idt_entry!(table, 235);
			idt_entry!(table, 236);
			idt_entry!(table, 237);
			idt_entry!(table, 238);
			idt_entry!(table, 239);
			idt_entry!(table, 240);
			idt_entry!(table, 241);
			idt_entry!(table, 242);
			idt_entry!(table, 243);
			idt_entry!(table, 244);
			idt_entry!(table, 245);
			idt_entry!(table, 246);
			idt_entry!(table, 247);
			idt_entry!(table, 248);
			idt_entry!(table, 249);
			idt_entry!(table, 250);
			idt_entry!(table, 251);
			idt_entry!(table, 252);
			idt_entry!(table, 253);
			idt_entry!(table, 254);
			idt_entry!(table, 255);

			table
		});
		idt.load();
	}
}

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

		Self::init_idt();

		pic::init();

		Self::enable_interrupts();
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

	const MIN_IRQ_NUM: usize = 48; // 0-32 for exceptions, 32-48 for masked pic
	const MAX_IRQ_NUM: usize = 255; // 255 for spurious apic
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
