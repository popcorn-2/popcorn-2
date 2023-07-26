#[macro_export]
macro_rules! newtype_enum {
    (
        $(#[$type_attrs:meta])*
        $visibility:vis enum $type:ident : $base_integer:ty => $(#[$impl_attrs:meta])* {
            $(
                $(#[$variant_attrs:meta])*
                $variant:ident = $value:expr,
            )*
        }
    ) => {
        $(#[$type_attrs])*
        #[repr(transparent)]
        #[derive(Clone, Copy, Eq, PartialEq)]
        $visibility struct $type(pub $base_integer);

        $(#[$impl_attrs])*
        #[allow(unused)]
        impl $type {
            $(
                $(#[$variant_attrs])*
                pub const $variant: $type = $type($value);
            )*
        }

        #[allow(unused)]
        impl core::fmt::Debug for $type {
            fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
                match *self {
                    // Display variants by their name, like Rust enums do
                    $(
                        $type::$variant => write!(f, stringify!($variant)),
                    )*

                    // Display unknown variants in tuple struct format
                    $type(unknown) => {
                        write!(f, "{}({})", stringify!($type), unknown)
                    }
                }
            }
        }
    }
}
