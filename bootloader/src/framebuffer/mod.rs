use aliasable::boxed::AliasableBox;
use alloc::boxed::Box;
use alloc::vec;
use core::{fmt, mem};
use core::ops::{Deref, Index, IndexMut};
use core::ptr::slice_from_raw_parts;
use derive_more::Constructor;
use log::trace;
use lvgl2::display;
use lvgl2::display::buffer::DrawBuffer;
use lvgl2::display::driver::DisplayUpdate;
use psf::PsfFont;
use uefi::prelude::BootServices;
use uefi::proto::console::gop::{BltOp, BltPixel, BltRegion, GraphicsOutput};
use uefi::proto::unsafe_protocol;

use crate::logging::FormatWrite;

#[derive(Constructor)]
pub struct FontFamily<'a> {
	regular: &'a dyn PsfFont,
	bold: Option<&'a dyn PsfFont>,
	italic: Option<&'a dyn PsfFont>,
	bold_italic: Option<&'a dyn PsfFont>
}

impl<'a> FontFamily<'a> {
	pub fn get_available_style(&self, style: FontStyle) -> FontStyle {
		if self.font_exists_for_style(style) { style }
		else {
			*style.fallbacks().iter()
			      .rev()
			      .reduce(|current, fallback| {
				      if self.font_exists_for_style(*fallback) { fallback }
				      else { current }
			      }).unwrap()
		}
	}

	fn font_exists_for_style(&self, style: FontStyle) -> bool {
		match style {
			FontStyle::Regular => true,
			FontStyle::Bold => self.bold.is_some(),
			FontStyle::Italic => self.italic.is_some(),
			FontStyle::BoldItalic => self.bold_italic.is_some()
		}
	}

	fn get_font_for_style(&self, style: FontStyle) -> &'a dyn PsfFont {
		match self.get_available_style(style) {
			FontStyle::Regular => self.regular,
			FontStyle::Bold => self.bold.unwrap(),
			FontStyle::Italic => self.italic.unwrap(),
			FontStyle::BoldItalic => self.bold_italic.unwrap()
		}
	}
}

#[derive(Copy, Clone, Debug)]
pub enum FontStyle {
	Regular,
	Bold,
	Italic,
	BoldItalic
}

impl FontStyle {
	pub fn fallbacks(self) -> &'static [Self] {
		match self {
			Self::Regular | Self::Bold | Self::Italic => &[Self::Regular],
			Self::BoldItalic => &[Self::Bold, Self::Italic, Self::Regular]
		}
	}
}

pub struct PixelBuffer(pub Box<[BltPixel]>, pub usize);

impl PixelBuffer {
	fn pixel(&self, x: usize, y: usize) -> &BltPixel { &self.0[x + (y * self.1)] }
	fn pixel_mut(&mut self, x: usize, y: usize) -> &mut BltPixel {
		&mut self.0[x + (y * self.1)]
	}
}

impl Index<(usize, usize)> for PixelBuffer {
	type Output = BltPixel;

	fn index(&self, index: (usize, usize)) -> &Self::Output {
		self.pixel(index.0, index.1)
	}
}

impl IndexMut<(usize, usize)> for PixelBuffer {
	fn index_mut(&mut self, index: (usize, usize)) -> &mut Self::Output {
		self.pixel_mut(index.0, index.1)
	}
}

pub struct Gui<'gop> {
	//width: usize,
	//height: usize,

	// SAFETY: THIS SECTION MUST BE KEPT IN ORDER
	// `display` references `display_driver` so must be dropped first
	// same for `display_driver` and `draw_buffer`
	// The 'gop is a lie and in fact is only the lifetime of the struct itself
	// This is safe because drop order means that each reference will be dropped before its owner
	/*display: display_driver::Display<'gop, 'gop>,
	display_driver: display_driver::DisplayDriver<'gop>,
	draw_buffer: draw_buffer::DrawBuffer,
	_phantom: PhantomPinned,*/
	pub(crate) display: display::Display<'static, 'static, 'static>,
	driver: AliasableBox<display::driver::Driver<'gop, 'static>>,
	buffer: AliasableBox<DrawBuffer>
	//_phantom: PhantomData<&'a mut u8>
}

