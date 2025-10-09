use alloc::boxed::Box;
use hashbrown::HashMap;
use log::debug;
use crate::hashmap_new;
use crate::ipc::{Error, protocol};
use crate::ipc::server::Server;

pub struct ProtocolVisitor<U: ?Sized> {
	table: HashMap<u128, fn(&mut U, *const u8) -> Result<(), Error>>,
}

impl<U: ?Sized> ProtocolVisitor<U> {
	pub const fn new() -> Self { Self { table: hashmap_new!() } }

	pub fn add_visitor<T: protocol::Protocol + ?Sized>(mut self, f: fn(&mut U, &T::Ctor) -> Result<(), Error>) -> Self {
		self.table.insert(T::UID, unsafe { core::mem::transmute(f) });
		self
	}
}

pub struct CtorArgs<'a> {
	uids: &'a [u128],
}

impl<'a> CtorArgs<'a> {
	pub fn process_with<S: Server>(&mut self, s: &S, context: &mut S::CtorContext) -> Result<(), Error> {
		let _table = s.dispatch_table();
		for &uid in self.uids {
			// todo: pull ctor layout out of protocol meta
			// let deserialize = table.ctor_deserialize(uid)?;
			debug!("deserialize for {uid:#x}");// @ {deserialize:#p}");
			let ctor = Box::<[u8]>::from([]); //deserialize(&mut self.args)?;
			let f = context.visitors().table.get(&uid).ok_or(Error::UnsupportedProtocol)?;
			f(&mut *context, ctor.as_ptr())?;
		}
		Ok(())
	}
	
	pub fn new(uids: &'a [u128]) -> Self {
		Self { uids }
	}
	
	pub fn uids(&self) -> &[u128] { self.uids }
}

pub trait CtorContext where Self: 'static + Default {
	fn visitors(&self) -> &'static ProtocolVisitor<Self>;
}
