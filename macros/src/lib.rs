use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input};

#[proc_macro_attribute]
pub fn test_should_panic(_attr: TokenStream, item: TokenStream) -> TokenStream {
	let func = parse_macro_input!(item as ItemFn);
	let ident = func.sig.ident.clone();

	let output = quote!{
		#[test_case]
		fn #ident () {
			#func

			match crate::panicking::catch_unwind(#ident) {
				Ok(_) => crate::sprintln!("[FAILED]"),
				Err(_) => {}
			}
		}
	};
	output.into()
}