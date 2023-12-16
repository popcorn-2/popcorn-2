#![feature(new_uninit)]
#![feature(decl_macro)]
#![feature(generic_const_items)]
#![feature(ptr_metadata)]
#![feature(auto_traits)]
#![feature(negative_impls)]
#![no_std]

extern crate alloc;

use core::time::Duration;

pub mod display;
pub mod misc;
pub mod object;
pub mod font;
pub mod input;

#[derive(Debug)]
pub struct Error;

pub fn init() {
	unsafe { lvgl_sys::lv_init(); }
}

pub fn tick_increment(elapsed: Duration) {
	unsafe { lvgl_sys::lv_tick_inc(elapsed.as_millis() as u32) }
}

pub fn timer_handler() {
	unsafe { lvgl_sys::lv_timer_handler(); }
}
