use super::cache::Initializer;
use super::features_macro;

use core::arch::x86_64::{__cpuid, __cpuid_count, CpuidResult};
use core::mem;

features_macro! {
	@TARGET: x86;
	@CFG: any(target_arch = "x86", target_arch = "x86_64", doc);
	@MACRO_NAME: is_x86_feature_detected;
	@MACRO_ATTRS: #[doc(cfg(any(target_arch = "x86", target_arch = "x86_64")))]
	@FEATURE: tsc: "tsc": "system has a timestamp counter accessible through `rdtsc`";
	@FEATURE: msr: "msr": "system supports reading and writing model specific registers through `rdmsr` and `wrmsr`";
	@FEATURE: apic: "apic": "system has an APIC";
	@FEATURE: cx16: "cx16": "system support 16-byte atomics";
	@FEATURE: pcid: "pcid": "system supports PCIDs in page tables";
	@FEATURE: x2apic: "x2apic": "APIC supports x2APIC features";
	@FEATURE: tsc_deadline: "tsc_deadline": "local APIC timer supports TSC deadline mode";
	@FEATURE: xsave: "xsave": "system supports the `xsave` instruction";
	@FEATURE: xsaveopt: "xsaveopt": "system supports the `xsaveopt` instruction";
	@FEATURE: xsavec: "xsavec": "system supports the `xsavec` instruction";
	@FEATURE: hypervisor: "hypervisor": "system is running in a hypervisor";
	@FEATURE: arat: "arat": "local APIC timer continues counting while sleeping";
	@FEATURE: intel_thread_director: "intel_thread_director": "system supports Intel Thread Director";
	@FEATURE: fsgsbase: "fsgsbase": "system supports writing to the `fs` and `gs` registers from userspace";
	@FEATURE: tsc_adjust: "tsc_adjust": "system supports adjusting TSC value per-core";
	@FEATURE: smep: "smep": "system supports Supervisor Mode Execution Prevention";
	@FEATURE: invpcid: "invpcid": "system supports invalidating an entire PCID from the TLB";
	@FEATURE: smap: "smap": "system supports Supervisor Mode Access Prevention";
	@FEATURE: la57: "la57": "system supports 57-bit virtual addresses";
	@FEATURE: rdpid: "rdpid": "system supports reading OS core ID from `IA32_TSC_AUX`";
	@FEATURE: hybrid: "hybrid": "system has AMP";
	@FEATURE: lass: "lass": "system supports trapping access based on MSB of address";
	@FEATURE: nmi_src: "nmi_src": "system supports reporting source of NMI exceptions";
	@FEATURE: xsave_x87: "xsave_x87": "system supports saving x87 registers with `xsave`";
	@FEATURE: xsave_sse: "xsave_sse": "system supports saving SSE registers with `xsave`";
	@FEATURE: xsave_avx: "xsave_avx": "system supports saving AVX registers with `xsave`";
	@FEATURE: xsave_avx512_opmask: "xsave_avx512_opmask": "system supports saving AVX512 opmask registers with `xsave`";
	@FEATURE: xsave_avx512_zmm_hi256: "xsave_avx512_zmm_hi256": "system supports saving upper 256 bits of `zmm0` through `zmm15` with `xsave`";
	@FEATURE: xsave_avx512_zmm_hi16: "xsave_avx512_zmm_hi16": "system supports saving `zmm16` through `zmm31` with `xsave`";
	@FEATURE: xsave_pkru: "xsave_pkru": "system supports saving PKRU register with `xsave`";
	@FEATURE: xsave_amx_cfg: "xsave_amx_cfg": "system supports saving AMX `tilecfg` register with `xsave`";
	@FEATURE: xsave_amx_tile_data: "xsave_amx_tile_data": "system supports saving AMX `tmm0` through `tmm7` registers with `xsave`";
	@FEATURE: xsave_apx_gpr: "xsave_apx_gpr": "system supports saving `r16` through `r31` GPRs with `save`";
	@FEATURE: nx: "nx": "page tables support no-execute bit";
	@FEATURE: pdpe1gb: "pdpe1gb": "page tables support 1 GiB huge pages";
	@FEATURE: rdtscp: "rdtscp": "system supports `rdtscp` instruction to read TSC and `IA32_TSC_AUX`";
	@FEATURE: extapic: "extapic": "APIC support Extended APIC Space";
	@FEATURE: invlpgb: "invlpgb": "system supports `invlpgb` instruction";
}

