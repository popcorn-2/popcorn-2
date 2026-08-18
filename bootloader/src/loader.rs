use alloc::boxed::Box;
use core::fmt;
use log::debug;
use uefi::{cstr16, Status, CStr16};
use uefi::proto::media::file::{FileAttribute, FileMode};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::media::file::File;

#[derive(Debug)]
pub enum ParseVersionError {
	InvalidChar(char),
	NotEnoughSegments(usize),
}

impl fmt::Display for ParseVersionError {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
	}
}

fn parse_kernel_version(from: &CStr16) -> Result<KernelVersion, ParseVersionError> {
	let mut version = KernelVersion {
		major: 0,
		minor: 0,
		patch: 0,
	};
	let mut targets: &mut [_] = &mut [&mut version.major, &mut version.minor, &mut version.patch];

	// skip `kernel-` portion
	for c in from.iter().skip(7).copied() {
		let c = char::from(c);
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

	let version = 'kernel_bin: {
		while let Some(file) = system_dir.read_entry_boxed()? {
			if !file.is_regular_file() { continue; }
			debug!("found file: {}", file.file_name());

			let mut iter = file.file_name().iter();
			if iter.copied().map(char::from).zip("kernel-".chars()).all(|(a, b)| a == b) {
				debug!("found potential kernel at {}", file.file_name());
				let version = parse_kernel_version(file.file_name())?;
				debug!("parsed version: {version}");
			}
		}
		panic!("no kernel file found");
	};
	Ok(version)
}

pub fn unpack_kernel(fs: &mut SimpleFileSystem) -> Result<(), Box<dyn core::error::Error>> {
	let version = find_latest_kernel(fs)?;
	Ok(())
}
