use alloc::boxed::Box;
use core::error::Error;
use log::debug;
use uefi::boot;
use uefi::boot::{OpenProtocolAttributes, OpenProtocolParams};
use utils::handoff;
use crate::mapper::Mapper;
use uefi::proto::console::gop::{GraphicsOutput, PixelBitmask, PixelFormat};
use kernel_api::mapping::Ty;
use kernel_api::memory::RawFrame;
use utils::handoff::ColorMask;

/// # Errors
///
/// Returns any errors from the firmware, or if memory mapping failed.
///
/// # Panics
///
/// If an unsupported framebuffer type is used by the system.
pub fn map_framebuffer(mapper: &mut Mapper) -> Result<handoff::Framebuffer, Box<dyn Error>> {
	debug!("mapping framebuffer");

	let handle = boot::get_handle_for_protocol::<GraphicsOutput>()?;
	// SAFETY: framebuffer shouldn't disappear while system is running?
	let mut gop = unsafe { boot::open_protocol::<GraphicsOutput>(
		OpenProtocolParams {
			handle,
			agent: boot::image_handle(),
			controller: None,
		},
		OpenProtocolAttributes::GetProtocol,
	)? };
	let mode = gop.current_mode_info();
	let mut fb = gop.frame_buffer();
	let physical_address = RawFrame::new(fb.as_mut_ptr().addr());

	let color_format = match mode.pixel_format() {
		PixelFormat::Rgb => ColorMask::RGBX,
		PixelFormat::Bgr => ColorMask::BGRX,
		PixelFormat::BltOnly => panic!("framebuffer not supported"),
		PixelFormat::Bitmask => {
			let PixelBitmask { red, green, blue, .. } = mode.pixel_bitmask().expect("framebuffer is of bitmask format");
			ColorMask { red, green, blue }
		}
	};

	let (_, virtual_addr) = mapper.new_mapping(
		Some(physical_address),
		None,
		fb.size().div_ceil(kernel_api::memory::PAGE_SIZE),
		Ty::FB,
	)?;

	debug!("fb done");

	Ok(handoff::Framebuffer {
		buffer: virtual_addr.as_ptr(),
		stride: mode.stride(),
		width: mode.resolution().0,
		height: mode.resolution().1,
		color_format,
		physical_address: *physical_address,
	})
}
