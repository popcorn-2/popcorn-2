#!/usr/bin/env python3

import argparse
import os
import subprocess
import sys
import json

parser = argparse.ArgumentParser("pop.py")

parser.add_argument(
    choices=["build", "run", "test", "clean"],
    dest="subcommand"
)
parser.add_argument("-v", "--verbose", action='count', default=0)
parser.add_argument("--arch", choices=["x86_64", "host"], default="host")
parser.add_argument("-j", "--jobs", action="store", type=int)
parser.add_argument("--release", action="store_true")
parser.add_argument("--accel", choices=["none", "kvm", "hvf"], default="none")

args, subcommand_parse = parser.parse_known_args()

cargo_flags = []

if args.verbose >= 2:
    cargo_flags.append("--verbose")

if args.jobs:
    cargo_flags.extend(["--jobs", str(args.jobs)])

target_inner = "debug"

if args.release and args.subcommand != "clean":
    cargo_flags.append("--release")
    target_inner = "release"


def run_cargo_command(subcommand: str, *cargo_args: [str], env: dict[str, str] | None = None):
    if env is None:
        env = {}

    command = [
        "cargo",
        subcommand,
        "--message-format=json",
        *cargo_flags,
        *cargo_args,
    ]
    if args.verbose >= 1:
        print(env, " ".join(command), file=sys.stderr)

    result = subprocess.run(command, env={**os.environ, **env}, capture_output=True, text=True)
    if result.returncode != 0:
        for line in result.stdout.strip().split("\n"):
            data = json.loads(line.strip())
            if data["reason"] == "compiler-message":
                if data["message"]["rendered"] is not None:
                    ty, message = data["message"]["rendered"].split(":", 1)
                    color = "\033[31m" if data["message"]["level"] == "error" else "\033[33m" if data["message"]["level"] == "warning" else ""
                    print(f"{color}{ty}\033[0m:{message}")
                else:
                    print(data["message"], data["spans"])
        raise RuntimeError("cargo failed")

    for line in reversed(result.stdout.strip().split("\n")):
        data = json.loads(line.strip())
        if data["reason"] == "compiler-artifact":
            return (data["filenames"][0], result)

    return ("", result)


def generate_iso(kernel_file: str, bootloader_file: str, driver_file: str, output_dir: str):
    result = subprocess.run([
        "cargo",
        "run",
        "-p", "builder",
    ], env={
        **os.environ,
        "CARGO_BIN_FILE_KERNEL": kernel_file,
        "CARGO_BIN_FILE_BOOTLOADER": bootloader_file,
        "CARGO_BIN_FILE_POPFS_popfs_uefi_driver": driver_file,
        "CARGO_CFG_TARGET_ARCH": "x86_64",
        "OUT_DIR": output_dir,
    })

    if result.returncode != 0:
        sys.exit("iso generation failed")


def run_qemu(iso: str, *qemu_args: [str], capture_output: bool) -> tuple[int,str]:
    command = [
                "qemu-system-x86_64",
                "-drive", "if=pflash,format=raw,readonly=on,file=OVMF_CODE.fd",
                "-drive", "if=pflash,format=raw,file=OVMF_VARS.fd",
                "-drive", f"format=raw,file={iso}",
                "--no-reboot",
                "-serial", "stdio",
                *qemu_args,
                *(["--accel", args.accel] if args.accel != "none" else [])
            ]
    if args.verbose >= 1:
        print(" ".join(command), file=sys.stderr)

    result = subprocess.run(command, capture_output=capture_output, text=capture_output)
    return result.returncode, result.stdout


