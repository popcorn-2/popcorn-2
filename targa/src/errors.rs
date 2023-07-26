use derive_more::Display;

#[derive(Display, Debug, Copy, Clone, Eq, PartialEq)]
pub enum ParsingError {
    NoHeader,
    NotEnoughData,
    Unsupported
}
