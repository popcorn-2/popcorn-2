use core::cell::{OnceCell, UnsafeCell};
use core::sync::atomic::AtomicPtr;
use kernel_api::sync::{IrqCell, RwSpinlock};
use crate::threading::ThreadControlBlock;
use crate::timing::TimerQueue;

macro_rules! percpu_gen {
    (pub struct $ident:ident {
	    $($(#[$attr:meta])* pub $field:ident: $ty:ty = $init:expr),* $(,)?
    }) => {
	    const PERCPU_INIT: $ident = $ident {
            $($(#[$attr])* $field: $init),* ,
            percpu: ::core::ptr::null_mut(),
        };

	    #[repr(transparent)]
	    struct BspWrapper($ident);

	    // SAFETY: `BspWrapper` is only used for the BSP percpu struct and can only be
	    //  accessed from the BSP
	    unsafe impl Sync for BspWrapper {}

	    static mut BSP_PERCPU: BspWrapper = {
		    let mut percpu = PERCPU_INIT;
		    // SAFETY: runs during init so cannot be accessed elsewhere
		    percpu.percpu = unsafe { &raw mut BSP_PERCPU.0 };
		    BspWrapper(percpu)
	    };

        pub struct $ident {
	        $($(#[$attr])* pub $field: $ty),* ,
            pub percpu: *mut $ident,
        }

        impl $ident {
		    pub fn bsp_init() {
			    unsafe { crate::hal::load_tls(BSP_PERCPU.0.percpu.cast()); }
			}

            pub fn ap_init() {
                let data = ::alloc::boxed::Box::leak(
                    ::alloc::boxed::Box::new(
                        $ident {
                            $($(#[$attr])* $field: $init),* ,
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
            ($field) => {{
	            let val: *mut $crate::percpu::Percpu;

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
                unsafe { &(*val).$field }
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
		#[cfg(feature = "hal-next")] pub arch: crate::arch::Percpu = crate::arch::Percpu::new(),
    }
}

pub(crate) use percpu_v2;
