/*#[macro_export]
macro_rules! module_license {
    ($license:literal) => {
        const _: () = {
            #[no_mangle]
            #[link_section = ".module_info"]
            pub static mut __popcorn_module_license_identifier: &'static str = $license;
        };
    };
}

#[macro_export]
macro_rules! module_author {
    ($author:literal) => {
        const _: () = {
            #[no_mangle]
            #[link_section = ".module_info.author"]
            pub static mut __popcorn_module_author_name: &'static str = $author;
        };
    };
}
*/

#[cfg(target_arch = "x86_64")]
#[macro_export]
macro_rules! ffi_abi {
    (type fn( $($param_ty:ty),* ) $(-> $ret_ty:ty)? ) => { extern "sysv64" fn($($param_ty),*) $(-> $ret_ty)? };

    ($vis:vis fn $name:ident ( $($param_name:ident : $param_ty:ty),* ) $(-> $ret_ty:ty)?) => {
        $vis extern "sysv64" $name fn($($param_name: $param_ty),*) $(-> $ret_ty)? {
            todo!()
        }
    }
}
