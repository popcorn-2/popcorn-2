use std::env;
use std::process::Command;
use chrono::{TimeZone, Utc};

fn main() {
	let now = match env::var("SOURCE_DATE_EPOCH") {
		Ok(val) => Utc.timestamp_opt(val.parse::<i64>().unwrap(), 0).unwrap(),
		Err(_) => Utc::now(),
	};
	println!("cargo:rustc-env=BUILD_TIMESTAMP={}", now.to_rfc3339());

	if let Ok(git) = Command::new("git").args(&["rev-parse", "HEAD"]).output() {
		if git.status.success() {
			println!("cargo:rustc-env=GIT_HASH={}", String::from_utf8_lossy(&git.stdout).trim());
		}
	}
}
