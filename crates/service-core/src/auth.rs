//! Authentication
use bitflags::bitflags;

bitflags! {
    /// Authorization flags that describe the account making the request
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct Flags : u16 {
        /// Missing or invalid token
        const NO_AUTH = 0;
        /// Bearer token purpose
        const BEARER_TOKEN = 1 << 0;
        /// Access token purpose
        const ACCESS_TOKEN = 1 << 1;
        /// Service account type
        const SERVICE_ACCOUNT = 1 << 2;
        /// Bot account type
        const BOT_ACCOUNT = 1 << 3;
        /// User account type
        const USER_ACCOUNT = 1 << 4;
        /// Admin account type
        const ADMIN_ACCOUNT = 1 << 5;
        /// Token is expired
        const EXPIRED = 1 << 6;
        /// Token is not expired
        const NOT_EXPIRED = 1 << 7;
    }
}

/// Combine [`Flags`]
#[macro_export]
macro_rules! auth {
    ($first:ident $(| $other:ident)*) => {
        $crate::auth::Flags::from_bits_truncate(
            $crate::auth::Flags::$first.bits() $(| $crate::auth::Flags::$other.bits())*
        )
    };
}

/// Convert [`Flags`] to an array of flag names
pub fn flag_names(flags: Flags) -> Vec<String> {
    flags.iter_names().map(|(name, _)| name.to_string()).collect()
}
