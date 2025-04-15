macro_rules! percpu {
    ($vis:vis static $ident:ident: $ty:ty = $init:expr) => {
        #[inline(always)]
        $vis fn $ident() -> &'static $ty {
            #[repr(transparent)]
            struct Wrap($ty);

            unsafe impl Sync for Wrap {}

            #[link_section = concat!(".percpu.", stringify!($ident))]
            #[used]
            static PERCPU: Wrap = Wrap($init);

            extern "C" { static __percpu_end: u8; }

            let val: *mut $ty;
            unsafe {
                ::core::arch::asm!(
                    "mov {}, gs:[0]",
                    out(reg) val,
                    options(nostack, preserves_flags)
                );
            }
            unsafe {
                &*val.byte_offset(::core::ptr::addr_of!(__percpu_end).offset_from(::core::ptr::addr_of!(PERCPU).cast::<u8>()))
            }
        }
    };
}

pub(crate) use percpu;
