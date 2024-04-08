use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::cell::OnceCell;
use log::{debug, warn};
use kernel_api::sync::{Mutex, MutexGuard};

pub macro irq_handler {
	(
	    $($vis:vis fn $name:ident() {
	        main => $main_block:block
	        eoi => $eoi_block:block
	    })*
	) => {
	    $($vis fn $name() {
		    $main_block
		    <$crate::hal::HalTy as $crate::hal::Hal>::get_and_disable_interrupts();
		    $eoi_block
	    })*
	},

	(
		|| {
			main => $main_block:block
	        eoi => $eoi_block:block
		}
	) => {
		|| {
			$main_block
		    <$crate::hal::HalTy as $crate::hal::Hal>::get_and_disable_interrupts();
		    $eoi_block
		}
	},

	(
		move || {
			main => $main_block:block
	        eoi => $eoi_block:block
		}
	) => {
		move || {
			$main_block
		    <$crate::hal::HalTy as $crate::hal::Hal>::get_and_disable_interrupts();
		    $eoi_block
		}
	},
}

#[thread_local]
static IRQ_HANDLES: Mutex<BTreeMap<usize, Box<dyn FnMut() /* + Send ???*/>>> = Mutex::new(BTreeMap::new());

#[thread_local]
static DEFER_IRQ: OnceCell<Box<dyn Fn()>> = OnceCell::new();

pub fn global_irq_handler(vector: usize) {
	use kernel_api::sync::MutexGuardExt as _;
	
	if vector == 0x30 { (DEFER_IRQ.get().unwrap())(); return; }
	
	let mut guard = IRQ_HANDLES.lock();
	if let Some(f) = guard.get_mut(&vector) {
		(*f)();
	} else {
		warn!("Unhandled IRQ: vector {vector}");
	}
	MutexGuard::unlock_no_interrupts(guard);
}

pub fn insert_handler(vector: usize, f: impl FnMut() + 'static) -> Option<()> {
	IRQ_HANDLES.lock().insert(vector, Box::new(f)).map(|_| ())
}

pub fn set_defer_irq(f: impl Fn() + 'static) {
	DEFER_IRQ.set(Box::new(f)).unwrap_or_else(|_| panic!());
}
