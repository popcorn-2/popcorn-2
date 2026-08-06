pub trait RawHeaderPart {
    const OFFSET: usize;
    const LEN: usize;
    type Output: for<'a> TryFrom<&'a [u8]>;
}

pub fn index_part<T: RawHeaderPart>(data: &[u8]) -> T::Output {
    let data = &data[T::OFFSET .. (T::OFFSET + T::LEN)];
    T::Output::try_from(data).unwrap_or_else(|_| panic!("indexed `data` with len `T::LEN`"))
}

macro_rules! header_part {
	($name:ident => ($offset:literal, $len:literal); $($rest:tt)*) => {
		pub enum $name {}
		impl $crate::raw::RawHeaderPart for $name {
			const OFFSET: usize = $offset;
			const LEN: usize = $len;
			type Output = [u8; $len];
		}

		header_part!($($rest)*);
	};
	() => {};
}

pub mod file {
	header_part! {
		Magic => (0x00, 4);
		Class => (0x04, 1);
		Data => (0x05, 1);
		HeaderVersion => (0x06, 1);
		OsAbi => (0x07, 1);
		AbiVersion => (0x08, 1);
		Type => (0x10, 2);
		Machine => (0x12, 2);
		FileVersion => (0x14, 4);
	}

	pub mod x32 {
		header_part! {
			Entry => (0x18, 4);
			ProgramHeaderOffset => (0x1C, 4);
			SectionHeaderOffset => (0x20, 4);
			Flags => (0x24, 4);
			HeaderSize => (0x28, 2);
			ProgramHeaderEntrySize => (0x2A, 2);
			ProgramHeaderEntryNum => (0x2C, 2);
			SectionHeaderEntrySize => (0x2E, 2);
			SectionHeaderEntryNum => (0x30, 2);
			SectionHeaderStrTabIndex => (0x32, 2);
		}
	}

	pub mod x64 {
		header_part! {
			Entry => (0x18, 8);
			ProgramHeaderOffset => (0x20, 8);
			SectionHeaderOffset => (0x28, 8);
			Flags => (0x30, 4);
			HeaderSize => (0x34, 2);
			ProgramHeaderEntrySize => (0x36, 2);
			ProgramHeaderEntryNum => (0x38, 2);
			SectionHeaderEntrySize => (0x3A, 2);
			SectionHeaderEntryNum => (0x3C, 2);
			SectionHeaderStrTabIndex => (0x3E, 2);
		}
	}
}

pub mod program {
	pub mod x32 {
		header_part! {
			Type => (0x00, 4);
			Offset => (0x04, 4);
			VAddr => (0x08, 4);
			PAddr => (0x0C, 4);
			FileSize => (0x10, 4);
			MemSize => (0x14, 4);
			Flags => (0x18, 4);
			Align => (0x1C, 4);
		}

		pub const SIZE: usize = 0x20;
	}

	pub mod x64 {
		header_part! {
			Type => (0x00, 4);
			Flags => (0x04, 4);
			Offset => (0x08, 8);
			VAddr => (0x10, 8);
			PAddr => (0x18, 8);
			FileSize => (0x20, 8);
			MemSize => (0x28, 8);
			Align => (0x30, 8);
		}

		pub const SIZE: usize = 0x38;
	}
}

pub mod section {
	pub mod x32 {
		header_part! {
			Name => (0x00, 4);
			Type => (0x04, 4);
			Flags => (0x08, 4);
			VAddr => (0x0C, 4);
			Offset => (0x10, 4);
			Size => (0x14, 4);
			Link => (0x18, 4);
			Info => (0x1C, 4);
			Align => (0x20, 4);
			EntrySize => (0x24, 4);
		}

		pub const SIZE: usize = 0x28;
	}

	pub mod x64 {
		header_part! {
			Name => (0x00, 4);
			Type => (0x04, 4);
			Flags => (0x08, 8);
			VAddr => (0x10, 8);
			Offset => (0x18, 8);
			Size => (0x20, 8);
			Link => (0x28, 4);
			Info => (0x2C, 4);
			Align => (0x30, 8);
			EntrySize => (0x38, 8);
		}

		pub const SIZE: usize = 0x3C;
	}
}

pub mod dynamic {
	pub mod x32 {
		header_part! {
			Tag => (0x00, 4);
			Un => (0x04, 4);
		}

		pub const SIZE: usize = 0x08;
	}

	pub mod x64 {
		header_part! {
			Tag => (0x00, 8);
			Un => (0x08, 8);
		}

		pub const SIZE: usize = 0x10;
	}
}
