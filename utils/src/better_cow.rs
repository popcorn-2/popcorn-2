use alloc::borrow::ToOwned;
use core::borrow::Borrow;
use core::cmp::Ordering;
use core::fmt::{Debug, Display, Formatter};
use core::hash::{Hash, Hasher};
use core::ops::Deref;

pub enum Cow<'a, T, U: ?Sized> {
	Owned(T),
	Borrowed(&'a U),
}

impl<'a, T: Borrow<U>, U: ?Sized> Deref for Cow<'a, T, U> {
	type Target = U;

	fn deref(&self) -> &Self::Target {
		match self {
			Self::Owned(o) => o.borrow(),
			Self::Borrowed(b) => b,
		}
	}
}

impl<'a, T: From<&'a U>, U: ?Sized> Cow<'a, T, U> {
	pub fn to_mut(&mut self) -> &mut T {
		if let Self::Borrowed(b) = self {
			*self = Self::Owned((*b).into());
		}

		match self {
			Self::Owned(o) => o,
			Self::Borrowed(_) => unreachable!(),
		}
	}

	pub fn into_owned(self) -> T {
		match self {
			Self::Owned(o) => o,
			Self::Borrowed(b) => b.into(),
		}
	}
}

impl<'a, T: Clone, U: ?Sized> Clone for Cow<'a, T, U> {
	fn clone(&self) -> Self {
		match self {
			Self::Borrowed(b) => Self::Borrowed(*b),
			Self::Owned(o) => Self::Owned(o.clone()),
		}
	}

	fn clone_from(&mut self, source: &Self) {
		match (self, source) {
			(&mut Self::Owned(ref mut dest), &Self::Owned(ref o)) => o.clone_into(dest),
			(t, s) => *t = s.clone(),
		}
	}
}

impl<'a, T: Copy, U: ?Sized> Copy for Cow<'a, T, U> {}

impl<'a, T: Debug, U: ?Sized + Debug> Debug for Cow<'a, T, U> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::Owned(o) => Debug::fmt(o, f),
			Self::Borrowed(b) => Debug::fmt(*b, f),
		}
	}
}

impl<'a, T: Display, U: ?Sized + Display> Display for Cow<'a, T, U> {
	fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
		match self {
			Self::Owned(o) => Display::fmt(o, f),
			Self::Borrowed(b) => Display::fmt(*b, f),
		}
	}
}

impl<'a, T, U: ?Sized + PartialEq> PartialEq for Cow<'a, T, U> where Self: Deref<Target = U> {
	fn eq(&self, other: &Self) -> bool {
		(**self).eq(other)
	}
}

impl<'a, T, U: ?Sized + Eq> Eq for Cow<'a, T, U> where Self: Deref<Target = U> {}

impl<'a, T, U: ?Sized + PartialOrd> PartialOrd for Cow<'a, T, U> where Self: Deref<Target = U> {
	fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
		(**self).partial_cmp(other)
	}
}

impl<'a, T, U: ?Sized + Ord> Ord for Cow<'a, T, U> where Self: Deref<Target = U> {
	fn cmp(&self, other: &Self) -> Ordering {
		(**self).cmp(other)
	}
}

impl<'a, T, U: ?Sized + Hash> Hash for Cow<'a, T, U> where Self: Deref<Target = U> {
	fn hash<H: Hasher>(&self, state: &mut H) {
		(**self).hash(state)
	}
}

impl<'a, T, U: ?Sized> Borrow<U> for Cow<'a, T, U> where Self: Deref<Target = U> {
	fn borrow(&self) -> &U {
		&**self
	}
}
