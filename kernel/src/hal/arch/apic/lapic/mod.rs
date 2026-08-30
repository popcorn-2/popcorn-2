use core::pin::Pin;
use core::ptr::addr_of_mut;
use acpi::sdt::madt::Madt;
use bit_field::BitField;
use num_enum::{IntoPrimitive, TryFromPrimitive};
use kernel_api::is_x86_feature_detected;

pub(in crate::hal) mod xapic;
//mod x2apic;
//mod timer;

pub(super) fn init_lapic(madt: Pin<&Madt>) {
	if !is_x86_feature_detected!("apic") {
		panic!("No LAPIC present");
	}
	
	if is_x86_feature_detected!("x2apic") {
		warn!("Ignoring x2apic for now");
		//x2apic::X2Apic::init(madt);
	} else {
		//xapic::XApic::init(madt);
	};

	xapic::XApic::init(madt);
}

macro_rules! lapic_register_ty {
    () => {u32};
	($field_ty:path) => {$field_ty}
}

macro_rules! lapic_registers {
	(
	    $(#[$attr:meta])* $vis:vis struct $name:ident {
	        $(
	            $field_vis:vis $field:ident $(: $field_ty:path)?
	        ),* $(,)?
	    }
    ) => {
	    $(#[$attr])* $vis struct $name {
		    $(
		        $field_vis $field: lapic_register_ty!($($field_ty)?),
		        ${concat(_pad, $field)}: [u32; 3],
		    )*
	    }
    };
}

lapic_registers! {
	#[repr(C)]
	pub struct Registers {
		_res0,
		_res1,
		id,
		version,
		_res2,
		_res3,
		_res4,
		_res5,
		task_priority,
		arbitration_priority,
		processor_priority,
		eoi,
		remote_read,
		logical_destination,
		destination_format,
		spurious_vector,
		_res6,
		_res7,
		_res8,
		_res9,
		_res10,
		_res11,
		_res12,
		_res13,
		_res14,
		_res15,
		_res16,
		_res17,
		_res18,
		_res19,
		_res20,
		_res21,
		_res22,
		_res23,
		_res24,
		_res25,
		_res26,
		_res27,
		_res28,
		_res29,
		_res30,
		_res31,
		_res32,
		_res33,
		_res34,
		_res35,
		_res36,
		_res37,
		icr_low,
		icr_high,
		timer_lvt: TimerLvt,
		thermal_sensor_lvt: Lvt,
		perf_monitor_lvt: Lvt,
		lint0_lvt: Lvt,
		lint1_lvt: Lvt,
		error_lvt: Lvt,
		timer_initial_count,
		timer_current_count,
		_res40,
		_res41,
		_res42,
		_res43,
		timer_divide_config,
	}
}

#[repr(transparent)]
struct Lvt(u32);

impl Lvt {
	#[expect(dead_code)]
	unsafe fn update(self: *mut Self, mut f: impl FnMut(&mut LvtState)) {
		unsafe {
			let _ = addr_of_mut!((*self).0).fetch_update_io(|old| {
				let mut state = old.into();
				f(&mut state);
				Some(state.into())
			});
		}
	}
}

#[derive(TryFromPrimitive, IntoPrimitive, Debug)]
#[repr(u32)]
pub enum DeliveryMode {
	Fixed = 0,
	Reserved1 = 1,
	Smi = 2,
	Reserved3 = 3,
	Nmi = 4,
	Init = 5,
	Reserved6 = 6,
	ExtInt = 7
}

struct LvtState {
	masked: bool,
	mode: DeliveryMode,
	vector: u8,
}

impl From<u32> for LvtState {
	fn from(value: u32) -> Self {
		Self {
			masked: value.get_bit(16),
			mode: DeliveryMode::try_from(value.get_bits(8..=10)).unwrap(),
			vector: value.get_bits(0..=7) as u8,
		}
	}
}

impl From<LvtState> for u32 {
	fn from(value: LvtState) -> Self {
		*0.set_bit(16, value.masked)
				.set_bits(8..=10, value.mode.into())
				.set_bits(0..=7, value.vector.into())
	}
}

#[repr(transparent)]
struct TimerLvt(Lvt);
