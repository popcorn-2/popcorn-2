use core::cell::{OnceCell, UnsafeCell};
use core::sync::atomic::AtomicPtr;
use kernel_api::sync::{IrqCell, RwSpinlock};
use crate::threading::ThreadControlBlock;
use crate::timing::TimerQueue;

macro_rules! percpu_gen {
    (pub struct $ident:ident {
        $(pub $field:ident: $ty:ty = $init:expr),* $(,)?
    }) => {
        pub struct $ident {
            $(pub $field: $ty),* ,
            pub percpu: *mut $ident,
        }

        impl $ident {
            pub fn init() {
                let data = ::alloc::boxed::Box::leak(
                    ::alloc::boxed::Box::new(
                        $ident {
                            $($field: $init),* ,
                            percpu: ::core::ptr::null_mut(),
                        }
                    )
                );
                data.percpu = data as *mut _;
                unsafe { crate::hal::load_tls((data as *mut $ident).cast()); }
            }
        }

        macro_rules! percpu_v2 {
            $(
            (@ $field) => {{
	            let val: *mut $crate::percpu::Percpu;
	            let msr_val: i32;

	            #[allow(unused_unsafe)]
                unsafe {
                    ::core::arch::asm!(
                        "rdmsr",
                        out("eax") _,
                        out("edx") msr_val,
                        in("ecx") 0xC0000101u32,
                        options(nostack, preserves_flags, pure, readonly)
                    );
                }

	            if msr_val >= 0 {
	                None
                } else {
		            #[allow(unused_unsafe)]
	                unsafe {
	                    ::core::arch::asm!(
	                        "mov {}, gs:[{}]",
	                        out(reg) val,
	                        const ::core::mem::offset_of!($crate::percpu::Percpu, percpu),
	                        options(nostack, preserves_flags, pure, readonly)
	                    );
	                }

		            #[allow(unused_unsafe)]
                    Some(unsafe { &(*val).$field })
	            }
            }};
            ($field) => {{
	            #[allow(unused_unsafe)]
	            unsafe { percpu_v2!(@ $field).unwrap_unchecked() }
            }};
            )*
        }
    };
}

percpu_gen! {
    pub struct Percpu {
        pub foo: UnsafeCell<usize> = UnsafeCell::new(6),
        pub local_timer_queue: TimerQueue = TimerQueue::new(),
        pub kernel_stack_top: AtomicPtr<u8> = AtomicPtr::new(core::ptr::null_mut()),
        pub scheduler: OnceCell<IrqCell<crate::threading::SchedulerTy>> = OnceCell::new(),
        pub local_timer: OnceCell<crate::hal::timing::TimerMeta> = OnceCell::new(),
        pub current_thread: RwSpinlock<Option<ThreadControlBlock>> = RwSpinlock::new(None), // todo: replace with something !Sync if opt needed
    }
}

pub(crate) use percpu_v2;
