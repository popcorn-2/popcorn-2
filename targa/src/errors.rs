use derive_more::Display;

#[derive(Display, Debug, Copy, Clone, Eq, PartialEq)]
pub enum ParsingError {
    /// The buffer is too short to contain a valid Targa header
    NoHeader,
    /// The buffer is too short to contain data for the resolution listed in the header
    NotEnoughData,
    /// The image uses an unsupported color encoding
    Unsupported
}
