use core::fmt;
use core::fmt::Formatter;

pub struct Args {

}

impl Args {
	pub fn try_parse_args(input: &str) -> Result<Args,Error> {
		todo!()
	}
}

pub struct Error;

impl fmt::Display for Error {
	fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
		todo!()
	}
}
