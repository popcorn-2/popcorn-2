#![feature(proc_macro_diagnostic)]
#![feature(let_chains)]

use proc_macro::{Diagnostic, Level, Span, TokenStream};
use quote::{quote, ToTokens};
use syn::{Abi, Ident, ItemFn, parse_macro_input, ReturnType, Signature, Visibility};
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;

/*static TABLES: [Mutex<Cell<Option<Span>>>; 4] = [const { Mutex::new(Cell::new(None)) }; 4];

#[proc_macro_attribute]
pub fn page_table(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut params = attr.into_iter();

    let level = match params.next() {
        Some(TokenTree::Literal(level)) => level,
        Some(token) => {
            Diagnostic::spanned(token.span(), Level::Error, "Expected a page table level (1-4)")
                    .emit();
            return item;
        },
        _ => return quote! { compile_error!("Expected a page table level (1-4)") }.into()
    };

    let span = level.span();
    let level = level.to_string();
    let Ok(level @ 1..=4) = level.parse::<usize>() else {
        Diagnostic::spanned(span, Level::Error, "Expected a page table level (1-4)")
                .emit();
        return item;
    };

    let data = TABLES[level - 1].get();
    if data.is_some() {
        let err = format!("Page table for level {level} has already been defined");
        let note = format!("Page table level {level} defined here");
        Diagnostic::spanned(span, Level::Error, err)
                .span_note(data.unwrap(), note)
                .emit();
    } else {
        table[level - 1].set(Some(span));
    }

    item
}*/

/*#[macro_export]
macro_rules! module_license {
    ($name: literal) => {};
}

#[macro_export]
macro_rules! module_name {
    ($name: literal) => {};
}

#[macro_export]
macro_rules! module_author {
    ($name: literal) => {};
}*/
