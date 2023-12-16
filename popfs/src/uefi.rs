#![feature(inline_const)]
#![no_std]
#![no_main]

extern crate alloc;

use alloc::boxed::Box;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::NonNull;

use uefi::{Guid, Identify};
use uefi::prelude::*;
use uefi::proto::media::partition::PartitionInfo;
use uefi::proto::unsafe_protocol;
use uefi::table::boot::{OpenProtocolAttributes, OpenProtocolParams};
use uefi_raw::protocol::device_path::DevicePathProtocol;
use uefi_services::println;

static mut SYSTEM_TABLE: Option<SystemTable<Boot>> = None;

fn system_table() -> &'static mut SystemTable<Boot> {
	unsafe { &mut SYSTEM_TABLE }.as_mut().unwrap()
}

static mut FOO: u8 = 5;

#[repr(C)]
#[unsafe_protocol("18A031AB-B443-4D1A-A5C0-0C09261E9F71")]
pub struct DriverBinding {
	supported: extern "efiapi" fn(this: &mut Self, controller_handle: Handle, remaining_device_path: Option<NonNull<DevicePathProtocol>>) -> Status,
	start: extern "efiapi" fn(this: &mut Self, controller_handle: Handle, remaining_device_path: Option<NonNull<DevicePathProtocol>>) -> Status,
	stop: extern "efiapi" fn(this: &mut Self, controller_handle: Handle, number_of_children: usize, child_handle_buffer: Option<NonNull<Handle>>) -> Status,
	version: u32,
	image_handle: Option<Handle>,
	driver_binding_handle: Option<Handle>,
}

impl DriverBinding {
	extern "efiapi" fn supported(&mut self, controller_handle: Handle, _: Option<NonNull<DevicePathProtocol>>) -> Status {
		let p = match unsafe { system_table().boot_services().open_protocol::<PartitionInfo>(
			OpenProtocolParams {
				handle: controller_handle,
				agent:  self.image_handle.unwrap(),
				controller: Some(self.image_handle.unwrap()),
			},
			OpenProtocolAttributes::ByDriver
		) } {
			Ok(p) => p,
			Err(_) => return Status::UNSUPPORTED
		};

		let p = match p.gpt_partition_entry() {
			Some(p) => p,
			None => return Status::UNSUPPORTED
		};

		let guid = p.partition_type_guid.0;
		if guid == const { Guid::parse_or_panic("8A6CC16C-D110-46F1-813F-0382046342C8") } {
			println!("found supported device");

			Status::SUCCESS
		} else {
			Status::UNSUPPORTED
		}
	}

	extern "efiapi" fn start(&mut self, controller_handle: Handle, _: Option<NonNull<DevicePathProtocol>>) -> Status {
		match unsafe {
			system_table().boot_services().install_protocol_interface(
				Some(controller_handle),
				&const { Guid::parse_or_panic("09c41eb7-f9d5-4c53-85be-b43af6492c09") },
				(&mut FOO as *mut u8).cast()
			)
		} {
			Ok(_) => Status::SUCCESS,
			Err(e) => e.status()
		}
	}

	extern "efiapi" fn stop(&mut self, controller_handle: Handle, _: usize, _: Option<NonNull<Handle>>) -> Status {
		match unsafe {
			system_table().boot_services().uninstall_protocol_interface(
				controller_handle,
				&const { Guid::parse_or_panic("09c41eb7-f9d5-4c53-85be-b43af6492c09") },
				(&mut FOO as *mut u8).cast()
			)
		} {
			Ok(_) => Status::SUCCESS,
			Err(e) => e.status()
		}
	}
}

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
	let Ok(_) = uefi_services::init(&mut system_table) else {
		let _ = system_table.stderr().output_string(cstr16!("Unable to initialise")); // Can't really do anything if this fails
		return Status::ABORTED;
	};

	println!("guiddddddd: {:?}", const { Guid::parse_or_panic("09c41eb7-f9d5-4c53-85be-b43af6492c09") });

	let driver_proto = Box::leak(Box::new(DriverBinding {
		supported: DriverBinding::supported,
		start: DriverBinding::start,
		stop: DriverBinding::stop,
		version: 0x10,
		image_handle: Some(image_handle),
		driver_binding_handle: Some(image_handle),
	}));

	unsafe { SYSTEM_TABLE = Some(system_table.unsafe_clone()); }

	unsafe {
		system_table.boot_services()
				.install_protocol_interface(Some(image_handle), &DriverBinding::GUID, (driver_proto as *mut DriverBinding).cast())
				.unwrap();
	}

	Status::SUCCESS
}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
	let _ = writeln!(system_table().stderr(), "{info}");
	loop {}
}
