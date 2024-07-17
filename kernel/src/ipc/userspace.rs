#[allow(unused_imports)] use crate::prelude::*;
use crossbeam_queue::SegQueue;
use utils::better_cow::Cow;
use crate::threading::tcb::ThreadState;
use crate::ipc::Error;
use crate::ipc::server::Server;
use crate::threading;
use crate::threading::ThreadId;

#[derive(Debug)]
pub(crate) struct UserspaceServer {
	pending_queue: SegQueue<Packet>,
	pid: ThreadId,
	processed_queue: (),
}

impl UserspaceServer {
	pub(crate) fn new_current_thread() -> Self {
		Self {
			pending_queue: SegQueue::new(),
			pid: threading::current_thread().expect("Cannot be caled while idling"),
			processed_queue: (),
		}
	}
}

impl Server for UserspaceServer {
	fn open(&self, endpoint: Cow<'_, Box<str>, str>) -> Result<u16, Error> {
		let endpoint = Box::<[u8]>::from(endpoint.into_owned()); // TODO(syscall-api): NUL terminate the string for better C interop
		let packet = Packet {
			proto_method: 0,
			payload1: PacketHalf::Buffered(endpoint),
			payload2: PacketHalf::None,
		};
		debug!("`UserpsaceServer{{ pid: {:?} }}` add packet `{packet:#?}`", self.pid);
		self.pending_queue.push(packet); // FIXME(panic): fallible OOM
		threading::park(|_| {});
		unimplemented!()
	}
}

#[derive(Debug)]
struct Packet {
	proto_method: u128,
	payload1: PacketHalf,
	payload2: PacketHalf,
}

#[derive(Debug)]
enum PacketHalf {
	DualArgs(usize, usize),
	Buffered(Box<[u8]>), // TODO: is it safe to use `[u8]`
}

impl PacketHalf {
	#[allow(non_upper_case_globals)]
	pub const None: Self = Self::DualArgs(0, 0);
}
