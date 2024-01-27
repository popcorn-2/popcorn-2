use core::marker::PhantomData;
use core::mem::MaybeUninit;
use kernel_api::memory::allocator::{AllocError, BackingAllocator};
use kernel_api::memory::Frame;
use table::{Table, PDPT, PML4};
use crate::paging::Entry;

mod table;

pub struct KTable {
	tables: Frame,
	_phantom_owns: PhantomData<[Table<PDPT>; 256]>
}

impl KTable {
	fn tables(&self) -> &[Table<PDPT>; 256] {
		unsafe {
			&*self.tables.to_page().as_ptr().cast()
		}
	}
}

#[repr(transparent)]
pub struct TTable {
	pml4: Frame,
	_phantom_owns: PhantomData<Table<PML4>>
}

impl TTable {
	pub fn new(ktable: &KTable, allocator: impl BackingAllocator) -> Result<Self, AllocError> {
		let table_frame = allocator.allocate_one()?;
		let table_ptr = table_frame.to_page().as_ptr().cast::<MaybeUninit<Table<PML4>>>();
		assert!(!table_ptr.is_null() && table_ptr.is_aligned());
		let table_ptr = unsafe { &mut *table_ptr };

		let mut table = Table::new();
		for (i, entry) in table.entries[256..].iter_mut().enumerate() {
			let ktable_frame = ktable.tables + i;
			entry.point_to_frame(ktable_frame)
					.expect("Empty table should have no mappings");
		}
		table_ptr.write(table);

		Ok(Self {
			pml4: table_frame,
			_phantom_owns: PhantomData
		})
	}
}
