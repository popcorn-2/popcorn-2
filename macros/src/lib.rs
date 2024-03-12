#![feature(proc_macro_diagnostic)]

use proc_macro::{Span, TokenStream};
use std::str::FromStr;
use syn::{Data, DeriveInput, parse_macro_input, Visibility};
use syn::__private::ToTokens;
use syn::spanned::Spanned;

#[proc_macro_derive(Hal)]
pub fn derive_hal_const(item: TokenStream) -> TokenStream {
	let item = parse_macro_input!(item as DeriveInput);
	let output = format!("const _: crate::HalTy = {};", item.ident);
	TokenStream::from_str(&output).unwrap()
}

#[proc_macro_derive(Fields)]
pub fn derive_fields(item: TokenStream) -> TokenStream {
	let item = parse_macro_input!(item as DeriveInput);
	let Data::Struct(ref s) = item.data else {
		item.span().unwrap().error("Expected `struct`").emit();
		return TokenStream::new();
	};

	let mut output = String::from("const _: () = {\n\tuse kernel::projection::Field;\n");

	for (i, field) in s.fields.iter().enumerate() {
		output += &format!(r#"
			{2} struct {0}_{1};

			impl {0} {{
				{2} type {1} = {0}_{1};
			}}

			unsafe impl Field for {0}_{1} {{
				type Base = {0};
				type Inner =
		"#,
			item.ident,
			field.ident.as_ref().map(|x| x.to_string()).unwrap_or_else(|| i.to_string()),
			match &item.vis {
				Visibility::Public(_) => "pub".to_string(),
				Visibility::Restricted(vis) => format!("pub({} {})", vis.in_token.map(|_| "in").unwrap_or_default(), vis.path.to_token_stream().to_string()),
				_ => "".to_string()
			}
		);

		let tokens = TokenStream::from(field.ty.to_token_stream());
		output += &tokens.to_string();
		output += &format!(r#"
			;
			const OFFSET: usize = core::mem::offset_of!({0}, {1});
		}};
		"#, item.ident, field.ident.as_ref().map(|x| x.to_string()).unwrap_or_else(|| i.to_string()));
	}

	output += "};";

	TokenStream::from_str(&output).unwrap()
}
