use alloc::boxed::Box;
use alloc::format;
use alloc::vec::Vec;
use core::error::Error;
use core::fmt;
use core::iter::zip;
use log::{debug, info};
use uefi::{boot, Status, Guid, guid, CStr16, cstr16, CString16};
use uefi::boot::{ScopedProtocol, SearchType};
use uefi::fs::FileSystem;
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::device_path::{DevicePath, DeviceSubType, DeviceType};
use uefi::proto::media::file::{FileAttribute, FileMode};
use uefi::proto::media::partition::{GptPartitionType, PartitionInfo};
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::media::file::File as _;

const ROOTFS_GUID: Guid = cfg_select! {
	target_arch = "x86_64" => guid!("8A6CC16C-D110-46F1-813F-0382046342C8"),
};

#[derive(Debug)]
pub enum RootfsError {
	UefiError(uefi::Error),
	LocateDiskError(uefi::Error),
}

impl From<Status> for RootfsError {
	fn from(status: Status) -> Self {
		Self::UefiError(uefi::Error::new(status, ()))
	}
}

impl fmt::Display for RootfsError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::UefiError(status) => write!(f, "UEFI error: {status}"),
			Self::LocateDiskError(status) => write!(f, "locate disk error: {status}"),
		}
	}
}

impl Error for RootfsError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::UefiError(status) | Self::LocateDiskError(status) => Some(status),
		}
	}
}

/// # Errors
///
/// Returns any errors from the firmware.
pub fn locate_rootfs() -> Result<ScopedProtocol<SimpleFileSystem>, Box<dyn Error>> {
	let disk = locate_kernel_disk()?;
	let filesystems = boot::locate_handle_buffer(SearchType::from_proto::<SimpleFileSystem>())?;

	// filter filesystems to the same disk as we're booted from
	let filesystems = filesystems.iter()
		.inspect(|handle| {
			if let Ok(path) = boot::open_protocol_exclusive::<DevicePath>(**handle) {
				debug!("found FS at {}", &*path);
			}
		})
		.filter_map(|handle| {
			let Ok(device_path) = boot::open_protocol_exclusive::<DevicePath>(*handle) else { return None; };
			is_same_disk(&disk, &device_path).then_some((device_path, handle))
		});

	// find a partition with the correct GUID for the rootfs
	let mut filesystems = filesystems.filter(|(device_path, _)| {
		let Ok(partition) = boot::locate_device_path::<PartitionInfo>(&mut &**device_path) else { return false; };
		let Ok(partition) = boot::open_protocol_exclusive::<PartitionInfo>(partition) else { return false; };

		partition.gpt_partition_entry().is_some_and(|entry| {
			let ty = entry.partition_type_guid;
			ty == GptPartitionType(ROOTFS_GUID)
		})
	});

	let (path, handle) = filesystems.next().ok_or(uefi::Error::new(Status::NOT_FOUND, ()))?;
	info!("booting from {}", &*path);

	Ok(boot::open_protocol_exclusive::<SimpleFileSystem>(*handle)?)
}

/// # Errors
///
/// Returns an error if the boot drive could not be detected.
fn locate_kernel_disk() -> Result<ScopedProtocol<DevicePath>, RootfsError> {
	let image = boot::image_handle();
	let image = boot::open_protocol_exclusive::<LoadedImage>(image).map_err(RootfsError::LocateDiskError)?;
	let device = image.device()
		.ok_or(uefi::Error::new(Status::UNSUPPORTED, ())).map_err(RootfsError::LocateDiskError)?;
	boot::open_protocol_exclusive::<DevicePath>(device).map_err(RootfsError::LocateDiskError)
}

fn is_same_disk(lhs: &DevicePath, rhs: &DevicePath) -> bool {
	let paths = zip(lhs.node_iter(), rhs.node_iter());

	#[expect(clippy::shadow_unrelated, reason = "false positive")]
	for (lhs, rhs) in paths {
		if lhs.device_type() == DeviceType::MEDIA && lhs.sub_type() == DeviceSubType::MEDIA_HARD_DRIVE {
			// reached a partition but have matched so far so must be on same disk
			return true;
		}

		if lhs.data() != rhs.data() {
			// paths have diverged
			return false;
		}
	}

	false
}

#[derive(Debug)]
pub enum ParseVersionError {
	InvalidChar(char),
	NotEnoughSegments(usize),
}

impl fmt::Display for ParseVersionError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::InvalidChar(char) => write!(f, "invalid kernel version: found char `{char}`"),
			Self::NotEnoughSegments(left) => write!(f, "invalid kernel version: missing {left} segmentss"),
		}
	}
}

impl Error for ParseVersionError {}

#[derive(Debug, Copy, Clone)]
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
	#[expect(unused, reason = "not supported yet")]
	pub symbol_map: Option<Vec<u8>>,
}

/// # Errors
///
/// Returns an error if the string contained an invalid kernel filename.
fn parse_kernel_version(from: &CStr16) -> Result<KernelVersion, ParseVersionError> {
	let mut version = KernelVersion {
		major: 0,
		minor: 0,
		patch: 0,
	};
	let mut targets: &mut [_] = &mut [&mut version.major, &mut version.minor, &mut version.patch];

	// skip `kernel-` portion
	let iter = from.iter().skip(7).copied().map(char::from);
	for char in iter {
		if let Some(digit) = char.to_digit(10) {
			*targets[0] *= 10;
			*targets[0] += digit;
		} else if char == '.' {
			if targets.len() == 1 { break; }
			targets = &mut targets[1..];
		} else {
			return Err(ParseVersionError::InvalidChar(char));
		}
	}

	if targets.len() > 1 { return Err(ParseVersionError::NotEnoughSegments(targets.len())); }

	Ok(version)
}

/// # Errors
///
/// Returns firmware errors if file access failed, or the filesystem layout is invalid.
pub fn find_latest_kernel(fs: &mut SimpleFileSystem) -> Result<KernelVersion, Box<dyn Error>> {
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
			if iter.copied().map(char::from).zip("kernel-".chars()).all(|(lhs, rhs)| lhs == rhs) {
				debug!("found potential kernel at {}", file.file_name());
				let version = parse_kernel_version(file.file_name())?;
				debug!("parsed version: {version}");
				break 'kernel_bin version;
			}
		}

		return Err(uefi::Error::new(Status::NOT_FOUND, ()).into());
	};

	Ok(version)
}

/// # Errors
///
/// Returns firmware errors if file access failed.
pub fn unpack_kernel(fs: ScopedProtocol<SimpleFileSystem>, version: KernelVersion) -> Result<(KernelFiles, Vec<u8>), Box<dyn Error>> {
	let kernel_path = {
		let path = format!("System\\kernel\\kernel-{version}.exec");
		CString16::try_from(&*path)?
	};
	let symbol_path = {
		let path = format!("System\\kernel\\symbols-{version}.map");
		CString16::try_from(&*path)?
	};
	let init_path = CString16::try_from("System\\bin\\init.elf")?;

	let mut fs = FileSystem::new(fs);
	info!("loading kernel from {kernel_path}");
	let kernel = fs.read(&*kernel_path)?;
	info!("loading `init` from {init_path}");
	let init = fs.read(&*init_path)?;
	let symbol_map = fs.try_exists(&*symbol_path)?.then(|| fs.read(&*symbol_path)).transpose()?;

	Ok((KernelFiles { kernel, symbol_map }, init))
}
