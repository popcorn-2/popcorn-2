#!/usr/bin/env python3

import argparse
import os
import subprocess
import sys

parser = argparse.ArgumentParser("pop.py")

parser.add_argument(
    choices=["build", "run", "test", "clean"],
    dest="subcommand"
)
parser.add_argument("-v", "--verbose", action='count', default=0)
parser.add_argument("--arch", choices=["x86_64"], required=True)
parser.add_argument("-j", "--jobs", action="store", type=int)

args, subcommand_parse = parser.parse_known_args()

match args.subcommand:
    case "build":
        pass
    case "run":
        pass
    case "clean":
        pass

cargo_flags = []

if args.verbose >= 2:
    cargo_flags.append("--verbose")

if args.jobs:
    cargo_flags.extend(["--jobs", str(args.jobs)])

def run_cargo_command(subcommand: str, *cargo_args: [str]):
    command = [
        "cargo",
        subcommand,
        *cargo_flags,
        *cargo_args
    ]
    if args.verbose >= 1:
        print(" ".join(command))

    return subprocess.run(command)


result = run_cargo_command(
    "build",
    "-p", "bootloader",
    "--target", "x86_64-unknown-uefi"
)

if result.returncode != 0:
    sys.exit("Bootloader build failed")

result = run_cargo_command(
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
)

if result.returncode != 0:
    sys.exit("Kernel build failed")

result = run_cargo_command(
    "rustc",
    "-p", "popfs",
    "--bin", "popfs_uefi_driver",
    "--target", "x86_64-unknown-uefi",
    "--",
    "-Z", "pre-link-args=/subsystem:efi_boot_service_driver",
)

if result.returncode != 0:
    sys.exit("popfs build failed")

result = subprocess.run([
    "cargo",
    "run",
    "-p", "builder",
], env={
    **os.environ,
    "CARGO_BIN_FILE_KERNEL": "target/x86_64-unknown-popcorn/debug/kernel.exec",
    "CARGO_BIN_FILE_BOOTLOADER": "target/x86_64-unknown-uefi/debug/bootloader.efi",
    "CARGO_BIN_FILE_POPFS_popfs_uefi_driver": "target/x86_64-unknown-uefi/debug/popfs_uefi_driver.efi",
    "CARGO_CFG_TARGET_ARCH": "x86_64",
    "OUT_DIR": "target/debug",
})

if result.returncode != 0:
    sys.exit("iso generation failed")

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
