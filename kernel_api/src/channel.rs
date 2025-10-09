//! An async multi-producer, single-consumer channel
//!
//! A new channel can be created by calling [`unbounded()`] which returns a [`Sender`] and [`Receiver`].
//! The [`Sender`] can be [`clone`]d, while the [`Receiver`] cannot.
//!
//! New items can be pushed to the back of the queue with [`Sender::push()`], and items can be popped
//! with [`Receiver::pop()`] or [`Receiver::try_pop()`].

use alloc::sync::Arc;
use core::fmt::{Debug, Formatter};
use core::pin::Pin;
use core::task::{Context, Poll};
use crossbeam_queue::SegQueue;
use futures::task::AtomicWaker;

/// The sending end of a channel
pub struct Sender<T> {
	inner: Arc<ChannelInner<T>>,
}

impl<T> Clone for Sender<T> {
	fn clone(&self) -> Self {
		Self { inner: Arc::clone(&self.inner) }
	}
}

/// The receiving end of a channel
pub struct Receiver<T> {
	inner: Arc<ChannelInner<T>>,
}

struct ChannelInner<T> {
	queue: SegQueue<T>,
	waker: AtomicWaker,
}

/// Creates a new unbounded chanel
pub fn unbounded<T>() -> (Sender<T>, Receiver<T>) {
	let inner = ChannelInner {
		queue: SegQueue::new(),
		waker: AtomicWaker::new(),
	};
	let inner = Arc::new(inner);

	(Sender { inner: Arc::clone(&inner) }, Receiver { inner })
}

impl<T> Sender<T> {
	/// Pushes a new item into the back of the channel
	pub fn push(&self, val: T) {
		self.inner.queue.push(val);
		self.inner.waker.wake();
	}
}

impl<T> Receiver<T> {
	/// Pushes a new item into the back of the channel
	pub fn push(&self, val: T) {
		self.inner.queue.push(val);
		self.inner.waker.wake();
	}

	/// Pops the front item from the channel, blocking until an item is available
	pub fn pop(&self) -> impl Future<Output = T> + '_ {
		struct Waiter<'a, T>(&'a ChannelInner<T>);

		impl<T> Future for Waiter<'_, T> {
			type Output = T;

			fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
				if let Some(val) = self.0.queue.pop() { return Poll::Ready(val); }
				self.0.waker.register(cx.waker());
				match self.0.queue.pop() {
					Some(val) => Poll::Ready(val),
					None => Poll::Pending,
				}
			}
		}

		Waiter(&self.inner)
	}

	/// Pops the front item in the channel, if one exists
	pub fn try_pop(&self) -> Option<T> {
		self.inner.queue.pop()
	}

	/// Creates a new [`Sender`] for this Receiver
	pub fn sender(&self) -> Sender<T> {
		Sender {
			inner: Arc::clone(&self.inner)
		}
	}

	/// The number of items available to pop from the channel
	pub fn len(&self) -> usize { self.inner.queue.len() }
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
