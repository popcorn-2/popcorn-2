use gpt::partition_types::{OperatingSystem, Type};
use uuid::Uuid;

use crate::cargo_env;

pub fn system() -> Type {
	const AMD64: Type = Type { guid: Uuid::from_u128(0x8A6CC16CD11046F1813F0382046342C8), os: OperatingSystem::None };

	match &*cargo_env!("CARGO_CFG_TARGET_ARCH") {
		"x86_64" => AMD64,
		_ => panic!("Unsupported architecture")
	}
}
