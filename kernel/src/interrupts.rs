use alloc::boxed::Box;
use alloc::collections::BTreeMap;
use core::cell::OnceCell;
use log::{debug, warn};
use kernel_api::sync::Mutex;

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
	if vector == 0x30 { (DEFER_IRQ.get().unwrap())(); return; }
	
	let flags: u64;
	unsafe { core::arch::asm!("pushf; pop {}", out(reg) flags); }
	debug!("flags: {flags:#x}");
	if let Some(f) = IRQ_HANDLES.lock().get_mut(&vector) {
		let flags: u64;
		unsafe { core::arch::asm!("pushf; pop {}", out(reg) flags); }
		debug!("flags: {flags:#x}");
		(*f)();
	} else {
		warn!("Unhandled IRQ: vector {vector}");
	}
}

pub fn insert_handler(vector: usize, f: impl FnMut() + 'static) -> Option<()> {
	IRQ_HANDLES.lock().insert(vector, Box::new(f)).map(|_| ())
}

pub fn set_defer_irq(f: impl Fn() + 'static) {
	DEFER_IRQ.set(Box::new(f)).unwrap_or_else(|_| panic!());
}
