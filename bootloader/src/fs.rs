use alloc::boxed::Box;
use core::error::Error;
use core::fmt;
use core::iter::zip;
use log::{debug, info};
use uefi::{boot, Status, StatusExt, Guid, guid, println};
use uefi::boot::{ScopedProtocol, SearchType};
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::device_path::{DevicePath, DeviceSubType, DeviceType};
use uefi::proto::media::disk::DiskIo;
use uefi::proto::media::partition::{GptPartitionType, PartitionInfo};
use uefi::proto::media::fs::SimpleFileSystem;

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
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
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
	let mut filesystems = filesystems.filter(|(device_path, handle)| {
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
