pub enum L4 {}
pub enum L3 {}
pub enum L2 {}
pub enum L1 {}

mod private {
	pub trait Sealed {}
	impl Sealed for super::L4 {}
	impl Sealed for super::L3 {}
	impl Sealed for super::L2 {}
	impl Sealed for super::L1 {}
}

pub trait Level: private::Sealed {}

pub trait ParentLevel: Level {
	type Child: Level;
}

impl Level for L4 {}
impl Level for L3 {}
impl Level for L2 {}
impl Level for L1 {}

impl ParentLevel for L4 {
	type Child = L3;
}

impl ParentLevel for L3 {
	type Child = L2;
}

impl ParentLevel for L2 {
	type Child = L1;
}
