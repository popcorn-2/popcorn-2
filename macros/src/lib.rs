use proc_macro::TokenStream;
use std::str::FromStr;
use syn::{DeriveInput, parse_macro_input};

#[proc_macro_derive(Hal)]
pub fn derive_hal_const(item: TokenStream) -> TokenStream {
	let item = parse_macro_input!(item as DeriveInput);
	let output = format!("const _: crate::HalTy = {};", item.ident);
	TokenStream::from_str(&output).unwrap()
}
