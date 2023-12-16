#!/usr/bin/env python3

import os
import subprocess

subprocess.run([
    "cargo",
    "build",
    "-p", "bootloader",
    "--target", "x86_64-unknown-uefi",
])

subprocess.run([
    "cargo",
    "rustc",
    "-p", "kernel",
    "--target", "x86_64-unknown-popcorn.json",
    "-Zbuild-std=compiler_builtins,core,alloc", "-Zbuild-std-features=compiler-builtins-mem",
    "--",
    "-C", "link-args=-export-dynamic",
    "-Z", "export-executable-symbols=on",
    "-C", "relocation-model=static",
    "-C", "symbol-mangling-version=v0",
    "-C", "panic=unwind",
    "-C", "link-args=-Tkernel/src/arch/amd64/linker.ld",
])

subprocess.run([
    "cargo",
    "rustc",
    "-p", "popfs",
    "--bin", "popfs_uefi_driver",
    "--target", "x86_64-unknown-uefi",
    "--",
    "-Z", "pre-link-args=/subsystem:efi_boot_service_driver",
])

subprocess.run([
    "cargo",
    "run",
    "-p", "builder",
], env = {
    **os.environ,
    "CARGO_BIN_FILE_KERNEL": "target/x86_64-unknown-popcorn/debug/kernel.exec",
    "CARGO_BIN_FILE_BOOTLOADER": "target/x86_64-unknown-uefi/debug/bootloader.efi",
    "CARGO_BIN_FILE_POPFS_popfs_uefi_driver": "target/x86_64-unknown-uefi/debug/popfs_uefi_driver.efi",
    "CARGO_CFG_TARGET_ARCH": "x86_64",
    "OUT_DIR": "target/debug",
})

'''
copied from original build system for checking all args are correct when finished

[profile.dev]
panic = 'abort'
rustflags = ["-C", "symbol-mangling-version=v0"]

[profile.release]
panic = 'abort'
rustflags = ["-C", "symbol-mangling-version=v0"]

[profile.dev.package.popfs]
rustflags = [
	"-Z", "pre-link-args=/subsystem:efi_boot_service_driver"
]

[profile.release.package.popfs]
rustflags = [
	"-Z", "pre-link-args=/subsystem:efi_boot_service_driver"
]

[profile.dev.package.kernel]
rustflags = [
	"-C", "panic=unwind",
	"-C", "link-args=-Tkernel/src/arch/amd64/linker.ld",
	"-C", "link-args=-export-dynamic",
	"-Z", "export-executable-symbols=on",
	"-C", "relocation-model=static",
	"-C", "force-frame-pointers=y",
	"-C", "force-unwind-tables=y"
]

[profile.release.package.kernel]
rustflags = [
	"-C", "panic=unwind",
	"-C", "link-args=-Tkernel/src/arch/amd64/linker.ld",
	"-C", "link-args=-export-dynamic",
	"-Z", "export-executable-symbols=on",
	"-C", "relocation-model=static",
	"-C", "force-frame-pointers=y",
	"-C", "force-unwind-tables=y"
]

[profile.test.package.kernel]
rustflags = [
	"-C", "panic=unwind",
	"-C", "link-args=-Tkernel/src/arch/amd64/linker.ld",
	"-C", "link-args=-export-dynamic",
	"-Z", "export-executable-symbols=on",
	"-C", "relocation-model=static",
	"-C", "force-frame-pointers=y",
	"-C", "force-unwind-tables=y",
	"--test"
]
'''