impl Gui<'_> {
	fn flush_display(gop: &mut GraphicsOutput, update: DisplayUpdate) {
		let update_width = update.area.x2 - update.area.x1 + 1;
		let update_height = update.area.y2 - update.area.y1 + 1;

		for c in update.colors.iter_mut() {
			c.set_a(0);
		}

		let buffer: &[BltPixel] = unsafe {
			// SAFETY: alpha channel has been set to 0 to comply with UEFI reserved byte requirements
			// memory layout of BltPixel and LVGL Color is identical
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
	}

	pub fn new(gop: &mut GraphicsOutput) -> Gui<'_> {
		use display::{Display, driver::Driver};

		let (width, height) = gop.current_mode_info().resolution();

		lvgl2::init();

		let mut buffer = AliasableBox::from_unique(Box::new(DrawBuffer::new(8000)));
		let mut driver = AliasableBox::from_unique(Box::new(Driver::new(
			unsafe { mem::transmute(&mut *buffer) },
			width,
			height,
			|update| Self::flush_display(gop, update)
		)));
		//let driver = Box::leak(Box::new(driver));
		let display = Display::new(unsafe { mem::transmute(&mut *driver) });

		trace!("lvgl initialised");

		Gui {
			display,
			driver,
			buffer
		}
	}
}

pub struct Tui<'a, 'b> {
	width: usize,
	height: usize,
	double_buffer: PixelBuffer,
	gop: &'a mut GraphicsOutput,
	font: FontFamily<'b>,
	location: (usize, usize),
	color: BltPixel,
	current_style: FontStyle
}

impl<'a, 'b> Tui<'a, 'b> {
	pub fn new(gop: &'a mut GraphicsOutput, pointer: &mut uefi::proto::console::pointer::Pointer, keyboard: &mut uefi::proto::console::text::Input, boot_services: &BootServices, native_resolution: (usize, usize), font: FontFamily<'b>) -> Self {
		let native_resolution = gop.modes().find(|mode| mode.info().resolution() == native_resolution);
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

		let actual_mode =
				if let Some(resolution) = native_resolution.or(optimal_resolution) &&
						gop.set_mode(&resolution).is_ok()
				{
					*resolution.info()
				} else {
					gop.current_mode_info()
				};

		let (width, height) = actual_mode.resolution();
		let aspect = width as f32 / height as f32;

		/*lvgl::init();

		let buf = DrawBuffer::<{20 * 400}>::default();
		let disp = Display::register(buf, width as u32, height as u32, |refresh| {
			let refresh_width = refresh.area.x2 - refresh.area.x1 + 1;
			let refresh_height = refresh.area.y2 - refresh.area.y1 + 1;

			trace!("display refresh");

			let blt_op = BltOp::BufferToVideo {
				buffer: unsafe {
					// SAFETY: Both buffers are in the form RGBX
					core::mem::transmute(&refresh.colors[..])
				},
				src: BltRegion::Full,
				dest: (refresh.area.x1.try_into().unwrap(), refresh.area.y1.try_into().unwrap()),
				dims: (refresh_width.try_into().unwrap(), refresh_height.try_into().unwrap()),
			};

			gop.blt(blt_op).expect("TODO: panic message");
		}).unwrap();

		let mut screen = disp.get_scr_act().unwrap();

		let mut screen_style = {
			let mut screen_style = Style::default();
			screen_style.set_bg_color(Color::from_rgb((0x33, 0x33, 0x33)));
			screen_style.set_text_color(Color::from_rgb((0xee, 0xee, 0xee)));
			screen_style.set_bg_opa(Opacity::OPA_100);
			screen_style.set_border_width(0);
			screen_style.set_radius(0);
			let quicksand_200 = unsafe {
				Font::new_raw(lvgl_sys::quicksand_200)
			};
			screen_style.set_text_font(quicksand_200);
			screen_style
		};

		screen.add_style(Part::Main, &mut screen_style);

		let mut flex_style = {
			let mut flex_style = Style::default();

			flex_style.set_layout(Layout::flex());
			flex_style.set_flex_flow(FlexFlow::COLUMN);
			flex_style.set_flex_main_place(FlexAlign::START);
			flex_style.set_flex_cross_place(FlexAlign::CENTER);
			flex_style.set_flex_track_place(FlexAlign::CENTER);
			//flex_style.set_pad_bottom(24);
			//flex_style.set_pad_top(24);
			flex_style.set_pad_row(12);

			flex_style
		};

		let menu_width = if aspect < 1.8 { pct(90) }
		else {
			/* ultra-wide screen - 90% of 16×9 area */
			i16::try_from(height).unwrap() * 16 / 10
		};

