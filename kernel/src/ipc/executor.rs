use core::pin::Pin;

#[doc(hidden)]
pub fn spawn(_task: Pin<Box<dyn Future<Output = ()> + 'static>>) {
	unimplemented!("no longer supported");
}
