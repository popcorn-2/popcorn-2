//! An async multi-producer, single-consumer channel.
//!
//! A new channel can be created by calling [`unbounded()`] which returns a [`Sender`] and [`Receiver`].
//! The [`Sender`] can be cloned, while the [`Receiver`] cannot.
//!
//! New items can be pushed to the back of the queue with [`Sender::push()`], and items can be popped
//! with [`Receiver::pop()`] or [`Receiver::try_pop()`].
//!
//! # Examples
//!
//! ```
//! use kernel_api::channel;
//! # fn spawn_task<F: Future<Output = ()>>(f: impl FnOnce() -> F) {
//! #    let fut = f();
//! #    let fut = core::pin::pin!(fut);
//! #    let mut ctx = core::task::Context::from_waker(core::task::Waker::noop());
//! #    while fut.poll(&mut ctx).is_pending() {}
//! # }
//!
//! // create a channel to send integers
//! let (sender, receiver) = channel::unbounded::<u32>();
//!
//! // send some integers to the other end of the channel
//! sender.push(1);
//! sender.push(2);
//! sender.push(3);
//!
//! // spawn a new async task and receive the integers
//! spawn_task(async move || {
//!     assert_eq!(receiver.pop().await, 1);
//!     assert_eq!(receiver.pop().await, 2);
//!
//!     // we can also receive without blocking
//!     assert_eq!(receiver.try_pop(), Some(3));
//! });
//! ```

use alloc::sync::Arc;
use core::fmt::{Debug, Formatter};
use core::pin::Pin;
use core::task::{Context, Poll};
use crossbeam_queue::SegQueue;
use futures::task::AtomicWaker;

/// The sending end of a channel.
///
/// This can be created either by creating a new channel with [`unbounded()`], calling [`Clone::clone()`]
/// on an existing `Sender`, or creating a new `Sender` for an existing [`Receiver`] with [`sender()`](Receiver::sender).
///
/// # Examples
///
/// ```
/// use kernel_api::channel;
/// # fn spawn_thread(f: impl FnOnce()) { f() }
///
/// // create a new channel
/// let (sender1, receiver) = channel::unbounded();
///
/// // send some values through the channel
/// sender1.push(1);
/// sender1.push(2);
///
/// // we can also clone the sender to be able to send from multiple threads
/// let sender2 = sender1.clone();
/// spawn_thread(move || {
///     sender2.push(3);
///     sender2.push(4);
/// });
///
/// // all values will end up at the receiver
/// assert_eq!(receiver.len(), 4);
/// ```
pub struct Sender<T> {
	inner: Arc<ChannelInner<T>>,
}

impl<T> Clone for Sender<T> {
	fn clone(&self) -> Self {
		Self { inner: Arc::clone(&self.inner) }
	}
}

/// The receiving end of a channel.
///
/// This allows a thread to block waiting until one of the associated senders pushes a value
/// into the channel.
///
/// # Examples
///
/// ```
/// use kernel_api::channel;
///
/// // create a channel
/// let (sender, receiver) = channel::unbounded();
///
/// // send some values into the channel
/// sender.push(1);
/// sender.push(2);
/// sender.push(3);
///
/// // check how many items there are
/// let count = receiver.len();
/// info!("there are {count} items waiting to be received");
///
/// // print the items in the channel
/// while let Some(value) = receiver.try_pop() {
///     info!("received {value} from channel");
/// }
/// ```
pub struct Receiver<T> {
	inner: Arc<ChannelInner<T>>,
}

struct ChannelInner<T> {
	queue: SegQueue<T>,
	waker: AtomicWaker,
}

/// Creates a new unbounded channel.
///
/// ```
/// use kernel_api::channel;
///
/// let (sender, receiver) = channel::unbounded();
/// ```
#[must_use]
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
	let inner = ChannelInner {
		queue: SegQueue::new(),
		waker: AtomicWaker::new(),
	};
	let inner = Arc::new(inner);

	(Sender { inner: Arc::clone(&inner) }, Receiver { inner })
}

