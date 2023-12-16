use core::arch::asm;
use core::mem::offset_of;
use kernel_api::memory::{PhysicalAddress, VirtualAddress};
use crate::tss::{Tss, TSS};

#[used]
static mut GDT: Gdt = {
    let mut gdt = Gdt::new();
    gdt.add_entry(EntryTy::KernelCode, Entry::new(Privilege::Ring0, true, true));
    gdt.add_entry(EntryTy::KernelData, Entry::new(Privilege::Ring0, false, true));
    gdt.add_entry(EntryTy::UserLongCode, Entry::new(Privilege::Ring3, true, true));
    gdt.add_entry(EntryTy::UserData, Entry::new(Privilege::Ring3, false, true));
    gdt
};

pub fn init_gdt() {
    // safety: kernel will only cause this from a single core
    unsafe {
        GDT.add_tss(&TSS);
        GDT.load();
    }
}

//#[derive_const(Default)]
#[repr(C, align(8))]
pub struct Gdt {
    null: Entry,
    kernel_code: Entry,
    kernel_data: Entry,
    user_compat_code: Entry,
    user_data: Entry,
    user_long_code: Entry,
    tss: SystemEntry
}

//#[derive_const(Default)]
#[repr(C)]
struct Entry(u64);
//#[derive_const(Default)]
#[repr(C)]
struct SystemEntry(u64, u64);

// TODO: replace with `#[derive_const(Default)]` when it lands (again)
mod const_default {
    use super::{Entry, SystemEntry};

    impl super::Gdt {
        pub const fn default() -> super::Gdt {
            super::Gdt {
                null: Entry::default(),
                kernel_code: Entry::default(),
                kernel_data: Entry::default(),
                user_compat_code: Entry::default(),
                user_data: Entry::default(),
                user_long_code: Entry::default(),
                tss: SystemEntry::default(),
            }
        }
    }

    impl Entry {
        pub const fn default() -> Entry {
           Entry(0)
        }
    }

    impl SystemEntry {
        pub const fn default() -> SystemEntry {
            SystemEntry(0, 0)
        }
    }
}

impl Gdt {
    const fn new() -> Gdt {
        Gdt::default()
    }

    const fn add_entry(&mut self, ty: EntryTy, entry: Entry) {
        let e = match ty {
            EntryTy::KernelCode => &mut self.kernel_code,
            EntryTy::KernelData => &mut self.kernel_data,
            EntryTy::UserCompatCode => &mut self.user_compat_code,
            EntryTy::UserData => &mut self.user_data,
            EntryTy::UserLongCode => &mut self.user_long_code,
        };

        *e = entry;
    }

    fn add_tss(&mut self, tss: &'static Tss) {
        self.tss = SystemEntry::new_from_tss(tss);
        // SAFETY: We just loaded the TSS into this index and its valid for a 'static lifetime
        unsafe { Tss::load(offset_of!(Gdt, tss).try_into().unwrap()); }
    }

    fn load(&'static self) {
        use core::mem::size_of_val;
        let ptr = Pointer {
            size: size_of_val(self).try_into().unwrap(),
            address: self,
        };

        unsafe {
            asm!("lgdt [{}]", in(reg) &ptr);
            asm!("
                push {0}
                lea {2:r}, [rip + 2f]
                push {2:r}
                retfq
                2:
                mov {2:x}, {1}
                mov ds, {2:x}
                mov es, {2:x}
                mov fs, {2:x}
                mov gs, {2:x}
                mov ss, {2:x}
            ", const offset_of!(Gdt, kernel_code), const offset_of!(Gdt, kernel_data), out(reg) _);
        }
    }
}

impl Entry {
    const fn new(dpl: Privilege, executable: bool, long_mode: bool) -> Entry {
        const ADDR: u64 = 0;
        const LIMIT: u64 = 0xFFFFF;
        const ACCESS_BYTE_DEFAULT: u64 = 0b1001_0010;

        let mut data = 0;
        let mut access_byte: u64 = ACCESS_BYTE_DEFAULT | (dpl.const_into() << 5) | (if executable {1} else {0} << 3);

        data |= LIMIT & 0xFFFF;
        data |= (ADDR & 0xFFFF) << 16;
        data |= ((ADDR >> 16) & 0xFF) << 32;
        data |= (access_byte & 0xFF) << 40;
        data |= ((LIMIT >> 16) & 0xF) << 48;
        data |= if long_mode {1} else {0} << 53;
        data |= 1 << 55; // granularity
        data |= ((ADDR >> 24) & 0xFF) << 56;

        Entry(data)
    }
}

impl SystemEntry {
    fn new_from_tss(tss: &'static Tss) -> SystemEntry {
        use core::mem::size_of_val;

        let mut low = 0u64;
        let addr: u64 = (tss as *const _ as usize).try_into().unwrap();
        let limit: u64 =  size_of_val(tss).try_into().unwrap();

        const ACCESS_BYTE_DEFAULT: u64 = 0b1000_1001;

        low |= limit & 0xFFFF;
        low |= (addr & 0xFFFF) << 16;
        low |= ((addr >> 16) & 0xFF) << 32;
        low |= (ACCESS_BYTE_DEFAULT & 0xFF) << 40;
        low |= ((limit >> 16) & 0xF) << 48;
        low |= 1 << 53; // long mode
        low |= ((addr >> 24) & 0xFF) << 56;

        SystemEntry(low, (tss as *const _ as u64) >> 32)
    }
}

#[repr(C, packed)]
pub struct Pointer {
    size: u16,
    address: &'static Gdt
}

impl From<&'static Gdt> for Pointer {
    fn from(value: &'static Gdt) -> Self {
        todo!()
    }
}

#[derive(Debug, Copy, Clone)]
enum Privilege {
    Ring0 = 0,
    Ring3 = 3
}

impl Privilege {
    // TODO: replace with `const From<T>` when it lands (again)
    const fn const_into(self) -> u64 {
        match self {
            Privilege::Ring0 => 0,
            Privilege::Ring3 => 3
        }
    }
}

/*impl const From<Privilege> for u64 {
    fn from(value: Privilege) -> Self {
        match value {
            Privilege::Ring0 => 0,
            Privilege::Ring3 => 3
        }
    }
}*/

#[derive(Debug, Copy, Clone)]
enum EntryTy {
    KernelCode,
    KernelData,
    UserCompatCode,
    UserData,
    UserLongCode
}

impl EntryTy {
    fn is_data(self) -> bool {
        match self {
            Self::KernelData | Self::UserData => true,
            _ => false
        }
    }

    fn is_executable(self) -> bool {
        !self.is_data()
    }
}
