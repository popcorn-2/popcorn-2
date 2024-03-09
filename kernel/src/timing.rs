use core::sync::atomic::AtomicU64;

// FIXME: system time breaks if you forget to reboot after 500 years
#[export_name = "__popcorn_system_time"]
pub(crate) static SYSTEM_TIME_NS: AtomicU64 = AtomicU64::new(0);
