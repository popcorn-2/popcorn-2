#[allow(unused_imports)] use crate::prelude::*;

#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Debug)]
// Invariant: max value is 0xFFFFFFFF-FFFFFFFF-FFFF-FFFF
pub struct ProtocolId(u128);

#[derive(Copy, Clone, PartialEq, Eq, Ord, PartialOrd, Hash, Debug)]
pub struct MethodId(u32);

/*#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum Method {
	Static(StaticMethod),
	Handled(HandledMethod),
}*/
pub type Method = HandledMethod;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct StaticMethod {
	arg_1: ArgTy,
	arg_2: ArgTy,
	arg_3: ArgTy,
	arg_4: ArgTy,
	ret: ArgTy,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct HandledMethod {
	pub a: ArgTy,
	pub b: ArgPairTy,
	pub ret: ArgTy,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ArgTy {
	Value,
	Object,
	None,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub enum ArgPairTy {
	None,
	Memory,
	String,
	OutMemory,
	OutString,
	Pair(ArgTy, ArgTy),
}

fn proto_method_to_parts(proto_method: u128) -> (ProtocolId, MethodId) {
	const PROTO_MASK: u128 = 0xFFFF_FFFF_FFFF_FFFF_FFFF_FFFF;

	let proto = proto_method & PROTO_MASK;
	let method = (proto_method & !PROTO_MASK) >> PROTO_MASK.trailing_ones();
	(ProtocolId(proto), MethodId(method as u32))
}
