use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use core::fmt;
use log::debug;
use uefi::{cstr16, Status, CStr16, CString16};
use uefi::boot::ScopedProtocol;
use uefi::fs::FileSystem;
use uefi::proto::media::file::{FileAttribute, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::media::file::File;

#[derive(Debug)]
pub enum ParseVersionError {
	InvalidChar(char),
	NotEnoughSegments(usize),
	WrongExtension,
}

impl fmt::Display for ParseVersionError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "invalid kernel version")
	}
}

impl core::error::Error for ParseVersionError {}

pub struct KernelVersion {
	major: u32,
	minor: u32,
	patch: u32,
}

impl fmt::Display for KernelVersion {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
	}
}

pub struct KernelFiles {
	pub kernel: Vec<u8>,
	pub symbol_map: Option<Vec<u8>>,
}

fn parse_kernel_version(from: &CStr16) -> Result<KernelVersion, ParseVersionError> {
	let mut version = KernelVersion {
		major: 0,
		minor: 0,
		patch: 0,
	};
	let mut targets: &mut [_] = &mut [&mut version.major, &mut version.minor, &mut version.patch];

	let mut iter = from.iter().skip(7).copied().map(char::from);
	// skip `kernel-` portion
	for c in iter {
		if let Some(digit) = c.to_digit(10) {
			*targets[0] *= 10;
			*targets[0] += digit;
		} else if c == '.' {
			if targets.len() == 1 { break; }
			else { targets = &mut targets[1..]; }
		} else {
			return Err(ParseVersionError::InvalidChar(c));
		}
	}

	if targets.len() > 1 { return Err(ParseVersionError::NotEnoughSegments(targets.len())); }

	Ok(version)
}

pub fn find_latest_kernel(fs: &mut SimpleFileSystem) -> Result<KernelVersion, Box<dyn core::error::Error>> {
	let mut root_dir = fs.open_volume()?;
	let system_dir = root_dir.open(
		cstr16!("System\\kernel"),
		FileMode::Read,
		FileAttribute::empty(),
	)?;
	let mut system_dir = system_dir.into_directory().ok_or(uefi::Error::new(Status::UNSUPPORTED, ()))?;

	// fixme: actually find the latest kernel version
	let version = 'kernel_bin: {
		while let Some(file) = system_dir.read_entry_boxed()? {
			if !file.is_regular_file() { continue; }
			debug!("found file: {}", file.file_name());

			let iter = file.file_name().iter();
			if iter.copied().map(char::from).zip("kernel-".chars()).all(|(a, b)| a == b) {
				debug!("found potential kernel at {}", file.file_name());
				let version = parse_kernel_version(file.file_name())?;
				debug!("parsed version: {version}");
				break 'kernel_bin version;
			}
		}
		panic!("no kernel file found");
	};

	Ok(version)
}

pub fn unpack_kernel(fs: ScopedProtocol<SimpleFileSystem>, version: KernelVersion) -> Result<KernelFiles, Box<dyn core::error::Error>> {
	let kernel_path = {
		let path = format!("System\\kernel\\kernel-{version}.exec");
		CString16::try_from(&*path)?
	};
	let symbol_path = {
		let path = format!("System\\kernel\\symbols-{version}.map");
		CString16::try_from(&*path)?
	};

	let mut fs = FileSystem::new(fs);
	let kernel = fs.read(&*kernel_path)?;
	let symbol_map = fs.try_exists(&*symbol_path)?.then(|| fs.read(&*symbol_path)).transpose()?;

	Ok(KernelFiles { kernel, symbol_map })
}
