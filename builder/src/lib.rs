#![feature(decl_macro)]

use std::{io, path::PathBuf};
use std::fmt::Debug;
use std::fs::File;
use std::io::{BufReader, Read, Seek, Write};

use fatfs::{FatType, FileSystem, format_volume, FormatVolumeOptions, FsOptions, ReadWriteSeek};
use fscommon::StreamSlice;
use gpt::GptConfig;
use gpt::disk::LogicalBlockSize;
use gpt::mbr::ProtectiveMBR;
use gpt::partition_types::Type;

use crate::config::{Config, PartitionType};

mod config;
mod partitions;

macro cargo_env($key:literal) {
    ::std::env::var($key).unwrap()
}

pub fn main() {
    let config = match Config::parse() {
        Ok(config) => config,
        Err(e) => panic!("Failed to parse config: {e}")
    };

    let mut kernel = {
        let kernel_path = cargo_env!("CARGO_BIN_FILE_KERNEL");
        println!("cargo:warning={kernel_path}");
        let f = File::open(kernel_path).expect("kernel file does not exist");
        BufReader::new(f)
    };

    let mut bootloader = {
        let bootloader_path = cargo_env!("CARGO_BIN_FILE_BOOTLOADER");
        println!("cargo:warning={bootloader_path}");
        let f = File::open(bootloader_path).expect("bootloader file does not exist");
        BufReader::new(f)
    };

    let mut popfs_driver = {
        let popfs_driver_path = cargo_env!("CARGO_BIN_FILE_POPFS_popfs_uefi_driver");
        let f = File::open(popfs_driver_path).expect("popfs driver file does not exist");
        BufReader::new(f)
    };

    let disk_image_path = {
        let out_dir = PathBuf::from(cargo_env!("OUT_DIR"));
        out_dir.join("popcorn2.iso")
    };

    let mut disk_image = File::options()
            .create(true)
            .read(true)
            .write(true)
            .truncate(true)
            .open(&disk_image_path)
            .expect("Could not create disk image");

    let partition_sizes = config.partitions.iter().fold(0, |total, partition| total + partition.size);
    format_disk_image(&mut disk_image, partition_sizes);

    for partition in &config.partitions {
        let name = partition.name.as_deref()
                            .unwrap_or(partition.part_type.default_name());

        let fs = create_fat_partition(
            &mut disk_image,
            partition.size,
            name,
            partition.part_type.into()
        );

        match partition.part_type {
            PartitionType::Efi => init_efi_partition(fs, &mut bootloader, &mut popfs_driver),
            PartitionType::System => init_system_partition(fs, &mut kernel),
            _ => {}
        }
    }

    println!("cargo:rustc-env=ISO_IMAGE={}", disk_image_path.display());
}

fn init_efi_partition(fs: FileSystem<impl ReadWriteSeek>, mut bootloader_data: impl Read, mut popfs_data: impl Read) {
    let root_dir = fs.root_dir();
    root_dir.create_dir("efi").unwrap();
    root_dir.create_dir("efi/boot").unwrap();
    let mut bootloader = root_dir.create_file("efi/boot/bootx64.efi").unwrap();
    bootloader.truncate().unwrap();
    io::copy(&mut bootloader_data, &mut bootloader).unwrap();

    root_dir.create_dir("efi/popcorn").unwrap();
    let mut conf = root_dir.create_file("efi/popcorn/config.toml").unwrap();
    let data = r#"
        [fonts]
        default = ""

        [kernel]
        root_disk = "8A6CC16C-D110-46F1-813F-0382046342C8"
        image = "foo"
        modules = []
    "#;
    conf.write(data.as_bytes()).unwrap();

    let mut popfs = root_dir.create_file("efi/popcorn/popfs.efi").unwrap();
    io::copy(&mut popfs_data, &mut popfs).unwrap();
}

fn init_system_partition(fs: FileSystem<impl ReadWriteSeek>, mut kernel_data: impl Read) {
    let root_dir = fs.root_dir();
    root_dir.create_dir("kernel").unwrap();
    let mut kernel = root_dir.create_file("kernel/kernel.exec").unwrap();
    kernel.truncate().unwrap();
    io::copy(&mut kernel_data, &mut kernel).unwrap();
}

fn create_fat_partition<T: Read + Write + Seek + Debug>(mut disk: T, size: u64, name: &str, part_type: Type) -> FileSystem<impl ReadWriteSeek> {
    let mut gdisk = GptConfig::new()
            .writable(true)
            .logical_block_size(LogicalBlockSize::Lb512)
            .open_from_device(Box::new(&mut disk))
            .expect("Failed to open GPT disk");

    let partition_id = gdisk.add_partition(name, size, part_type, 0, None)
                            .expect("Unable to create partition");

    let partition = gdisk.partitions().get(&partition_id).unwrap();

    let start_offset = partition.bytes_start(LogicalBlockSize::Lb512).unwrap();
    let end_offset = start_offset + partition.bytes_len(LogicalBlockSize::Lb512).unwrap();

    gdisk.write().expect("Unable to write partition table");

    let mut partition = StreamSlice::new(disk, start_offset, end_offset).unwrap();

    let format_options = FormatVolumeOptions::new()
            .fat_type(FatType::Fat32);
    format_volume(&mut partition, format_options).unwrap();

    FileSystem::new(partition, FsOptions::new()).unwrap()
}

fn format_disk_image(mut disk_image: &mut File, total_partition_sizes: u64) {
    let disk_size = total_partition_sizes + 1024 * 64;
    disk_image.set_len(disk_size)
              .expect("Unable to set disk size");

    let mbr = ProtectiveMBR::with_lb_size(
        u32::try_from((disk_size / 512) - 1).unwrap_or(0xFF_FF_FF_FF)
    );
    mbr.overwrite_lba0(disk_image).expect("Failed to write MBR");

    let mut gdisk = GptConfig::new()
            .writable(true)
            .logical_block_size(LogicalBlockSize::Lb512)
            .create_from_device(Box::new(&mut disk_image), None)
            .expect("Failed to create GPT disk");
    gdisk.update_partitions(Default::default()).expect("Unable to write GPT partition table");

    gdisk.write().expect("Unable to write disk image");
}
