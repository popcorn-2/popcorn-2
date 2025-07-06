use alloc::sync::Arc;
use core::cell::{LazyCell, OnceCell, UnsafeCell};
use core::sync::atomic::AtomicUsize;
use crossbeam_queue::SegQueue;
use kernel_api::sync::{IrqCell, RwSpinlock};
use crate::threading::{ControlEvent, Thread, ThreadId, ThreadPointer};
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
            $(($field) => {{
                let val: *mut $crate::percpu::Percpu;

                #[allow(unused_unsafe)]
                unsafe {
                    ::core::arch::asm!(
                        "mov {}, gs:[{}]",
                        out(reg) val,
                        const ::core::mem::offset_of!($crate::percpu::Percpu, percpu),
                        options(nostack, preserves_flags)
                    );
                }

                #[allow(unused_unsafe)]
                unsafe { &(*val).$field }
            }};)*
        }
    };
}

percpu_gen! {
    pub struct Percpu {
        pub foo: UnsafeCell<usize> = UnsafeCell::new(6),
        pub local_timer_queue: TimerQueue = TimerQueue::new(),
        pub kernel_stack_top: AtomicUsize = AtomicUsize::new(0),
        pub scheduler: OnceCell<(IrqCell<crate::threading::SchedulerTy>, Arc<SegQueue<ControlEvent>>)> = OnceCell::new(),
        pub idle_thread: LazyCell<(ThreadId, Thread, UnsafeCell<ThreadPointer>)> = LazyCell::new(crate::threading::create_idle_thread),
        pub local_timer: OnceCell<crate::hal::timing::TimerMeta> = OnceCell::new(),
        pub current_thread: RwSpinlock<Option<ThreadPointer>> = RwSpinlock::new(None),
    }
}

pub(crate) use percpu_v2;
