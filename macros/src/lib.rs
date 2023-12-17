use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemFn, parse_macro_input, Ident};

#[proc_macro_attribute]
pub fn test_should_panic(_attr: TokenStream, item: TokenStream) -> TokenStream {
	let func = parse_macro_input!(item as ItemFn);
	let ident = func.sig.ident.clone();

	let output = quote!{
		#[test_case]
		static #ident: ::kernel::test_harness::ShouldPanic<fn()> = {
			#func

			const fn name<T: ?Sized>(_val: &T) -> &'static str {
                ::core::any::type_name::<T>()
			}

			::kernel::test_harness::ShouldPanic(#ident, name(& #ident))
		};
	};
	output.into()
}

#[proc_macro_attribute]
pub fn test_ignored(_attr: TokenStream, item: TokenStream) -> TokenStream {
	let func = parse_macro_input!(item as ItemFn);
	let ident = func.sig.ident.clone();

	let output = quote!{
		#[test_case]
		static #ident: ::kernel::test_harness::Ignored<fn()> = {
			#func

			const fn name<T: ?Sized>(_val: &T) -> &'static str {
                ::core::any::type_name::<T>()
			}

			::kernel::test_harness::Ignored(#ident, name(& #ident))
		};
	};
	output.into()
}
