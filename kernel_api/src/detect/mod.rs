//! Provides runtime system feature detection.

#![expect(clippy::allow_attributes, reason = "required attributes may depend on architecture specific implementation")]

#[cfg(target_arch = "x86_64")]
mod x86;

#[cfg(target_arch = "x86_64")]
#[doc(hidden)]
pub use x86::*;

mod cache;

macro get_first_feature {
    ($feature:literal $(, $rest:literal)*) => {$feature},
}

macro_rules! features_macro {
    (
        @TARGET: $target:ident;
        @CFG: $cfg:meta;
        @MACRO_NAME: $macro_name:ident;
        @MACRO_ATTRS: $(#[$macro_attrs:meta])*
        $(@BIND_FEATURE_NAME: $bind_feature:tt; $feature_impl:tt;)*
        $(@FEATURE: $feature:ident: $feature_lit:tt: $feature_doc:tt;)*
    ) => {
        #[macro_export]
        $(#[$macro_attrs])*
        /// Returns a bool if the current system supports the requested feature
        ///
        /// The features available to test are:
        $(#[doc = concat!("- `", $feature_lit, "` - ", $feature_doc, "\n")])*
        ///
        /// # Examples
        ///
        /// ```
        #[doc = concat!("use kernel_api::", stringify!($macro_name), ";\n\n")]
        #[doc = concat!("if ", stringify!($macro_name), "!(\"", $crate::detect::get_first_feature!($($feature_lit),*), "\") {")]
        #[doc = concat!("    info!(\"`", $crate::detect::get_first_feature!($($feature_lit),*), "` is supported on this platform\");\n")]
        /// }
        /// ```
        macro_rules! $macro_name {
            $(
                ($feature_lit) => {
                    $crate::detect::__detected::$feature()
                };
            )*
            $(
                ($bind_feature) => {
                    $crate::detect::__detected::$feature_impl()
                };
            )*
            ($t:tt,) => {
                    $crate::$macro_name!($t);
            };
            ($t:tt) => {
                compile_error!(
                    concat!(
                        concat!("unknown ", stringify!($target)),
                        concat!(" target feature: ", $t)
                    )
                )
            };
        }

        #[doc(hidden)]
        #[allow(non_camel_case_types, reason = "feature names may not be camel case")]
        #[derive(Copy, Clone, Debug)]
        #[repr(u32)]
        #[cfg($cfg)]
        pub enum Feature {
            $(
                $feature,
            )*
            _last
        }

        #[cfg($cfg)]
        impl Feature {
            pub(crate) fn to_str(self) -> &'static str {
                match self {
                    $(Feature::$feature => $feature_lit,)*
                    Feature::_last => unreachable!(),
                }
            }
        }

        #[doc(hidden)]
        #[cfg($cfg)]
        pub mod __detected {
            $(
                #[inline]
                #[doc(hidden)]
                pub fn $feature() -> bool {
                    $crate::detect::cache::test($crate::detect::Feature::$feature)
                }
            )*
        }
    };
}

pub(crate) use features_macro;

/// Returns an iterator over feature names and their support on the current system.
///
/// # Examples
///
/// Print all supported features:
/// ```
/// use kernel_api::detect::features;
///
/// println!("Supported features:");
/// features()
///     .filter_map(|(name, supported)| supported.then_some(name))
///     .for_each(|name| println!("- {name}"));
/// ```
pub fn features() -> impl Iterator<Item = (&'static str, bool)> {
    (0u32..Feature::_last as u32).map(|discriminant: u32| {
	    // SAFETY: `Feature` is `repr(u32)`, `discriminant` is within the valid range of discriminants
	    //  as it is less than `Feature::_last` and there are no gaps in discriminant values
        let f: Feature = unsafe { core::mem::transmute::<u32, Feature>(discriminant) };
        let name = f.to_str();
        let enabled = cache::test(f);
        (name, enabled)
    })
}