def build(kernel_file: str | None = None, kernel_cargo_flags = None, kernel_build_env: dict[str, str] | None = None):
    if kernel_cargo_flags is None:
        kernel_cargo_flags = []
    if kernel_build_env is None:
        kernel_build_env = {}

    _, result = run_cargo_command(
        "build",
        "-p", "bootloader",
        "--target", "x86_64-unknown-uefi"
    )

    if result.returncode != 0:
        sys.exit("Bootloader build failed")

    if kernel_file is None:
        file, result = run_cargo_command(
            "rustc",
            "-p", "kernel",
            "--target", "x86_64-unknown-popcorn.json",
            "-Zbuild-std=compiler_builtins,core,alloc", "-Zbuild-std-features=compiler-builtins-mem",
            *kernel_cargo_flags,
            "--",
            "-C", "link-args=-export-dynamic",
            "-Z", "export-executable-symbols=on",
            "-C", "relocation-model=static",
            "-C", "symbol-mangling-version=v0",
            "-C", "panic=unwind",
            "-C", "link-args=-Tkernel_hal/src/arch/amd64/linker.ld",
            env=kernel_build_env
        )

        if result.returncode != 0:
            sys.exit("Kernel build failed")

        kernel_file = file

    _, result = run_cargo_command(
        "rustc",
        "-p", "popfs",
        "--bin", "popfs_uefi_driver",
        "--target", "x86_64-unknown-uefi",
        "--",
        "-Z", "pre-link-args=/subsystem:efi_boot_service_driver",
    )

    if result.returncode != 0:
        sys.exit("popfs build failed")

    generate_iso(kernel_file, f"target/x86_64-unknown-uefi/{target_inner}/bootloader.efi", f"target/x86_64-unknown-uefi/{target_inner}/popfs_uefi_driver.efi", f"target/{target_inner}")


match args.subcommand:
    case "build":
        parser_build = argparse.ArgumentParser("pop.py build")
        parser_build.add_argument("--from-kernel-file")
        args = parser_build.parse_args(subcommand_parse, args)

        build(args.from_kernel_file)
    case "run":
        build()
        exit(run_qemu(f"target/{target_inner}/popcorn2.iso"))

    case "clean":
        result = run_cargo_command("clean")
        exit(result.returncode)
    case "test":
        parser_test = argparse.ArgumentParser("pop.py test")
        parser_test.add_argument("--coverage")
        parser_test.add_argument("--junit")
        args = parser_test.parse_args(subcommand_parse, args)

        rustc_coverage_env = {"RUSTFLAGS": "-Cinstrument-coverage -Zno-profiler-runtime"} if args.coverage else {}
        qemu_coverage_args = ["-debugcon", f"file:{args.coverage}"] if args.coverage else []
        kernel_cargo_flags = ["--profile=test"]

        if args.junit is not None:
            kernel_cargo_flags.extend(["--features", "junit_test_out"])

        build(kernel_cargo_flags=kernel_cargo_flags, kernel_build_env=rustc_coverage_env)
        code, stdout = run_qemu(f"target/{target_inner}/popcorn2.iso", "-display", "none", "-device", "isa-debug-exit,iobase=0xf4,iosize=0x04", *qemu_coverage_args, capture_output=(args.junit is not None))
        if args.junit is not None:
            with open(args.junit, "w") as f:
                f.write(stdout.split("Hello world!")[1])
        if code == 1:
            sys.exit("Tests failed")
        elif code == 33:
            exit(0)
        else:
            exit(code)

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

RUSTFLAGS="-Cinstrument-coverage -Zno-profiler-runtime" cargo rustc --profile=test --message-format=json -p kernel --target x86_64-unknown-popcorn.json -Zbuild-std=compiler_builtins,core,alloc -Zbuild-std-features=compiler-builtins-mem -- -C link-args=-export-dynamic -Z export-executable-symbols=on -C relocation-model=static -C symbol-mangling-version=v0 -C panic=unwind -C link-args=-Tkernel/src/arch/amd64/linker.ld
 qemu-system-x86_64 -drive if=pflash,format=raw,readonly=on,file=OVMF_CODE.fd -drive if=pflash,format=raw,file=OVMF_VARS.fd -drive format=raw,file=target/debug/popcorn2.iso --no-reboot -serial stdio -device VGA,edid=on,xres=1280,yres=800 -accel hvf "-device" "isa-debug-exit,iobase=0xf4,iosize=0x04" -debugcon file:coverage.profraw
'''
