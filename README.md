(Italics represents currently unsupported)

# Disk structure

All partitions are expected to be on the same disk (including the EFI system partition). This may change in future.

## EFI system partition

**GUID**: `C12A7328-F81F-11D2-BA4B-00A0C93EC93B`

All popcorn related files shall go in the `/EFI/POPCORN` directory. This may contain the μwave EFI application, named `UWAVEX.efi` where `X` is the UEFI short name for the machine, for example `UWAVEx64.efi` on amd64, or `UWAVEAA64.efi` on AArch64. This file may alternatively be stored in the `/EFI/BOOT` directory, named `BOOTX.efi` where `X` is treated as before.

The `/EFI/POPCORN` directory shall also contain a file named either `config.toml` *or `configX.toml` (where `X` is treated as before). Where both are present, the machine specific values override the values in `config.toml`.* See [bootloader configuation](#bootloader) for information on the format.

The `/EFI/POPCORN` directory may also contain other files as referenced by the configuration file.

## System partition

**Format**: ~~PopcornFS~~ FAT32

### GUIDs

- amd64: `8A6CC16C-D110-46F1-813F-0382046342C8`

### Structure

This shall contain a folder in root directory named `kernel`.

**Currently versioning is not supported, so all files listed below ending in a version string shall have no suffix, for example `kernel-X.exec` is instead just `kernel.exec`.**

> Is this the best way to do versioning of different kernels? Or should it take into account potentially different non-kernel package versions somehow?

This shall contain one or more files named `kernel-X.exec`, where `X` is a version string. This is the kernel executable. The `kernel` folder may also contain a file named `kernel-X.map` where `X` is the version string of the corresponding kernel, for example `kernel-0.3.2.exec`/`kernel-0.3.2.map`. This contains a list of kernel symbols as output by the `nm` utility in BSD format. This file shall be sorted in ascending order of address, and the symbols may be demangled. There shall also be a file named `config-X.toml`, with the `X` matching the version string for the corresponding kernel. See [kernel configuration](#kernel) for information on the format.

# Configuration

## Bootloader

## Kernel

# Boot procedure

## Preboot stage

The bootloader for the specific machine is loaded by the UEFI firmware. It then attempts to initialise a UART connection for debugging, as well as the GPU, mouse and keyboard. It also attempts to read EDID information from the connected display to find the native resolution and DPI.

Once hardware has been initialised, the bootloader may display a UI to allow the user to choose a configuration (eg. specific kernel version), or instead may immediately start loading the kernel. *The user may hold the `alt` key to force display of boot options, and the `V` key to display verbose log output.*

> What log levels should be displayed in the output? Currently levels below `info` are removed at build time from release builds, and levels below `debug` from debug builds.

> Should `error` and `warn` levels be displayed regardless of verbose mode?

The bootloader then locates the first partition on the disk that the EFI system partition is on, and uses the first partition with a GUID matching the [System partition](#system-partition) as the System partition. The kernel image and modules are loaded from this partition. The bootloader creates a stack for the kernel and requests a memory map from the firmware, before handing control of the system to the kernel.

## Kernel initialisation

---

# Porting

Create a new module inside `kernel_hal::arch`, and gate it to the correct target with a `#[cfg]` attribute. Create a ZST to contain the root HAL functions, and add a `#[derive(kernel_hal::Hal)]` to it. Then manually implement all required items in `kernel_hal::Hal` for the ZST.

For example
```rust
// kernel_hal/src/arch/mod.rs

#[cfg(target_arch = "x86_64")]
mod amd64;

// kernel_hal/src/arch/amd64.rs

use kernel_hal::Hal;

#[derive(Hal)]
struct Amd64Hal;

unsafe impl Hal for Amd64Hal {
    fn breakpoint() {
        unsafe { core::arch::asm!("int3"); }
    }
    
    // ...
}

```

---

# Design notes that should go somewhere else

- **TODO**: On amd64, just before handoff, the `fs` register is initialised to the value of `0xdead101ca1` ("dead local"). If a page fault occurs in the <**TODO**> bytes of memory below this address, a debug note is printed that this may be due to use of core locals before initialisation. Once the TLS section for the bootstrap processor is initialised, the `fs` register is updated to point at the end of the actual TLS section.
  > Can we pass off the actual TLS size from the handover process to the page fault handler rather than guessing the size of the checked region?

---

# Popcorn partition GUIDs
- `8A6CC16C-D110-46F1-813F-0382046342C8`
- `B1D8F0F9-05CB-42E1-A591-A6980E7B5909`
- `110DF7AB-1916-4544-B1B2-D2E9C2B911B7`
- `3C4B5291-68BE-48F4-8E70-92EA6F57A7B1`
- `D6D0DCF4-0E35-461F-B14E-C5C6110F6D45`
- `685DC97E-289A-459E-889A-2DCB2F1EB3E2`
- `316DE66A-20E8-4051-AAC0-847C733789C6`
- `61B8C9A1-6230-410E-941A-0EFF6D849D92`
- `B64A1767-9CE7-4C8D-BBBC-4B77B72C2956`
- `E7441BC3-9451-438A-9294-293508058F5C`
- `CD57C1F6-CD72-43FC-99C3-A00245C9F61E`
- `F05FEA17-568C-43B9-A2BB-A7B72075F302`
- `2CABC03F-4369-44F4-A5D0-585DA66BB7D4`
- `B5B907E8-B0E4-4619-9A4D-3635E05FBEA7`
- `057F1D0A-E8A3-4C3C-A11D-7F14BC2AF14D`
- `F4032BCB-8268-481B-895E-E34F5974EC62`
- `2FE337A1-1F87-4D70-97C1-1C3B59D86D22`
- `9670D514-9A9B-420C-98D7-5F096C46B629`
- `9B0D37BB-FDAF-4363-915D-6406E8C44AA6`
- `2CB8AE15-DF19-491B-8007-A63CE32593C3`
- `E3F1111E-CC98-401C-8482-6869FB1B3CAC`
- `F9B14E9F-53B5-4A7C-B533-C1D67F645D8C`
- `5D903611-8BDE-498C-A83C-370D3850D51B`
- `3FBE0F7F-BC3F-4B27-820E-08B61854738B`
- `580FCF85-16E2-4FCE-B0B9-BEA08F619524`