		let mut flex_box = Obj::create(&mut screen).unwrap();
		flex_box.set_size(menu_width, pct(95));
		flex_box.set_align(Align::Center, 0, 0);
		flex_box.add_style(Part::Main, &mut flex_style);

		let mut label = Label::create(&mut flex_box).unwrap();
		label.set_text(cstr_core::cstr!("popcorn")).unwrap();

		let mut item_style = {
			let mut item_style = Style::default();
			item_style.set_text_color(Color::from_rgb((0, 0, 0)));
			let open_sans_48 = unsafe {
				Font::new_raw(lvgl_sys::open_sans_48)
			};
			item_style.set_text_font(open_sans_48);
			item_style.set_bg_color(Color::from_rgb((0xef, 0xbb, 0x40)));
			item_style.set_outline_color(Color::from_rgb((0x1c, 0x8a, 0xeb)));
			item_style.set_bg_opa(Opacity::OPA_100);
			item_style.set_pad_bottom(8);
			item_style.set_pad_top(8);
			item_style.set_pad_left(8);
			item_style.set_pad_right(8);
			item_style
		};

		let mut item_style_hover = {
			let mut item_style = Style::default();
			item_style.set_bg_color(Color::from_rgb((0x1c, 0x8a, 0xeb)));
			item_style
		};

		let mut item_style_focus = {
			let mut item_style = Style::default();
			item_style.set_outline_pad(2);
			item_style.set_outline_width(2);
			item_style
		};

