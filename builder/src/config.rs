use derive_more::Display;
use gpt::partition_types;
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

#[derive(Deserialize, Debug)]
pub struct Config {
	#[serde(rename = "partition")]
	pub partitions: Vec<Partition>
}

#[derive(Deserialize, Debug)]
pub struct Partition {
	#[serde(rename = "type")]
	pub part_type: PartitionType,
	pub size: u64,
	pub name: Option<String>
}

#[derive(Deserialize, Debug, Eq, PartialEq, Copy, Clone)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum PartitionType {
	Efi,
	System,
	Custom
}

impl PartitionType {
	pub fn default_name(self) -> &'static str {
		match self {
			PartitionType::Efi => "EFI",
			PartitionType::System => "Popcorn System",
			PartitionType::Custom => "Custom"
		}
	}
}

impl From<PartitionType> for partition_types::Type {
	fn from(value: PartitionType) -> Self {
		match value {
			PartitionType::Efi => partition_types::EFI,
			PartitionType::System => crate::partitions::system(),
			PartitionType::Custom => partition_types::BASIC
		}
	}
}

impl Config {
	pub fn parse() -> Result<Self, Error> {
		//let manifest_dir = PathBuf::from(cargo_env!("CARGO_MANIFEST_DIR"));
		let config_path = PathBuf::from("config.toml"); //manifest_dir.join("config.toml");

		let s = fs::read_to_string(config_path)
				.map_err(|_| Error::FileNotFound)?;

		let config: Config = toml::from_str(&s).map_err(|e| Error::ParseError(e))?;

		if config.partitions.iter()
				.filter(|part| part.part_type == PartitionType::Efi)
				.collect::<Vec<_>>()
				.len() != 1 {
			return Err(Error::NoEfiPartition);
		}

		if config.partitions.iter()
		         .filter(|part| part.part_type == PartitionType::System)
		         .collect::<Vec<_>>()
		         .len() != 1 {
			return Err(Error::NoSystemPartition);
		}

		Ok(config)
	}
}

#[derive(Display)]
pub enum Error {
	#[display(fmt = "`config.toml` not found")]
	FileNotFound,
	ParseError(toml::de::Error),
	NoSystemPartition,
	NoEfiPartition
}
