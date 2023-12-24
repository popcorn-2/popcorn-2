pub enum Global {}
pub enum Upper {}
pub enum Middle {}
pub enum Lower {}

mod private {
	pub trait Sealed {}
	impl Sealed for super::Global {}
	impl Sealed for super::Upper {}
	impl Sealed for super::Middle {}
	impl Sealed for super::Lower {}
}

pub trait LevelInternal: private::Sealed {}

pub trait ParentLevel: super::Level {
	type Child: super::Level;
}

impl LevelInternal for Global {}
impl LevelInternal for Upper {}
impl LevelInternal for Middle {}
impl LevelInternal for Lower {}

impl ParentLevel for Global {
	type Child = Upper;
}

impl ParentLevel for Upper {
	type Child = Middle;
}

impl ParentLevel for Middle {
	type Child = Lower;
}
