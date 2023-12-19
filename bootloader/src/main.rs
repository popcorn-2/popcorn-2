#![feature(ptr_metadata)]
#![feature(try_blocks)]
#![feature(let_chains)]
#![feature(pointer_is_aligned)]
#![feature(new_uninit)]
#![feature(split_array)]
#![feature(slice_ptr_len)]
#![feature(slice_ptr_get)]
#![feature(inline_const)]
#![feature(type_name_of_val)]
#![feature(arbitrary_self_types)]
#![feature(concat_bytes)]
#![feature(allocator_api)]
#![feature(iter_collect_into)]
#![feature(noop_waker)]
#![feature(c_str_literals)]
#![feature(kernel_memory_addr_access)]
#![no_main]
#![no_std]

extern crate alloc;

use alloc::{format, vec};
use alloc::borrow::ToOwned;
use alloc::boxed::Box;
use alloc::ffi::CString;
use alloc::vec::Vec;
use core::{fmt, mem};
use core::arch::asm;
use core::fmt::Write;
use core::panic::PanicInfo;
use core::ptr::NonNull;
use core::time::Duration;

use bitflags::Flags;
use derive_more::Display;
use log::{debug, error, info, trace, warn};
use more_asserts::assert_lt;
use uefi::{Char16, Event, Guid};
use uefi::data_types::{Align, Identify};
use uefi::fs::{FileSystem, Path};
use uefi::prelude::*;
use uefi::proto::console::gop::{GraphicsOutput, PixelFormat};
use uefi::proto::console::pointer::Pointer;
use uefi::proto::console::serial::Serial;
use uefi::proto::console::text::{Input, Key, ScanCode};
use uefi::proto::loaded_image::LoadedImage;
use uefi::proto::media::fs::SimpleFileSystem;
use uefi::proto::media::partition::PartitionInfo;
use uefi::table::boot::{AllocateType, EventType, MemoryDescriptor, MemoryType, OpenProtocolAttributes, OpenProtocolParams, PAGE_SIZE, SearchType, TimerTrigger, Tpl};
use uefi::table::runtime::ResetType;

use kernel_api::memory::{PhysicalAddress, VirtualAddress};
use lvgl2::font::Font;
use lvgl2::input::{encoder, pointer};
use lvgl2::input::encoder::ButtonUpdate;
use lvgl2::input::pointer::Update;
use lvgl2::misc::Color;
use lvgl2::object::{Object, style, Widget};
use lvgl2::object::button::Button;
use lvgl2::object::group::Group;
use lvgl2::object::image::{Image, ImageSource};
use lvgl2::object::label::Label;
use lvgl2::object::layout::{flex, Layout};
use lvgl2::object::style::{ExternalStyle, Opacity, Part, State, Style};
use utils::handoff;
use utils::handoff::{ColorMask, MemoryMapEntry, Range};

use crate::config::Config;
use crate::framebuffer::Gui;
use crate::paging::{Frame, Page, TableEntryFlags};

mod framebuffer;
mod config;
mod paging;
mod logging;
mod elf;

struct DualWriter<T: Write, U: Write>(T, U);

impl<T: Write, U: Write> Write for DualWriter<T, U> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let a = self.0.write_str(s);
        let b = self.1.write_str(s);
        a?;
        b?;
        Ok(())
    }
}

