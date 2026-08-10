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
use uefi::proto::media::file::File;

const ROOTFS_GUID: Guid = cfg_select! {
	target_arch = "x86_64" => guid!("8A6CC16C-D110-46F1-813F-0382046342C8"),
	target_arch = "aarch64" => guid!("B1D8F0F9-05CB-42E1-A591-A6980E7B5909"),
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
			Self::UefiError(status) => write!(f, "UEFI error: {}", status),
			Self::LocateDiskError(status) => write!(f, "locate disk error: {}", status),
		}
	}
}

impl Error for RootfsError {
	fn source(&self) -> Option<&(dyn Error + 'static)> {
		match self {
			Self::UefiError(status) => Some(status),
			Self::LocateDiskError(status) => Some(status),
		}
	}
}

pub fn locate_rootfs() -> Result<ScopedProtocol<SimpleFileSystem>, Box<dyn Error>> {
	let disk = locate_kernel_disk()?;
	let filesystems = boot::locate_handle_buffer(SearchType::from_proto::<SimpleFileSystem>())?;

	// filter filesystems to the same disk as we're booted from
	let filesystems = filesystems.into_iter()
		.inspect(|handle| {
			if let Ok(path) = boot::open_protocol_exclusive::<DevicePath>(**handle) {
				debug!("found FS at {}", &*path);
			}
		})
		.filter_map(|handle| {
			let Ok(device_path) = boot::open_protocol_exclusive::<DevicePath>(*handle) else { return None; };
			is_same_disk(&*disk, &*device_path).then(|| (device_path, handle))
		});

	// find a partition with the correct GUID for the rootfs
	let mut filesystems = filesystems.filter(|(device_path, _)| {
		let Ok(partition) = boot::locate_device_path::<PartitionInfo>(&mut &**device_path) else { return false; };
		let Ok(partition) = boot::open_protocol_exclusive::<PartitionInfo>(partition) else { return false; };

		if let Some(entry) = partition.gpt_partition_entry() {
			let ty = entry.partition_type_guid;
			ty == GptPartitionType(ROOTFS_GUID)
		} else { false }
	});

	let (path, handle) = filesystems.next().ok_or(uefi::Error::new(Status::NOT_FOUND, ()))?;
	info!("booting from {}", &*path);
	for (path, _) in filesystems {
		debug!("also found {}", &*path);
	}

	Ok(boot::open_protocol_exclusive::<SimpleFileSystem>(*handle)?)
}

fn locate_kernel_disk() -> Result<ScopedProtocol<DevicePath>, RootfsError> {
	let image = boot::image_handle();
	let image = boot::open_protocol_exclusive::<LoadedImage>(image).map_err(RootfsError::LocateDiskError)?;
	let device = image.device()
		.ok_or(uefi::Error::new(Status::UNSUPPORTED, ())).map_err(RootfsError::LocateDiskError)?;
	boot::open_protocol_exclusive::<DevicePath>(device).map_err(RootfsError::LocateDiskError)
}

fn is_same_disk(lhs: &DevicePath, rhs: &DevicePath) -> bool {
	let mut paths = zip(lhs.node_iter(), rhs.node_iter());

	while let Some((lhs, rhs)) = paths.next() {
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

	let iter = from.iter().skip(7).copied().map(char::from);
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