pub(super) fn detect() -> Initializer {
	let mut initializer = Initializer::new();

	let (max_basic_leaf, _vendor_id) = {
		let CpuidResult {
			eax: max_basic_leaf,
			ebx,
			ecx,
			edx,
		} = __cpuid(0);
		let vendor_id: [[u8; 4]; 3] = [
			u32::to_ne_bytes(ebx),
			u32::to_ne_bytes(edx),
			u32::to_ne_bytes(ecx),
		];
		let vendor_id: [u8; 12] = unsafe { mem::transmute(vendor_id) };
		(max_basic_leaf, vendor_id)
	};

	if max_basic_leaf < 1 { return initializer; }

	let CpuidResult {
		ecx: proc_info_ecx,
		edx: proc_info_edx,
		..
	} = __cpuid(0x0000_0001_u32);

	let thermal_power_eax = if max_basic_leaf >= 6 {
		let CpuidResult { eax, .. } = __cpuid(0x0000_0006_u32);
		eax
	} else {
		0 // CPUID does not support "Thermal/power Management Features"
	};

	let (extended_features_eax, extended_features_ebx, extended_features_ecx, extended_features_edx) = if max_basic_leaf >= 7 {
		let CpuidResult { eax, ebx, ecx, edx } = __cpuid(0x0000_0007_u32);
		(eax, ebx, ecx, edx)
	} else {
		(0, 0, 0, 0) // CPUID does not support "Extended Features"
	};

	let extended_features_1_eax = if extended_features_eax >= 1 {
		let CpuidResult { eax, .. } = __cpuid_count(0x0000_0007, 1);
		eax
	} else {
		0 // CPUID does not support "Extended Features"
	};

	let (_xsave_xcr0_high, xsave_xcr0_low) = if proc_info_ecx & (1 << 26) != 0 {
		let CpuidResult { eax, edx, .. } = __cpuid_count(0x0000_000d_u32, 0);
		(edx, eax)
	} else {
		(0, 0)
	};

	let xsave_feature = if proc_info_ecx & (1 << 26) != 0 {
		let CpuidResult { eax, .. } = __cpuid_count(0x0000_000d_u32, 1);
		eax
	} else {
		0
	};

	let CpuidResult {
		eax: extended_max_basic_leaf,
		..
	} = __cpuid(0x8000_0000_u32);

	let (extended_proc_info_ecx, extended_proc_info_edx) = if extended_max_basic_leaf >= 1 {
		let CpuidResult { ecx, edx, .. } = __cpuid(0x8000_0001_u32);
		(ecx, edx)
	} else {
		(0, 0)
	};

	let paging_features_ebx = if extended_max_basic_leaf >= 8 {
		let CpuidResult { ebx, .. } = __cpuid(0x8000_0008_u32);
		ebx
	} else {
		0
	};

	{
		let mut enable = |value: u32, bit: u32, feature: Feature| {
			if value & (1 << bit) != 0 {
				initializer.set(feature);
			}
		};

		enable(proc_info_edx, 4, Feature::tsc);
		enable(proc_info_edx, 5, Feature::msr);
		enable(proc_info_edx, 9, Feature::apic);
		enable(proc_info_ecx, 13, Feature::cx16);
		enable(proc_info_ecx, 17, Feature::pcid);
		enable(proc_info_ecx, 21, Feature::x2apic);
		enable(proc_info_ecx, 24, Feature::tsc_deadline);
		enable(proc_info_ecx, 26, Feature::xsave);
		enable(xsave_feature, 0, Feature::xsaveopt);
		enable(xsave_feature, 1, Feature::xsavec);
		enable(proc_info_ecx, 31, Feature::hypervisor);
		enable(thermal_power_eax, 2, Feature::arat);
		enable(thermal_power_eax, 23, Feature::intel_thread_director);
		enable(extended_features_ebx, 0, Feature::fsgsbase);
		enable(extended_features_ebx, 1, Feature::tsc_adjust);
		enable(extended_features_ebx, 7, Feature::smep);
		enable(extended_features_ebx, 10, Feature::invpcid);
		enable(extended_features_ebx, 20, Feature::smap);
		enable(extended_features_ecx, 16, Feature::la57);
		enable(extended_features_ecx, 22, Feature::rdpid);
		enable(extended_features_edx, 15, Feature::hybrid);
		enable(extended_features_1_eax, 6, Feature::lass);
		enable(extended_features_1_eax, 20, Feature::nmi_src);
		enable(xsave_xcr0_low, 0, Feature::xsave_x87);
		enable(xsave_xcr0_low, 1, Feature::xsave_sse);
		enable(xsave_xcr0_low, 2, Feature::xsave_avx);
		enable(xsave_xcr0_low, 5, Feature::xsave_avx512_opmask);
		enable(xsave_xcr0_low, 6, Feature::xsave_avx512_zmm_hi256);
		enable(xsave_xcr0_low, 7, Feature::xsave_avx512_zmm_hi16);
		enable(xsave_xcr0_low, 9, Feature::xsave_pkru);
		enable(xsave_xcr0_low, 17, Feature::xsave_amx_cfg);
		enable(xsave_xcr0_low, 18, Feature::xsave_amx_tile_data);
		enable(xsave_xcr0_low, 19, Feature::xsave_apx_gpr);
		enable(extended_proc_info_edx, 20, Feature::nx);
		enable(extended_proc_info_edx, 26, Feature::pdpe1gb);
		enable(extended_proc_info_edx, 27, Feature::rdtscp);
		enable(extended_proc_info_ecx, 3, Feature::extapic);
		enable(paging_features_ebx, 3, Feature::invlpgb);
	}

	initializer
}