#[entry]
fn main(image_handle: Handle, mut system_table: SystemTable<Boot>) -> Status {
    let Ok(_) = uefi_services::init(&mut system_table) else {
        return Status::ABORTED;
    };

    let services = system_table.boot_services();

    let uart = match services.get_handle_for_protocol::<Serial>() {
        Ok(uart) => uart,
        Err(e) => {
            return e.status();
        }
    };

    let mut uart = match unsafe {
        system_table.boot_services().open_protocol::<Serial>(OpenProtocolParams {
            handle: uart,
            agent: image_handle,
            controller: Some(image_handle),
        }, OpenProtocolAttributes::GetProtocol)
    } {
        Ok(uart) => uart,
        Err(e) => return e.status()
    };

    let gop = match services.get_handle_for_protocol::<GraphicsOutput>() {
        Ok(gop) => gop,
        Err(e) => {
            let _ = writeln!(uart, "Unable to enable graphics"); // Can't really do anything if this fails
            return e.status();
        }
    };

    let mut gop = match unsafe {
        services.open_protocol::<GraphicsOutput>(OpenProtocolParams {
            handle: gop,
            agent: services.image_handle(),
            controller: None,
        }, OpenProtocolAttributes::GetProtocol)
    } {
        Ok(gop) => gop,
        Err(e) => {
            let _ = writeln!(uart, "Unable to enable graphics");
            return e.status();
        }
    };

    // SAFETY: We don't touch the logger after calling exit_boot_services()
    // (unless someone breaks the code)
    unsafe { logging::init(&mut *uart).unwrap(); }

    if let Ok(image) = services.open_protocol_exclusive::<LoadedImage>(image_handle) {
        info!("base address: {:p}", image.info().0);
    }

    let mouse = match services.get_handle_for_protocol::<Pointer>() {
        Ok(mouse) => mouse,
        Err(e) => {
            error!("Unable to find mouse");
            return e.status();
        }
    };

    let mut mouse = match services.open_protocol_exclusive::<Pointer>(mouse) {
        Ok(mouse) => mouse,
        Err(e) => {
            error!("Unable to find mouse");
            return e.status();
        }
    };

    let keyboard = match services.get_handle_for_protocol::<Input>() {
        Ok(keyboard) => keyboard,
        Err(e) => {
            error!("Unable to find keyboard");
            return e.status();
        }
    };

    let mut keyboard = match services.open_protocol_exclusive::<Input>(keyboard) {
        Ok(keyboard) => keyboard,
        Err(e) => {
            error!("Unable to find keyboard");
            return e.status();
        }
    };

    let size_mm = if let Ok(edid_handle) = services.get_handle_for_protocol::<framebuffer::ActiveEdid>()
            && let Ok(edid) = services.open_protocol_exclusive::<framebuffer::ActiveEdid>(edid_handle)
            && edid.len() > 71 {

        const DTD_OFFSET: usize = 54;
        let width_mm_lsb = edid[DTD_OFFSET + 12];
        let height_mm_lsb = edid[DTD_OFFSET + 13];
        let mm_msb = edid[DTD_OFFSET + 14];

        let width_mm = (width_mm_lsb as u16) | (((mm_msb as u16) & 0xF0) << 4);
        let height_mm = (height_mm_lsb as u16) | (((mm_msb as u16) & 0x0F) << 8);

        let size_mm = Some((width_mm, height_mm));

        info!("display size is {size_mm:?}");

        size_mm
    } else {
        warn!("Could not get EDID info");
        None
    };

    info!("Mouse: {:?}", mouse.mode());

	let mut verbose_mode = false;
	while let Ok(Some(key)) = keyboard.read_key() {
		debug!("{key:?}");

		const CHAR16_VL: Char16 = unsafe { Char16::from_u16_unchecked(b'v' as u16) };
		const CHAR16_VU: Char16 = unsafe { Char16::from_u16_unchecked(b'V' as u16) };

		match key {
			Key::Printable(CHAR16_VL | CHAR16_VU) => verbose_mode = true,
			_ => {}
		}

		if verbose_mode { break; } // Break once all options handled so it doesn't hang on holding keys
	}

	debug!("Verbose mode {verbose_mode}");

    let mut fs = match services.get_image_file_system(image_handle) {
        Ok(fs) => fs,
        Err(e) => return e.status()
    };

    let Ok(popfs_driver) = fs.read(Path::new(cstr16!(r"EFI\POPCORN\popfs.efi"))) else {
        panic!("Unable to find popfs driver")
    };
    /* TODO: Check if already loaded and if not, add to Driver#### efivars, adjust BootNext to point to uwave, then reboot
    let popfs_driver = services.load_image(image_handle, LoadImageSource::FromBuffer {

        buffer: &popfs_driver,
        file_path: None,
    }).unwrap();
    services.start_image(popfs_driver).unwrap();
     */

    let Ok(config) = fs.read_to_string(Path::new(cstr16!(r"EFI\POPCORN\config.toml"))) else {
        panic!("Unable to find bootloader config file")
    };
    let config: Config = toml::from_str(&config).unwrap();

   /* let regular = &default_font.regular;
    let bold = &default_font.bold;
    let italic = &default_font.italic;
    let bold_italic = &default_font.bold_italic;

    let regular = fs.read(regular).unwrap_or_else(|_| panic!("Unable to find regular font file"));
    let bold = bold.as_ref().map(|path| fs.read(path).unwrap_or_else(|_| panic!("Unable to find bold font file")));
    let italic = italic.as_ref().map(|path| fs.read(path).unwrap_or_else(|_| panic!("Unable to find italic font file")));
    let bold_italic = bold_italic.as_ref().map(|path| fs.read(path).unwrap_or_else(|_| panic!("Unable to bold-italic regular font file")));

    let regular = psf::try_parse(&regular).unwrap_or_else(|e| panic!("Invalid file for regular font: {}", e));
    let bold = bold.as_ref().map(|data| psf::try_parse(data).unwrap_or_else(|e| panic!("Invalid file for bold font: {}", e)));
    let italic = italic.as_ref().map(|data| psf::try_parse(data).unwrap_or_else(|e| panic!("Invalid file for italic font: {}", e)));
    let bold_italic = bold_italic.as_ref().map(|data| psf::try_parse(data).unwrap_or_else(|e| panic!("Invalid file for bold-italic font: {}", e)));
*/
    mouse.reset(false).unwrap();

    //let default_font = FontFamily::new(regular, bold, italic, bold_italic);

    let mode = {
        let optimal_resolutions = gop.modes()
                                     .filter(|mode|
                                             mode.info().resolution().1 == 1080 ||
                                                     mode.info().resolution().1 == 720 ||
                                                     mode.info().resolution().1 == 480
                                     );

        let mut optimal_resolution = optimal_resolutions.max_by_key(|mode| mode.info().resolution().0);

        if optimal_resolution.is_none() {
            let fallback_optimal_resolutions = gop.modes()
                                                  .filter(|mode|
                                                          mode.info().resolution().0 == 1920 ||
                                                                  mode.info().resolution().0 == 1280 ||
                                                                  mode.info().resolution().0 == 640
                                                  );
            optimal_resolution = fallback_optimal_resolutions.max_by_key(|mode| mode.info().resolution().0);
        }

        optimal_resolution
    };

    if let Some(ref mode) = mode && gop.set_mode(mode).is_err() {
        warn!("Unable to set display resolution");
    }

    let (width, height) = gop.current_mode_info().resolution();
    let aspect = (width as f32) / (height as f32);
    let dpmm = size_mm.map(|(width_mm, height_mm)| {
        let dpmm_width = width / usize::from(width_mm);
        let dpmm_height = height / usize::from(height_mm);
        (dpmm_width + dpmm_height) / 2
    }).unwrap_or(50 /* 130 dpi */);

    /*lvgl2::init();
    let buffer = DrawBuffer::new(8000);
    let mut driver = Driver::new(buffer, width, height, |update| {
        let update_width = update.area.x2 - update.area.x1 + 1;
        let update_height = update.area.y2 - update.area.y1 + 1;

        trace!("display update ({}, {}) ({}, {})", update.area.x1, update.area.y1, update_width, update_height);

        for c in update.colors.iter_mut() {
            c.set_a(0);
        }

        let buffer: &[BltPixel] = unsafe {
            mem::transmute(update.colors)
        };

        let blt_op = BltOp::BufferToVideo {
            buffer,
            src: BltRegion::Full,
            dest: (
                update.area.x1.try_into().unwrap(),
                update.area.y1.try_into().unwrap()
            ),
            dims: (
                update_width.try_into().unwrap(),
                update_height.try_into().unwrap()
            ),
        };

        gop.blt(blt_op).expect("Failed to flush display");

        trace!("paint finished");
    });*/

    trace!("initalising lvgl");
    let mut ui = Gui::new(&mut gop);

    let mut screen = ui.display.active_screen();
    let mut style = screen.inline_style(Part::Main, State::DEFAULT);
    style.set_bg_color(Color::from_rgb(0x33, 0x33, 0x33));
    style.set_text_color(Color::from_rgb(0xee, 0xee, 0xee));
    style.set_bg_opa(Opacity::OPA_100);
    style.set_border_width(0);
    style.set_radius(0);
    let open_sans_48 = unsafe {
        Font::new(&lvgl_sys::open_sans_48)
    };
    style.set_text_font(open_sans_48);

    let mut flex_box = Object::new(Some(screen));
    let mut style = flex_box.inline_style(Part::Main, State::DEFAULT);
    style.set_layout(Layout::flex());
    style.set_flex_flow(flex::Flow::COLUMN);
    style.set_flex_main_place(flex::Align::START);
    style.set_flex_cross_place(flex::Align::CENTER);
    style.set_flex_track_place(flex::Align::CENTER);
    style.set_pad_row(12);

    let menu_width = if aspect < 1.8 { lvgl2::misc::pct(90) }
    else {
        /* ultra-wide screen - 90% of 16×9 area */
        i16::try_from(height).unwrap() * 16 / 10
    };

    style.set_size(menu_width, lvgl2::misc::pct(95));
    style.set_align(lvgl2::object::style::Align::Center);

    let mut label = Label::new(Some(flex_box.as_mut()));
    label.set_text(c"popcorn");
    let quicksand_200 = unsafe {
        Font::new(&lvgl_sys::quicksand_200)
    };
    label.inline_style(Part::Main, State::DEFAULT).set_text_font(quicksand_200);

    let mut item_style = {
        let mut item_style = ExternalStyle::new();
        item_style.set_bg_color(Color::from_rgb(0xef, 0xbb, 0x40));
        item_style.set_text_color(Color::from_rgb(0, 0, 0));
        item_style.set_outline_color(Color::from_rgb(0x1c, 0x8a, 0xeb));
        item_style.set_bg_opa(Opacity::OPA_100);
        item_style.set_pad_bottom(8);
        item_style.set_pad_top(8);
        item_style.set_pad_left(8);
        item_style.set_pad_right(8);
        item_style
    };

    let mut item_style_hover = {
        let mut item_style = ExternalStyle::new();
        item_style.set_bg_color(Color::from_rgb(0x1c, 0x8a, 0xeb));
        item_style
    };

    let mut item_style_focus = {
        let mut item_style = ExternalStyle::new();
        item_style.set_outline_pad(2);
        item_style.set_outline_width(2);
        item_style
    };

    let mut button_group = Group::new();
    let mut boot_start_events = Vec::new();

    for i in 0..5 {
        let event = unsafe { services.create_event(EventType::empty(), Tpl::APPLICATION, None, None) }.unwrap();
        boot_start_events.push(unsafe { event.unsafe_clone() });

        let mut btn = Button::new_with_callback(Some(flex_box.as_mut()), move || {
            services.signal_event(&event).unwrap();
        });
        btn.inline_style(Part::Main, State::DEFAULT)
                .set_width(lvgl2::misc::pct(98));
        btn.add_style(Part::Main, State::DEFAULT, &mut item_style);
        btn.add_style(Part::Main, State::PRESSED, &mut item_style_hover);
        btn.add_style(Part::Main, State::FOCUSED, &mut item_style_focus);

        let mut label = Label::new(Some(btn.upcast_mut()));

        let text = match i {
            0 => "Boot popcorn2".to_owned(),
            _ => format!("Boot option {i}")
        };
        let text = CString::new(text).unwrap().into_boxed_c_str();
        label.set_text(Box::leak(text));

        button_group.add_object(btn.upcast_mut());

        mem::forget(btn);
        mem::forget(label);
    }

    let mut power_options = {
        let mut power_options = Object::new(Some(flex_box.as_mut()));
        let mut style = power_options.inline_style(Part::Main, State::DEFAULT);
        style.set_flex_flow(flex::Flow::ROW);
        style.set_flex_main_place(flex::Align::START);
        style.set_flex_cross_place(flex::Align::CENTER);
        style.set_flex_track_place(flex::Align::CENTER);
        style.set_pad_column(12);
        style.set_width(lvgl2::misc::pct(100));
        style.set_align(lvgl2::object::style::Align::Center);
        power_options
    };

    let power_style = {
        let mut power_style = ExternalStyle::new();
        power_style
    };

    let sf_pro = unsafe { Font::new(&lvgl_sys::sf_symbols_48) };

    let mut button_reboot = Button::new_with_callback(Some(screen), || {
        info!("rebooting");
        system_table.runtime_services().reset(ResetType::WARM, Status::SUCCESS, None);
    });
    button_reboot.add_style(Part::Main, State::DEFAULT, &mut item_style);
    button_reboot.add_style(Part::Main, State::PRESSED, &mut item_style_hover);
    button_reboot.add_style(Part::Main, State::FOCUSED, &mut item_style_focus);
    let mut s = button_reboot.inline_style(Part::Main, State::DEFAULT);
    s.set_radius(lvgl2::misc::pct(100));
    unsafe { lvgl_sys::lv_obj_align(button_reboot.upcast_mut().raw, style::Align::TopRight.into(), -(48*3), 48); }
    let mut label = Label::new(Some(button_reboot.upcast_mut()));
    label.set_text(c"􀅈");
    label.inline_style(Part::Main, State::DEFAULT).set_text_font(sf_pro);
    button_group.add_object(button_reboot.upcast_mut());

    let mut button_off = Button::new_with_callback(Some(screen), || {
        info!("shutting down");
        system_table.runtime_services().reset(ResetType::SHUTDOWN, Status::SUCCESS, None);
    });
    button_off.add_style(Part::Main, State::DEFAULT, &mut item_style);
    button_off.add_style(Part::Main, State::PRESSED, &mut item_style_hover);
    button_off.add_style(Part::Main, State::FOCUSED, &mut item_style_focus);
    let mut s = button_off.inline_style(Part::Main, State::DEFAULT);
    s.set_radius(lvgl2::misc::pct(100));
    unsafe { lvgl_sys::lv_obj_align(button_off.upcast_mut().raw, style::Align::TopRight.into(), -48, 48); }
    let mut label = Label::new(Some(button_off.upcast_mut()));
    label.set_text(c"􀆨");
    label.inline_style(Part::Main, State::DEFAULT).set_text_font(sf_pro);
    button_group.add_object(button_off.upcast_mut());

    if let Some(size_mm) = size_mm {
        let mut label = Label::new(Some(flex_box.as_mut()));
        let text = CString::new(format!("Physical size {size_mm:?}")).unwrap().into_boxed_c_str();
        label.set_text(Box::leak(text));

        mem::forget(label);
    }

    let mut log = Label::new(Some(flex_box.as_mut()));
    let mut style = log.inline_style(Part::Main, State::DEFAULT);
    style.set_bg_color(Color::from_rgb(0, 0, 0));
    style.set_text_color(Color::from_rgb(255, 255, 255));
    style.set_border_width(2);
    style.set_width(lvgl2::misc::pct(80));
    style.set_height(256);
    style.set_bg_opa(Opacity::OPA_COVER);
    log.set_recolor(true);
    log.set_text(c"Hello world!\n#ff0000 ERROR#: This is a test");

    let mut cursor = Image::new(Some(screen));
    {
        let cursor_src = ImageSource::new(unsafe { &lvgl_sys::cursor });
        cursor.set_source(cursor_src);
    }

    let mut touch_point = (
        width as f32 / 2f32,
        height as f32 / 2f32
    );
    let mut left_button = false;
    let pointer_mode = *mouse.mode();

    let pointer_update = || {
        match mouse.read_state() {
            Ok(Some(event)) => {
                info!("mouse event: {event:?}");
                if pointer_mode.resolution[0] != 0 {
                    touch_point.0 += (event.relative_movement[0] as f32) / (pointer_mode.resolution[0] as f32);
                }
                if pointer_mode.resolution[1] != 0 {
                    touch_point.1 += (event.relative_movement[1] as f32) / (pointer_mode.resolution[1] as f32);
                }
                left_button = pointer_mode.has_button[0] && event.button[0];
            },
            Err(e) => error!("mouse error: {e:?}"),
            _ => {}
        }

        Update {
            pressed: left_button,
            location: (touch_point.0 as i16, touch_point.1 as i16)
        }
    };

    let mut pointer_driver = pointer::Driver::new(pointer_update);
    let mut pointer = lvgl2::input::Input::new(&mut pointer_driver);
    pointer.set_cursor(cursor.upcast_mut());

    let keyboard_update = || {
        const CHAR16_SPACE: Char16 = unsafe { Char16::from_u16_unchecked(b' ' as u16) };
        const CHAR16_ENTER: Char16 = unsafe { Char16::from_u16_unchecked(b'\r' as u16) };

        match keyboard.read_key() {
            Ok(Some(event)) => {
                info!("keyboard event: {event:?}");

                match event {
                    Key::Special(ScanCode::LEFT | ScanCode::UP) => {
                        ButtonUpdate::Left
                    },
                    Key::Special(ScanCode::RIGHT | ScanCode::DOWN) => {
                        ButtonUpdate::Right
                    },
                    Key::Printable(CHAR16_SPACE | CHAR16_ENTER) => ButtonUpdate::Click,
                    _ => ButtonUpdate::Released
                }
            },
            Err(e) => {
                error!("keyboard error: {e:?}");
                ButtonUpdate::Released
            },
            _ => ButtonUpdate::Released
        }
    };

    let mut keyboard_driver = encoder::Driver::new_buttons(keyboard_update);
    let mut keyboard2 = lvgl2::input::Input::new(&mut keyboard_driver);
    keyboard2.set_group(button_group);

    services.set_watchdog_timer(0, 0x10000, None).unwrap();

    const LVGL_TICK_DELAY: Duration = Duration::from_millis(30);

    extern "efiapi" fn timer_callback(_: Event, _: Option<NonNull<core::ffi::c_void>>) {
        lvgl2::timer_handler();
        lvgl2::tick_increment(LVGL_TICK_DELAY);
    }

    let timer_event = unsafe { services.create_event(EventType::TIMER | EventType::NOTIFY_SIGNAL, Tpl::CALLBACK, Some(timer_callback), None) }.unwrap();
    services.set_timer(&timer_event, TimerTrigger::Periodic((LVGL_TICK_DELAY.as_millis() * 10).try_into().unwrap())).unwrap();

    if let Ok(image) = services.open_protocol_exclusive::<LoadedImage>(image_handle) {
        debug!("Loaded at base addr {:p}", image.info().0);
    }

    const AUTOBOOT_DELAY_TIME: Duration = Duration::from_secs(1);

    let autoboot_event = unsafe { services.create_event(EventType::TIMER, Tpl::APPLICATION, None, None) }.unwrap();
    services.set_timer(&autoboot_event, TimerTrigger::Relative((AUTOBOOT_DELAY_TIME.as_millis() * 10).try_into().unwrap())).unwrap();

    let autoboot_event_num = boot_start_events.len();
    boot_start_events.push(autoboot_event);

    loop {
        let event_num = services.wait_for_event(&mut boot_start_events).unwrap();
        if event_num == 5 {
            info!("autoboot delay up");
        } else {
            info!("button {event_num} pressed");
        }

        match event_num {
            0 | 5 => break,
            _ => todo!("Buttons")
        }
    }

    let (mut kernel, symbol_map) = locate_kernel(&image_handle, &services);


    // =========== test code using kernel from efi part ===========

    /*

    let modules = config.kernel_config.modules.into_iter().map(CString16::try_from)
                        .map(|r| r.map(PathBuf::from))
                        .collect::<Result<Vec<_>, _>>()
                        .expect("Invalid module path");

     */

    // FIXME: This shouldn't just be KERNEL_CODE
    let kernel = elf::load_kernel(&mut kernel, |count, ty| services.allocate_pages(ty, memory_types::KERNEL_CODE, count))
            .expect("Unable to load kernel");
    let elf::KernelLoadInfo { kernel, mut page_table, address_range } = kernel;
    let mut address_range = {
        VirtualAddress::align_down::<4096>(address_range.start)..VirtualAddress::align_up::<4096>(address_range.end)
    };

    let kernel_symbols = kernel.exported_symbols();
    debug!("{:x?}", kernel_symbols);

    /*let mut testing_fn: u64 = 0;
    for module in &modules {
        let result: Result<(),ModuleLoadError> = try {
            let base = kernel_last_page;
            info!("Loading module from `{}` at base address of {:#x}", module, base);
            let mut module = fs.read(module).map_err(|_| ModuleLoadError::FileNotFound)?;

            let module = {
                let mut module = ::elf::File::try_new(&mut module).map_err(|_| ModuleLoadError::InvalidElf)?;
                module.relocate(base.try_into().unwrap());
                module.link(&kernel_symbols).map_err(|e| ModuleLoadError::LinkingFailed(e.name().to_owned()))?;
                module
            };

            module.segments().filter(|segment| segment.segment_type == SegmentType::LOAD)
                  .try_for_each(|segment| {
                      let segment_vaddr = usize::try_from(segment.vaddr).unwrap();

                      let page_count = (usize::try_from(segment.memory_size).unwrap() + PAGE_SIZE - 1) / PAGE_SIZE;
                      let last_page = segment_vaddr + page_count * PAGE_SIZE;
                      if last_page > kernel_last_page { kernel_last_page = last_page; }

                      let Ok(allocation) = services.allocate_pages(AllocateType::AnyPages, memory_types::MODULE_CODE, page_count) else {
                          return Err(ModuleLoadError::Oom);
                      };

                      unsafe {
                          ptr::copy_nonoverlapping(module[segment.file_location()].as_ptr(), allocation as *mut _, segment.file_size.try_into().unwrap());
                          ptr::write_bytes((allocation + segment.file_size) as *mut u8, 0, (segment.memory_size - segment.file_size).try_into().unwrap());
                      }

                      kernel_page_table.try_map_range(Page(segment_vaddr.try_into().unwrap()), Frame(allocation.try_into().unwrap()), page_count.try_into().unwrap(), || todo!())
                                       .map_err(|e: MapError<()>| match e {
                                           MapError::AlreadyMapped => unreachable!(),
                                           MapError::SelfMapOverwrite => panic!("Attempted to overwrite page table self map"),
                                           MapError::AllocationError(_) => ModuleLoadError::Oom
                                       })?;

                      Ok(())
                  })?;

            let module_exports = module.exported_symbols();
            let mut author = "[UNKNOWN]";
            let mut fqn = "[UNKNOWN]";
            let mut name = Option::<&str>::None;

            if let Some(allocator_entrypoint) = module_exports.get(c"__popcorn_module_main_allocator") {
                testing_fn = allocator_entrypoint.value.get();
            }
            if let Some(symbol) = module_exports.get(c"__popcorn_module_author") {
                let author_data = module.data_at_address(symbol.value).unwrap();
                let author_data = unsafe { &*slice_from_raw_parts(author_data, symbol.size.try_into().unwrap()) };
                author = core::str::from_utf8(author_data).map_err(|_| ModuleLoadError::InvalidAuthorMetadata)?;
            }
            if let Some(symbol) = module_exports.get(c"__popcorn_module_modulename") {
                let name_data = module.data_at_address(symbol.value).unwrap();
                let name_data = unsafe { &*slice_from_raw_parts(name_data, symbol.size.try_into().unwrap()) };
                name = Some(core::str::from_utf8(name_data).map_err(|_| ModuleLoadError::InvalidNameMetadata)?);
            }
            if let Some(symbol) = module_exports.get(c"__popcorn_module_modulefqn") {
                let fqn_data = module.data_at_address(symbol.value).unwrap();
                let fqn_data = unsafe { &*slice_from_raw_parts(fqn_data, symbol.size.try_into().unwrap()) };
                fqn = core::str::from_utf8(fqn_data).map_err(|_| ModuleLoadError::InvalidFqnMetadata)?;
            }

            match name {
                Some(name) => info!("Loaded module `{name}` ({fqn}) by `{author}`"),
                None => info!("Loaded module `{fqn}` by `{author}`")
            }
        };

        if let Err(e) = result {
            panic!("Failed to load module: {e}")
        }
    }*/

    services.close_event(timer_event).unwrap();
    drop(ui);

    // map framebuffer
    let framebuffer_info: Option<handoff::Framebuffer> = try {
        use uefi::proto::console::gop::PixelBitmask;

        let mode_info = gop.current_mode_info();
        let mut framebuffer_info = gop.frame_buffer();
        let (width, height) = mode_info.resolution();

        let page_count = (framebuffer_info.size() + PAGE_SIZE - 1) / PAGE_SIZE;
        address_range.start = (address_range.start - page_count * PAGE_SIZE).align_down();
        let fb_start = address_range.start;

        let framebuffer_addr = framebuffer_info.as_mut_ptr() as usize;

        page_table.try_map_range_with::<(), _>(
            Page(fb_start.addr.try_into().unwrap()),
            Frame(framebuffer_addr.try_into().unwrap()),
            page_count.try_into().unwrap(),
            || services.allocate_pages(AllocateType::AnyPages, memory_types::PAGE_TABLE, 1).map_err(|_| ()),
            TableEntryFlags::WRITABLE | TableEntryFlags::NO_EXECUTE | TableEntryFlags::MMIO
        ).ok()?;

        let color_format = match mode_info.pixel_format() {
            PixelFormat::Rgb => Some(ColorMask::RGBX),
            PixelFormat::Bgr => Some(ColorMask::BGRX),
            PixelFormat::Bitmask => {
                let PixelBitmask{ red, green, blue, .. } = mode_info.pixel_bitmask().unwrap();
                Some(ColorMask{ red, green, blue })
            },
            PixelFormat::BltOnly => None
        }?;

        handoff::Framebuffer {
            buffer: fb_start.addr as *mut u8,
            stride: mode_info.stride(),
            width,
            height,
            color_format
        }
    };

    let stack_top = {
        const STACK_PAGE_COUNT: usize = 32;
        address_range.start = VirtualAddress::align_down(address_range.start - STACK_PAGE_COUNT*4096);

        let Ok(allocation) = services.allocate_pages(AllocateType::AnyPages, memory_types::KERNEL_STACK, STACK_PAGE_COUNT) else {
            panic!("Failed to allocate enough memory to load popcorn2");
        };

        page_table.try_map_range::<(), _>(Page(address_range.start.addr.try_into().unwrap()), Frame(allocation), STACK_PAGE_COUNT.try_into().unwrap(), || services.allocate_pages(AllocateType::AnyPages, memory_types::PAGE_TABLE, 1).map_err(|_| ()))
                         .unwrap();

        address_range.start + STACK_PAGE_COUNT*4096
    };

    let symbol_map = symbol_map.map(|m| NonNull::from(&mut *m.into_boxed_slice()));

    info!("new stack top at {:#x}", stack_top.addr);

    // allocate before getting memory map from UEFI
    let mut kernel_mem_map = {
        let size = services.memory_map_size();
        Vec::with_capacity(size.map_size / size.entry_size + 16)
    };

    let mut memory_map_buffer = {
        let size = services.memory_map_size();
        let size = size.map_size + size.entry_size * 16;
        vec![0u8; size]
    };
    let mut memory_map = services.memory_map(
        MemoryDescriptor::align_buf(&mut memory_map_buffer).unwrap()
    ).unwrap();

    let stack_ptr: u64;
    unsafe { asm!("mov {}, rsp", out(reg) stack_ptr); }

    for mem in memory_map.entries().filter(|mem|
            mem.ty == MemoryType::LOADER_DATA ||
                    mem.ty == MemoryType::LOADER_CODE ||
                    (mem.phys_start..mem.phys_start + mem.page_count * 4096).contains(&stack_ptr)
    ) {
        debug!("{:x?} ({:#x} -> {:#x}) - {:?}", mem.ty, mem.phys_start, mem.phys_start + mem.page_count * 4096, mem.att);

        // UEFI memory sections are always aligned by firmware
        (0..mem.page_count).map(|page_num| mem.phys_start + page_num * 4096).try_for_each(|addr| {
            page_table.try_map_page::<(), _>(Page(addr), Frame(addr), || services.allocate_pages(AllocateType::AnyPages, memory_types::PAGE_TABLE, 1).map_err(|_| ()))
        }).unwrap();
    }

    debug!("Generating kernel memory map");
    let kernel_mem_map = {
        let descriptor_to_entry = |descriptor: &MemoryDescriptor| {
            use handoff::MemoryType::*;

            let mut ty = match descriptor.ty {
                MemoryType::CONVENTIONAL |
                MemoryType::BOOT_SERVICES_CODE |
                MemoryType::BOOT_SERVICES_DATA |
                MemoryType::PERSISTENT_MEMORY => Free,
                MemoryType::LOADER_CODE => BootloaderCode,
                MemoryType::LOADER_DATA => BootloaderData,
                memory_types::KERNEL_CODE => KernelCode,
                memory_types::PAGE_TABLE => KernelPageTable,
                memory_types::MODULE_CODE => ModuleCode,
                MemoryType::ACPI_NON_VOLATILE => AcpiPreserve,
                MemoryType::ACPI_RECLAIM => AcpiReclaim,
                memory_types::KERNEL_STACK => KernelStack,
                MemoryType::RUNTIME_SERVICES_CODE => RuntimeCode,
                MemoryType::RUNTIME_SERVICES_DATA => RuntimeData,
                _ => Reserved
            };

            MemoryMapEntry {
                coverage: Range(PhysicalAddress::new(descriptor.phys_start.try_into().unwrap()), PhysicalAddress::new((descriptor.phys_start + descriptor.page_count * 4096).try_into().unwrap())),
                ty
            }
        };

        memory_map.sort();
        let mem_map_data = memory_map.entries().map(|entry| {
            descriptor_to_entry(entry)
        });
        assert_lt!(mem_map_data.len(), kernel_mem_map.capacity());
        let last_item = mem_map_data.reduce(|old_item, new_item| {
            if old_item.ty == new_item.ty && old_item.coverage.1 == new_item.coverage.0 {
                MemoryMapEntry {
                    ty: old_item.ty,
                    coverage: Range(old_item.coverage.0, new_item.coverage.1)
                }
            } else {
                kernel_mem_map.push(old_item);
                new_item
            }
        });
        if let Some(item) = last_item {
            kernel_mem_map.push(item);
        };
        kernel_mem_map
    };

    let kernel_entry = kernel.entrypoint();
    debug!("Handover to kernel with entrypoint at {:#x}", kernel_entry);

    drop(button_reboot);
    drop(button_off);
    drop(pointer);
    drop(keyboard2);
    drop(pointer_driver);
    drop(keyboard_driver);
    drop(gop);
    drop(fs);
    drop(uart);
    drop(mouse);
    drop(keyboard);

    let handoff = handoff::Data {
        framebuffer: framebuffer_info,
        memory: handoff::Memory {
            map: kernel_mem_map,
            page_table_root: (&page_table).into(),
            stack: handoff::Stack {
                top: stack_top.addr,
                bottom: address_range.start.addr
            }
        },
        modules: handoff::Modules {

        },
        log: handoff::Logging {
            symbol_map
        },
        test: handoff::Testing {
            module_func: unsafe { mem::transmute(1usize) }
        }
    };

    let _ = system_table.exit_boot_services();
    page_table.switch();

    //type KernelStart = ffi_abi!(type fn(&handoff::Data) -> !);
    //let kernel_entry: KernelStart = unsafe { mem::transmute(kernel_entry) };
    unsafe {
        asm!("
            mov rsp, {}
            xor ebp, ebp
            call {}
        ", in(reg) stack_top.addr, in(reg) kernel_entry, in("rdi") &handoff, options(noreturn))
    }
}

fn locate_kernel(image_handle: &Handle, services: &BootServices) -> (Vec<u8>, Option<Vec<u8>>) {
    // FIXME: this doesn't check which disk is being used so it'll happily load popcorn from any random disk

    let mut root_partition_handle: Option<Handle> = None;
    if let Ok(partition_handles) = services.locate_handle_buffer(SearchType::ByProtocol(&PartitionInfo::GUID)) {
        for partition_handle in partition_handles.iter() {
            let partition_info = services.open_protocol_exclusive::<PartitionInfo>(*partition_handle).unwrap();
            match partition_info.gpt_partition_entry() {
                Some(gpt_entry) if {
                    let guid = gpt_entry.partition_type_guid.0;
                    guid == const { Guid::parse_or_panic("8A6CC16C-D110-46F1-813F-0382046342C8") }
                } => root_partition_handle = Some(*partition_handle),
                _ => continue
            }
        }
    }

    let root_partition_handle = root_partition_handle.expect("No popcorn system disk found");

    let mut fs = {
        debug!("root partition protos: {:?}", services.protocols_per_handle(root_partition_handle).as_deref());

        let Ok(proto) = services.open_protocol_exclusive::<SimpleFileSystem>(root_partition_handle) else {
            panic!()
        };

        FileSystem::new(proto)
    };

    // TODO: versioning
    let symbol_map = fs.read(Path::new(cstr16!(r"\kernel\kernel.map"))).ok();
    let kernel_data = fs.read(Path::new(cstr16!(r"\kernel\kernel.exec"))).expect("Unable to find a bootable kernel");

    /*
    let symbol_map = fs.read(Path::new(cstr16!(r"\EFI\POPCORN\symbols.map")))
                       .ok().map(|v| {
        debug!("{:x?}", &v[0..10]);
        let p = Box::into_raw(v.into_boxed_slice());
        unsafe { NonNull::new_unchecked(p) }
    });
     */
    (kernel_data, symbol_map)
}

#[panic_handler]
fn panic_handler(info: &PanicInfo) -> ! {
    error!("{}", info);
    loop {}
}

/*
| UEFI type                  | Use                                                              |
|----------------------------|------------------------------------------------------------------|
| EfiReservedMemoryType      | Unusable memory                                                  |
| EfiLoaderCode              | Bootloader code                                                  |
| EfiLoaderData              | Bootloader data and memory allocations by bootloader             |
| EfiBootServicesCode        | Boot services driver code - preserve to use boot services        |
| EfiBootServicesData        | Boot services driver data - preserve to use boot services        |
| EfiRuntimeServicesCode     | Runtime services driver code - preserve to use runtime services |
| EfiRuntimeServicesData     | Runtime services driver data - preserve to use runtime services |
| EfiConventionalMemory      | Free memory                                                      |
| EfiUnusableMemory          | Memory with errors detected                                      |
| EfiACPIReclaimMemory       | Memory containing ACPI tables - preserve until parsing ACPI      |
| EfiACPIMemoryNVS           | ACPI firmware memory that must be preserved across sleep         |
| EfiMemoryMappedIO          | ???                                                              |
| EfiMemoryMappedIOPortSpace | ???                                                              |
| EfiPalCode                 | ???                                                              |
| EfiPersistentMemory        | Non-volatile but otherwise conventional memory                   |
| EfiUnacceptedMemoryType    | ???                                                              |
 */

mod memory_types {
	use uefi::table::boot::MemoryType;

	pub const KERNEL_CODE: MemoryType = MemoryType::custom(0x8000_0000);
    pub const MODULE_CODE: MemoryType = MemoryType::custom(0x8000_0001);
    pub const PAGE_TABLE: MemoryType = MemoryType::custom(0x8000_0002);
    pub const MEMORY_ALLOCATOR_DATA: MemoryType = MemoryType::custom(0x8000_0003);
    pub const KERNEL_STACK: MemoryType = MemoryType::custom(0x8000_0004);
    pub const FRAMEBUFFER: MemoryType = MemoryType::custom(0x8000_0005);
}

#[derive(Display)]
enum ModuleLoadError {
    #[display(fmt = "Could not locate requested module")]
    FileNotFound,
    #[display(fmt = "Module file is corrupted")]
    InvalidElf,
    #[display(fmt = "Failed to resolve symbol {_0:?}")]
    LinkingFailed(CString),
    #[display(fmt = "Could not allocate memory for module")]
    Oom,
    #[display(fmt = "Invalid data in `author` metadata")]
    InvalidAuthorMetadata,
    #[display(fmt = "Invalid data in `name` metadata")]
    InvalidNameMetadata,
    #[display(fmt = "Invalid data in `fqn` metadata")]
    InvalidFqnMetadata,
}
