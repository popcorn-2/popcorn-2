use core::sync::atomic::{AtomicUsize, Ordering};
use crate::detect::Feature;

static CACHE: AtomicUsize = AtomicUsize::new(0);

const CAPACITY: usize = (core::mem::size_of::<usize>() * 8 - 1);
const INIT_BIT: usize = 1 << CAPACITY;
const INIT_MASK: usize = INIT_BIT - 1;

pub struct Initializer(usize);

impl Initializer {
	pub const fn new() -> Self { Self(0) }
	
	pub fn set(&mut self, feature: Feature) {
		let bit = feature as usize;
		debug_assert!(
			bit < CAPACITY,
			"Too many features",
		);
		
		self.0 |= 1 << bit;
	}
}

#[cold]
fn detect_and_initialize() {
	debug_assert_eq!(
		CACHE.load(Ordering::Relaxed) & INIT_MASK,
		0,
		"Cache is already initialised"
	);

	let features = super::detect();
	CACHE.store(features.0 | INIT_BIT, Ordering::Relaxed);
}

pub fn test(feature: Feature) -> bool {
	let cache = CACHE.load(Ordering::Relaxed);
	let feature = feature as usize;

	debug_assert!(
		feature < CAPACITY,
		"Too many features",
	);
	
	if cache & INIT_MASK == 0 { detect_and_initialize(); }
	cache & feature != 0
}
