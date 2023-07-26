use macros::module_init;

#[module_init]
extern "C" fn baz() -> bool {
	todo!()
}
