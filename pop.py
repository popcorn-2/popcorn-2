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
parser.add_argument("--arch", choices=["x86_64", "host"], default="host")
parser.add_argument("-j", "--jobs", action="store", type=int)

args, subcommand_parse = parser.parse_known_args()

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


match args.subcommand:
    case "build":
        parser_build = argparse.ArgumentParser("pop.py build")
        parser_build.add_argument("--release", action="store_true")
        parser_build.add_argument("--from-kernel-file")
        args = parser_build.parse_args(subcommand_parse, args)

        if args.release:
            cargo_flags.append("--release")

        result = run_cargo_command(
            "build",
            "-p", "bootloader",
            "--target", "x86_64-unknown-uefi"
        )

        if result.returncode != 0:
            sys.exit("Bootloader build failed")

        if not args.from_kernel_file:
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

            kernel_file = "target/x86_64-unknown-popcorn/debug/kernel.exec"
        else:
            kernel_file = args.from_kernel_file

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
            "CARGO_BIN_FILE_KERNEL": kernel_file,
            "CARGO_BIN_FILE_BOOTLOADER": "target/x86_64-unknown-uefi/debug/bootloader.efi",
            "CARGO_BIN_FILE_POPFS_popfs_uefi_driver": "target/x86_64-unknown-uefi/debug/popfs_uefi_driver.efi",
            "CARGO_CFG_TARGET_ARCH": "x86_64",
            "OUT_DIR": "target/debug",
        })

        if result.returncode != 0:
            sys.exit("iso generation failed")

    case "run":
        raise RuntimeError("Command `run` not yet supported")
    case "clean":
        result = run_cargo_command("clean")
        exit(result.returncode)
    case "test":
        raise RuntimeError("Command `test` not yet supported")

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
