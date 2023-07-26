#![feature(proc_macro_diagnostic)]
#![feature(let_chains)]

use proc_macro::{Diagnostic, Level, TokenStream};
use proc_macro::Level::Error;
use quote::{quote, ToTokens};
use syn::{Abi, Attribute, ExprLit, Ident, ItemFn, Lit, LitStr, Meta, MetaList, parse_macro_input, Path, ReturnType, Signature, Token, Type, Visibility};
use syn::__private::TokenStream2;
use syn::parse::{Parse, Parser, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use proc_macro2::Span;
use syn::Meta::{List, NameValue, Path as MetaPath};
use syn::token::Token;

#[proc_macro_attribute]
pub fn module_init(_attr: TokenStream, item: TokenStream) -> TokenStream {
	let mut init_fn = parse_macro_input!(item as ItemFn);

	let mut error = false;

	if init_fn.sig.unsafety.is_some() {
		Diagnostic::spanned(init_fn.sig.unsafety.span().unwrap(), Level::Error, "Module init function must not be unsafe")
				.emit();
		error = true;
	}
	if init_fn.sig.asyncness.is_some() {
		Diagnostic::spanned(init_fn.sig.asyncness.span().unwrap(), Level::Error, "Module init function must not be async")
				.emit();
		error = true;
	}

	if init_fn.sig.abi.is_none() || init_fn.sig.abi.clone().unwrap().name.unwrap().value() == "Rust" {
	} else {
		let span = if init_fn.sig.abi.is_some() { init_fn.sig.abi.span().unwrap() } else { init_fn.sig.span().unwrap() };
		Diagnostic::spanned(span, Level::Error, "Module init function must have Rust linkage")
				.emit();
		error = true;
	}

	if init_fn.sig.inputs.len() > 0 {
		Diagnostic::spanned(init_fn.sig.inputs.span().unwrap(), Level::Error, "Module init function must take no arguments")
				.emit();
		error = true;
	}

	let attr = Attribute::parse_outer.parse_str("#[no_mangle]");
	init_fn.attrs.append(&mut attr.unwrap());
	init_fn.vis = Visibility::Public((Token![pub])(init_fn.vis.span()));
	init_fn.sig.ident = Ident::new("__popcorn_module_init", init_fn.sig.ident.span());

	if error { return TokenStream::from(init_fn.into_token_stream()); }

	let output = quote! {
        #init_fn

        const _: extern "Rust" fn() -> bool = __popcorn_module_init;
    };
	TokenStream::from(output)
}

#[cfg(not(feature = "test"))]
#[proc_macro]
pub fn module_license(license: TokenStream) -> TokenStream {
	let name = parse_macro_input!(license as LitStr);
	let parsed = {
		let name = name.value();
		License::try_from(&name[..])
	};

	let output = match parsed {
		Err(_) => {
			Diagnostic::spanned(name.span().unwrap(), Level::Error, "Unknown license type")
					.emit();
			TokenStream2::new()
		}
		Ok(ty) => {
			let id = ty as u64;
			quote! {
				const _: () = {
			        #[no_mangle]
			        #[link_section = ".module_info"]
			        pub static __popcorn_module_license: u64 = #id;
				};
		    }
		}
	};

	TokenStream::from(output)
}

#[repr(u64)]
enum License {
	Unknown,
	Apache1_0,
	Apache1_1,
	Apache2_0,
	Gpl1Only,
	Gpl1Later,
	Gpl2Only,
	Gpl2Later,
	Gpl3Only,
	Gpl3Later,
	Mpl1_0,
	Mpl1_1,
	Mpl2_0
}

impl TryFrom<&str> for License {
	type Error = ();

	fn try_from(value: &str) -> Result<Self, Self::Error> {
		Ok(match value {
			"Apache-1.0" => Self::Apache1_0,
			"Apache-1.1" => Self::Apache1_1,
			"Apache-2.0" => Self::Apache2_0,
			"GPL-1.0-only" => Self::Gpl1Only,
			"GPL-1.0-or-later" => Self::Gpl1Later,
			"GPL-2.0-only" => Self::Gpl2Only,
			"GPL-2.0-or-later" => Self::Gpl2Later,
			"GPL-3.0-only" => Self::Gpl3Only,
			"GPL-3.0-or-later" => Self::Gpl3Later,
			"MPL-1.0" => Self::Mpl1_0,
			"MPL-1.1" => Self::Mpl1_1,
			"MPL-2.0" => Self::Mpl2_0,
			_ => Err(())?
		})
	}
}

#[cfg(not(feature = "test"))]
#[proc_macro]
pub fn module_author(name: TokenStream) -> TokenStream {
	let name = parse_macro_input!(name as LitStr).value();
	string_data(name, "author").into()
}

#[cfg(not(feature = "test"))]
#[proc_macro]
pub fn module_name(names: TokenStream) -> TokenStream {
	let data = Punctuated::<LitStr, Token![,]>::parse_separated_nonempty.parse(names).map_err(|e| e.to_compile_error());
	let data = match data {
		Ok(p) => p,
		Err(e) => return e.into()
	};

	if data.len() != 1 && data.len() != 2 {
		Diagnostic::spanned(data.span().unwrap(), Level::Error, "`module_name` takes two arguments").emit();
		return TokenStream::new();
	}

	let name = &data[0];
	let fqn = if data.len() == 2 { &data[1] } else { name };

	let name_tokens = string_data(name.value(), "modulename");
	let fqn_tokens = string_data(fqn.value(), "modulefqn");

	TokenStream::from(quote!{
		#name_tokens
		#fqn_tokens
	})
}

fn string_data(input: String, info_name: &str) -> proc_macro2::TokenStream {
	let input = input.as_bytes();
	let len = input.len();
	let data = proc_macro2::Literal::byte_string(input);

	let data_name: Ident = syn::parse_str(&format!("__popcorn_module_{info_name}")).unwrap();

	let output = quote!{
		const _: () = {
			#[no_mangle]
	        #[link_section = ".module_info"]
	        pub static #data_name : [u8; #len ] = * #data;
		};
	};

	output
}

#[cfg(not(feature = "test"))]
#[proc_macro]
pub fn module_export_type(ty: TokenStream) -> TokenStream {
	let data = Punctuated::<Path, Token![,]>::parse_separated_nonempty.parse(ty).map_err(|e| e.to_compile_error());
	let data = match data {
		Ok(p) => p,
		Err(e) => return e.into()
	};

	if data.len() != 2 {
		Diagnostic::spanned(data.span().unwrap(), Level::Error, "`module_export_type` exports requires two arguments - the first is a module trait and the second is a type").emit();
		return TokenStream::new();
	}

	let export_ty = &data[0];
	let singleton_ty = &data[1];

	let output = quote!{
		const _: () = {
			static mut __popcorn_module_object: ::core::mem::MaybeUninit<#singleton_ty> = ::core::mem::MaybeUninit::uninit();

			#[no_mangle]
		    pub fn __popcorn_module_init() -> Result<&'static dyn #export_ty, ()> {
				Ok(unsafe {
					__popcorn_module_object.write(<#singleton_ty as #export_ty>::new()?)
				})
		    }
		};
	};

	//TokenStream::from(output)
	TokenStream::new()
}

#[cfg(not(feature = "test"))]
#[proc_macro_attribute]
pub fn module_main(attributes: TokenStream, func: TokenStream) -> TokenStream {
	let attributes = parse_macro_input!(attributes as Meta);
	let func = parse_macro_input!(func as ItemFn);

	if let NameValue(_) = attributes {
		Diagnostic::spanned(attributes.span().unwrap(), Level::Error, "Expected module type with options").emit();
		return quote!{
			#func
		}.into();
	}

	let ident = match attributes {
		NameValue(_) => unreachable!(),
		MetaPath(ref path) |
		List(MetaList{ ref path, .. }) => {
			let Some(ident) = path.get_ident() else {
				Diagnostic::spanned(attributes.span().unwrap(), Level::Error, "Expected module type with options").emit();
				return quote!{
					#func
				}.into();
			};
			ident
		}
	};

	let mut options = vec![];
	if let List(MetaList{ ref tokens, .. }) = attributes {
		let attributes = Punctuated::<Ident, Token![,]>::parse_separated_nonempty.parse2(tokens.clone()).map_err(|e| e.to_compile_error());;
		let attributes = match attributes {
			Ok(p) => p,
			Err(e) => return e.into()
		};
		options.extend(attributes.iter().map(Ident::to_string))
	}

	let ty = match ident.to_string().as_str() {
		"allocator" => {
			if options.iter().any(|s| s == "general") {
				ModuleTy::Allocator(AllocatorTy::General)
			} else {
				Diagnostic::spanned(attributes.span().unwrap(), Level::Error, "Unknown allocator type - expected one of `general`").emit();
				return quote!{
					#func
				}.into();
			}
		}
		_ => ModuleTy::Error
	};

	if ty == ModuleTy::Error {
		Diagnostic::spanned(attributes.span().unwrap(), Level::Error, "Expected one of `allocator`").emit();
		return quote!{
			#func
		}.into();
	}

	let fname = ty.to_export_name();
	let inner_fname = func.sig.ident.clone();

	let class = ty.to_tokens();

	let output = quote!{
		const _: () = {
			#[no_mangle]
			pub extern "sysv64" fn #fname (range: Range<Frame>) -> Result<&'static dyn ::kernel_exports::memory::PhysicalMemoryAllocator, ()> {
			    let obj = Box::new( #inner_fname (range)?);
			    Ok(Box::leak(obj))
			}

			#class
		};

		#func
	};

	TokenStream::from(output)
}

#[derive(Eq, PartialEq, Copy, Clone)]
enum ModuleTy {
	Error,
	Allocator(AllocatorTy)
}

impl ModuleTy {
	fn to_export_name(self) -> Ident {
		match self {
			Self::Allocator(_) => Ident::new("__popcorn_module_main_allocator", Span::call_site()),
			_ => unimplemented!()
		}
	}

	fn to_tokens(self) -> proc_macro2::TokenStream {
		let (class, subclass): (u64, u64) = match self {
			Self::Error => unreachable!(),
			Self::Allocator(subclass) => (1, subclass.into())
		};

		quote! {
			#[no_mangle]
	        #[link_section = ".module_info"]
	        pub static __popcorn_module_class: u64 = #class;

			#[no_mangle]
	        #[link_section = ".module_info"]
	        pub static __popcorn_module_subclass: u64 = #subclass;
		}
	}
}

#[derive(Eq, PartialEq, Copy, Clone)]
enum AllocatorTy {
	General,

}

impl Into<u64> for AllocatorTy {
	fn into(self) -> u64 {
		match self {
			Self::General => 0
		}
	}
}

#[cfg(feature = "test")]
#[proc_macro]
pub fn module_license(license: TokenStream) -> TokenStream { TokenStream::new() }

#[cfg(feature = "test")]
#[proc_macro]
pub fn module_author(name: TokenStream) -> TokenStream  { TokenStream::new() }

#[cfg(feature = "test")]
#[proc_macro]
pub fn module_name(names: TokenStream) -> TokenStream  { TokenStream::new() }

#[cfg(feature = "test")]
#[proc_macro]
pub fn module_export_type(ty: TokenStream) -> TokenStream { TokenStream::new() }