impl<T> Sender<T> {
	/// Pushes a new item into the back of the channel.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::channel;
	///
	/// let (sender, receiver) = channel::unbounded();
	///
	/// sender.push(5);
	/// assert_eq!(receiver.try_pop(), Some(5));
	/// ```
	pub fn push(&self, val: T) {
		self.inner.queue.push(val);
		self.inner.waker.wake();
	}
}

impl<T> Receiver<T> {
	/// Pushes a new item into the back of the channel.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::channel;
	///
	/// let (_, receiver) = channel::unbounded();
	///
	/// receiver.push(5);
	/// assert_eq!(receiver.try_pop(), Some(5));
	/// ```
	pub fn push(&self, val: T) {
		self.inner.queue.push(val);
		self.inner.waker.wake();
	}

	/// Pops the front item from the channel, blocking until an item is available.
	///
	/// # Examples
	///
	/// ```
	/// # async fn inner() {
	/// use kernel_api::channel;
	///
	/// let (sender, receiver) = channel::unbounded();
	///
	/// sender.push(5);
	/// let val = receiver.pop().await;
	/// assert_eq!(val, 5);
	/// # }
	/// # let fut = core::pin::pin!(inner());
	/// # let mut ctx = core::task::Context::from_waker(core::task::Waker::noop());
	/// # while let core::task:::Poll::Pending = fut.poll(&mut ctx) {}
	/// ```
	pub fn pop(&self) -> impl Future<Output = T> + '_ {
		struct Waiter<'a, T>(&'a ChannelInner<T>);

		impl<T> Future for Waiter<'_, T> {
			type Output = T;

			fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
				if let Some(val) = self.0.queue.pop() { return Poll::Ready(val); }
				self.0.waker.register(cx.waker());
				self.0.queue.pop()
					.map_or(Poll::Pending, Poll::Ready)
			}
		}

		Waiter(&self.inner)
	}

	/// Pops the front item in the channel, if one exists.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::channel;
	///
	/// let (sender, receiver) = channel::unbounded();
	///
	/// assert_eq!(receiver.try_pop(), None);
	/// sender.push(5);
	/// assert_eq!(receiver.try_pop(), Some(5));
	/// assert_eq!(receiver.try_pop(), None);
	/// ```
	#[must_use]
	pub fn try_pop(&self) -> Option<T> {
		self.inner.queue.pop()
	}

	/// Creates a new [`Sender`] for this Receiver.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::channel;
	///
	/// let (_, receiver) = channel::unbounded();
	/// let sender = receiver.sender();
	/// sender.push(5);
	/// assert_eq!(receiver.try_pop(), Some(5));
	/// ```
	#[must_use]
	pub fn sender(&self) -> Sender<T> {
		Sender {
			inner: Arc::clone(&self.inner)
		}
	}

	/// The number of items available to pop from the channel.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::channel;
	///
	/// let (sender, receiver) = channel::unbounded();
	///
	/// assert_eq!(receiver.len(), 0);
	/// sender.push(5);
	/// sender.push(1);
	/// assert_eq!(receiver.len(), 2);
	/// receiver.try_pop().unwrap();
	/// assert_eq!(receiver.len(), 1);
	/// ```
	#[must_use]
	pub fn len(&self) -> usize { self.inner.queue.len() }

	/// Returns `true` if there are no items in the channel.
	///
	/// # Examples
	///
	/// ```
	/// use kernel_api::channel;
	///
	/// let (sender, receiver) = channel::unbounded();
	///
	/// assert_eq!(receiver.is_empty(), true);
	/// sender.push(5);
	/// assert_eq!(receiver.is_empty(), false);
	/// ```
	#[must_use]
	pub fn is_empty(&self) -> bool { self.inner.queue.is_empty() }
}

impl<T> Debug for Sender<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "Sender {{ .. }}")
	}
}

impl<T> Debug for Receiver<T> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		write!(f, "Receiver {{ .. }}")
	}
}