		/*let mut menu_style = Style::default();
		menu_style.set_radius(0);
		menu_style.set_layout(Layout::flex());


		let mut menu_item_style = Style::default();


		let mut boot_menu = Obj::create(&mut screen).unwrap();
		boot_menu.add_style(Part::Main, &mut menu_style);
		let boot_menu_width = if aspect < 1.8 { lvgl::misc::area::pct(90) } else { /* widescreen - 90% of 16×9 area */ i16::try_from(height).unwrap() * 16 / 10 };
		boot_menu.set_size(boot_menu_width, lvgl::misc::area::pct(75).into());
		//boot_menu.set_align(Align::Center, 0, 0);*/

		let mut group = Group::default();

		for _ in 0..5 {
			let mut item = Btn::create(&mut flex_box).unwrap();
			item.set_width(pct(98).try_into().unwrap());
			item.add_style(Part::Main, &mut item_style);
			unsafe {
				lvgl_sys::lv_obj_add_style(
					item.raw().as_mut(),
					core::mem::transmute::<_, &mut NonNull<lvgl_sys::lv_style_t>>(&mut item_style_hover).as_mut() as *mut _,
					lvgl_sys::lv_style_selector_t::from(Part::Main) | lvgl_sys::LV_STATE_PRESSED,
				);

				lvgl_sys::lv_obj_add_style(
					item.raw().as_mut(),
					core::mem::transmute::<_, &mut NonNull<lvgl_sys::lv_style_t>>(&mut item_style_focus).as_mut() as *mut _,
					lvgl_sys::lv_style_selector_t::from(Part::Main) | lvgl_sys::LV_STATE_FOCUSED,
				);
			}
			let mut item_text = Label::create(&mut item).unwrap();
			item_text.set_text(cstr_core::cstr!("Boot menu option")).unwrap();
			group.add_obj(&item).unwrap();
		}

		let mut touch_point = (
			width as f32 / 2f32,
			height as f32 / 2f32
		);
		let mut left_button = false;
		let pointer_mode = *pointer.mode();

		let pointer_update = || {
			match pointer.read_state() {
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

			let point = Point::new(
				touch_point.0 as i32,
				touch_point.1 as i32
			);
			if left_button { PointerInputData::Touch(point).pressed().once() }
			else { PointerInputData::Touch(point).released().once() }
		};

		let keyboard_update = || {
			const CHAR16_SPACE: Char16 = unsafe { Char16::from_u16_unchecked(0x20) };
			const CHAR16_ENTER: Char16 = unsafe { Char16::from_u16_unchecked(0x0d) };

			match keyboard.read_key() {
				Ok(Some(event)) => {
					info!("keyboard event: {event:?}");

					unsafe {
						lvgl_sys::lv_obj_add_style(
							flex_box.raw().as_mut(),
							core::mem::transmute::<_, &mut NonNull<lvgl_sys::lv_style_t>>(&mut item_style_focus).as_mut() as *mut _,
							lvgl_sys::lv_style_selector_t::from(Part::Main) | lvgl_sys::LV_STATE_FOCUSED,
						);
					}

					match event {
						Key::Special(ScanCode::LEFT | ScanCode::UP) => {
							EncoderInputData::TurnLeft.pressed().once()
						},
						Key::Special(ScanCode::RIGHT | ScanCode::DOWN) => {
							EncoderInputData::TurnRight.pressed().once()
						},
						Key::Printable(CHAR16_SPACE | CHAR16_ENTER) => EncoderInputData::Press.pressed().once(),
						_ => EncoderInputData::Press.released().once()
					}
				},
				Err(e) => {
					error!("keyboard error: {e:?}");
					EncoderInputData::Press.released().once()
				},
				_ => EncoderInputData::Press.released().once()
			}
		};

		let mut mouse = Pointer::register(pointer_update, &disp).unwrap();
		let mut keyboard = Encoder::register(keyboard_update, &disp).unwrap();
		group.set_indev(&mut keyboard).unwrap();

		unsafe {
			let cursor_img = lvgl_sys::lv_img_create(screen.raw().as_ptr());
			lvgl_sys::lv_img_set_src(cursor_img, (&lvgl_sys::cursor as *const lvgl_sys::lv_img_dsc_t).cast());
			lvgl_sys::lv_indev_set_cursor(mouse.get_descriptor().unwrap() as *mut lvgl_sys::lv_indev_t, cursor_img);
		}

		let timer_event = unsafe { boot_services.create_event(EventType::TIMER, Tpl::APPLICATION, None, None) }.unwrap();
		boot_services.set_timer(&timer_event, TimerTrigger::Periodic(10000)).unwrap(); // Every 1000ms
		let mut events = [timer_event];

		loop {
			lvgl::task_handler();

			boot_services.wait_for_event(&mut events).unwrap();

			lvgl::tick_inc(Duration::from_millis(1000));

			//info!("tick");

			/*if true /*let Ok(Some(loc)) = pointer.read_state()*/ {
				latest_touch_status.x += 1;//loc.relative_movement[0];
				latest_touch_status.y += 1;//loc.relative_movement[1];

				info!("Cursor at {latest_touch_status:?}");
			}*/
		}*/

		let double_buffer_backing = vec![BltPixel::new(0, 0, 0); width * height];

		Self {
			width,
			height,
			double_buffer: PixelBuffer(double_buffer_backing.into_boxed_slice(), width),
			gop,
			font,
			location: (0, 0),
			color: BltPixel::new(0xee, 0xee, 0xee),
			current_style: FontStyle::Regular
		}
	}

	fn flush(&mut self) -> uefi::Result {
		let buffer = &*self.double_buffer.0;
		let blt_op = BltOp::BufferToVideo {
			buffer,
			src: BltRegion::Full,
			dest: (0, 0),
			dims: (self.width, self.height),
		};

		self.gop.blt(blt_op)
	}

	pub fn set_font_style(&mut self, style: FontStyle) {
		self.current_style = style;
	}

	pub fn set_font_color(&mut self, r: u8, g: u8, b: u8) {
		self.color = BltPixel::new(r, g, b);
	}

	fn shift_up(&mut self) {
		let font = self.font.get_font_for_style(self.current_style);
		let char_height = font.char_height();

		self.double_buffer.0.rotate_right(self.width * (self.height - char_height - 1));
		for i in &mut self.double_buffer.0[self.width * (self.height - char_height - 1)..self.width * self.height] {
			*i = BltPixel::new(0,0,0);
		}
	}

	fn newline(&mut self) {
		let font = self.font.get_font_for_style(self.current_style);
		let char_height = font.char_height();

		let new_y = self.location.1 + char_height + 1;
		if new_y + char_height >= self.height { // If any part of the next line is offscreen
			self.shift_up();
		} else {
			self.location.1 += char_height + 1;
		}
		self.location.0 = 0;
	}

	fn advance(&mut self, n: isize) {
		enum Direction { Left, Right }

		let font = self.font.get_font_for_style(self.current_style);
		let char_width = font.char_width();

		let direction = if n >= 0 { Direction::Right } else { Direction::Left };
		let advance_pixels = n.unsigned_abs() * (char_width + 1);

		match direction {
			Direction::Left => self.location.0 = self.location.0.saturating_sub(advance_pixels),
			Direction::Right => {
				self.location.0 += advance_pixels;
				if (self.location.0 + char_width) > self.width {
					self.newline();
				}
			}
		}
	}
}

