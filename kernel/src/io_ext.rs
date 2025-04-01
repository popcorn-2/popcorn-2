use core::sync::atomic::{
	AtomicI8, AtomicI16, AtomicI32, AtomicI64, AtomicI128, AtomicIsize,
	AtomicU8, AtomicU16, AtomicU32, AtomicU64, AtomicU128, AtomicUsize,
	Ordering
};

mod private {
	pub trait Sealed {}
}

pub trait IoExt: private::Sealed + Sized {
	unsafe fn compare_exchange_io(
		self: *mut Self,
		current: Self,
		new: Self,
	) -> Result<Self, Self>;

	unsafe fn compare_exchange_weak_io(
		self: *mut Self,
		current: Self,
		new: Self,
	) -> Result<Self, Self>;

	unsafe fn fetch_add_io(self: *mut Self, val: Self) -> Self;
	unsafe fn fetch_sub_io(self: *mut Self, val: Self) -> Self;
	unsafe fn fetch_and_io(self: *mut Self, val: Self) -> Self;
	unsafe fn fetch_nand_io(self: *mut Self, val: Self) -> Self;
	unsafe fn fetch_or_io(self: *mut Self, val: Self) -> Self;
	unsafe fn fetch_xor_io(self: *mut Self, val: Self) -> Self;

	unsafe fn fetch_update_io(
		self: *mut Self,
		f: impl FnMut(Self) -> Option<Self>,
	) -> Result<Self, Self>;

	unsafe fn load_io(self: *mut Self) -> Self;
	unsafe fn store_io(self: *mut Self, val: Self);
	unsafe fn swap_io(self: *mut Self, val: Self) -> Self;
}

macro_rules! io_ext_impl {
    ($ty:ty, $atomic_ty:ty) => {
	    impl private::Sealed for $ty {}
	    
	    impl IoExt for $ty {
		    unsafe fn compare_exchange_io(
				self: *mut Self,
				current: Self,
				new: Self,
			) -> Result<Self, Self> {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.compare_exchange(::core::hint::black_box(current), ::core::hint::black_box(new), Ordering::SeqCst, Ordering::SeqCst))
		    }
		
			unsafe fn compare_exchange_weak_io(
				self: *mut Self,
				current: Self,
				new: Self,
			) -> Result<Self, Self> {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.compare_exchange_weak(::core::hint::black_box(current), ::core::hint::black_box(new), Ordering::SeqCst, Ordering::SeqCst))
		    }
		
			unsafe fn fetch_add_io(self: *mut Self, val: Self) -> Self {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.fetch_add(::core::hint::black_box(val), Ordering::SeqCst))
		    }
		    
			unsafe fn fetch_sub_io(self: *mut Self, val: Self) -> Self {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.fetch_sub(::core::hint::black_box(val), Ordering::SeqCst))
		    }
		    
			unsafe fn fetch_and_io(self: *mut Self, val: Self) -> Self {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.fetch_and(::core::hint::black_box(val), Ordering::SeqCst))
		    }
		    
			unsafe fn fetch_nand_io(self: *mut Self, val: Self) -> Self {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.fetch_nand(::core::hint::black_box(val), Ordering::SeqCst))
		    }
		    
			unsafe fn fetch_or_io(self: *mut Self, val: Self) -> Self {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.fetch_or(::core::hint::black_box(val), Ordering::SeqCst))
		    }
		    
			unsafe fn fetch_xor_io(self: *mut Self, val: Self) -> Self {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.fetch_xor(::core::hint::black_box(val), Ordering::SeqCst))
		    }
		    
			unsafe fn fetch_update_io(
				self: *mut Self,
				mut f: impl FnMut(Self) -> Option<Self>,
			) -> Result<Self, Self> {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.fetch_update(Ordering::SeqCst, Ordering::SeqCst, |v| ::core::hint::black_box(f(::core::hint::black_box(v)))))
		    }
		
			unsafe fn load_io(self: *mut Self) -> Self {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.load(Ordering::SeqCst))
		    }
		    
			unsafe fn store_io(self: *mut Self, val: Self) {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.store(::core::hint::black_box(val), Ordering::SeqCst));
		    }
		    
			unsafe fn swap_io(self: *mut Self, val: Self) -> Self {
			    let atomic = unsafe { <$atomic_ty>::from_ptr(self) };
			    ::core::hint::black_box(atomic.swap(::core::hint::black_box(val), Ordering::SeqCst))
		    }
	    }
    };
}

io_ext_impl!(i8, AtomicI8);
io_ext_impl!(i16, AtomicI16);
io_ext_impl!(i32, AtomicI32);
io_ext_impl!(i64, AtomicI64);
io_ext_impl!(i128, AtomicI128);
io_ext_impl!(isize, AtomicIsize);

io_ext_impl!(u8, AtomicU8);
io_ext_impl!(u16, AtomicU16);
io_ext_impl!(u32, AtomicU32);
io_ext_impl!(u64, AtomicU64);
io_ext_impl!(u128, AtomicU128);
io_ext_impl!(usize, AtomicUsize);
