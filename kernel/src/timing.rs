use core::sync::atomic::AtomicU64;

// FIXME: system time breaks if you forget to reboot after 500 years
#[export_name = "__popcorn_system_time"]
pub(crate) fn system_time() -> u128 { 0 }
