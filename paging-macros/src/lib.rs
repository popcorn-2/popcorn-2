use proc_macro::{TokenStream};
use quote::{quote, ToTokens};
use syn::{ItemStruct, LitInt, parse_macro_input};

#[proc_macro_attribute]
pub fn page_table(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = parse_macro_input!(attr as LitInt);

    let Ok(4) = attr.base10_parse() else {
        unimplemented!()
    };

    let item = parse_macro_input!(item as ItemStruct);
    let item_name = item.ident.clone();

    let output = quote!{
        #item

        #[no_mangle]
        fn __popcorn_paging_page_table_new() -> #item_name {
            <#item_name as crate::paging::ArchPageTable>::new()
        }

        #[no_mangle]
        fn __popcorn_paging_page_table_foo(table: & #item_name) {
            //crate::paging::ArchPageTable::foo(table)
        }
    };

    output.into()
}

#[proc_macro]
pub fn l4_table_ty(_item: TokenStream) -> TokenStream {
    "()".parse().unwrap()
}