impl<'a, 'b> fmt::Write for Tui<'a, 'b> {
	fn write_str(&mut self, s: &str) -> fmt::Result {
		let font = self.font.get_font_for_style(self.current_style);
		let (char_width, char_height) = (font.char_width(), font.char_height());

		for c in s.chars() {
			match c {
				'\n' => {
					self.newline();
					self.flush().map_err(|_| fmt::Error)?;
				},
				'\t' => self.advance(4),
				c => {
					let c = font.locate_char(c).map_err(|_| fmt::Error)?;

					for y in 0..char_height {
						for x in 0..char_width {
							let draw_loc = (self.location.0 + x, self.location.1 + y);
							if c.is_set(x, y) { self.double_buffer[draw_loc] = self.color; }
						}
					}

					self.advance(1);
				}
			}
		}

		Ok(())
	}
}

impl<'a, 'b> FormatWrite for Tui<'a, 'b> {
	fn set_color(&mut self, color: (u8, u8, u8)) {
		self.color = BltPixel::new(color.0, color.1, color.2);
	}

	fn set_font_style(&mut self, style: FontStyle) {
		self.current_style = style;
	}
}

#[repr(C)]
#[unsafe_protocol("bd8c1056-9f36-44ec-92a8-a6337f817986")]
pub struct ActiveEdid {
	edid_size: u32,
	edid_data: *const u8
}

impl Deref for ActiveEdid {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		unsafe { &*slice_from_raw_parts(self.edid_data, self.edid_size.try_into().unwrap()) }
	}
}
