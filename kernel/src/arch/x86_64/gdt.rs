use core::mem::offset_of;
use core::ptr;
use kernel_api::num::ufat;
use kernel_api::sync::LazyLock;

#[repr(C, align(8))]
pub struct Gdt {
	null: Entry,
	kernel_code: Entry,
	kernel_data: Entry,
	user_compat_code: Entry,
	user_data: Entry,
	user_long_code: Entry,
	tss: SystemEntry,
}

impl Gdt {
	pub const KERNEL_CODE_SEGMENT: usize = offset_of!(Self, kernel_code);
	pub const KERNEL_DATA_SEGMENT: usize = offset_of!(Self, kernel_data);
	pub const USER_CODE_SEGMENT: usize = offset_of!(Self, user_long_code);
	pub const USER_DATA_SEGMENT: usize = offset_of!(Self, user_data);
	pub const TSS_SEGMENT: usize = offset_of!(Self, tss);

	pub const INIT: LazyLock<Gdt> = LazyLock::new(|| {
		let mut gdt = Gdt::new();
		gdt.tss = SystemEntry::from_tss(&super::tss::BSP_TSS);
		gdt
	});

	fn new() -> Self {
		Self {
			null: Entry::NULL,
			kernel_code: Entry::KERNEL_CODE,
			kernel_data: Entry::KERNEL_DATA,
			user_compat_code: Entry::NULL,
			user_data: Entry::USER_DATA,
			user_long_code: Entry::USER_CODE,
			tss: SystemEntry::NULL,
		}
	}

	pub fn load(&'static self) {
		#[repr(C, packed)]
		struct Pointer {
			limit: u16,
			address: &'static Gdt
		}

		let ptr = Pointer {
			limit: size_of::<Self>().truncate::<u16>() - 1,
			address: self,
		};

		unsafe {
			core::arch::asm!(
				"lgdt [{pointer}]",
				"push {code_segment}",
				"lea {scratch:r}, [rip + 2f]",
				"push {scratch:r}",
				"retfq",
				"2: mov {scratch:x}, {data_segment}",
				"mov ds, {scratch:x}",
				"mov es, {scratch:x}",
				"mov ss, {scratch:x}",
				"ltr {tss_segment:x}",

				pointer = in(reg) &ptr,
				scratch = out(reg) _,
				code_segment = const Self::KERNEL_CODE_SEGMENT,
				data_segment = const Self::KERNEL_DATA_SEGMENT,
				tss_segment = in(reg) Self::TSS_SEGMENT,
			);
		}
	}
}

#[repr(C)]
struct Entry(usize);

#[repr(C)]
struct SystemEntry(ufat);

impl Entry {
	const NULL: Self = Self(0);
	const KERNEL_CODE: Self = Self::new(0, true, true);
	const KERNEL_DATA: Self = Self::new(0, false, true);
	const USER_CODE: Self = Self::new(3, true, true);
	const USER_DATA: Self = Self::new(3, false, true);

	const fn new(dpl: usize, executable: bool, long_mode: bool) -> Self {
		debug_assert!(dpl < 4, "DPL can only be 0, 1, 2, 3");
		let access = (1 << 0) | // accessed
			(1 << 1) | // writable
			(usize::from(executable) << 3) |
			(1 << 4) | // non-system
			(dpl << 5) |
			(1 << 7); // present

		Entry(
			0xFFFF | // limit low
			(access << 40) |
			(0xF << 48) | // limit high
			(usize::from(long_mode & executable) << 53) |
			(1 << 55) // granularity
		)
	}
}

impl SystemEntry {
	const NULL: Self = Self(ufat::new(0, 0));

	fn from_tss(tss: &'static super::tss::Tss) -> Self {
		let base = ptr::from_ref(tss).expose_provenance();
		let limit = size_of::<super::tss::Tss>() - 1;

		SystemEntry(
			ufat::new(
				base >> 32,
				(limit & 0xFFFF) |
					(base & 0xFFFFFF) << 16 |
					(0x9 << 40) | // non-busy 64 bit TSS
					(1 << 47) | // present
					((base >> 24) & 0xFF) << 56,
			)
		)
	}
}
