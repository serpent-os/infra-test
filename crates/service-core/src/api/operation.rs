use http;
use serde::{de::DeserializeOwned, Serialize};

use crate::api::Version;
use crate::auth;

pub trait Operation {
    type RequestBody: Serialize + DeserializeOwned;
    type ResponseBody: Serialize + DeserializeOwned;

    const VERSION: Version;
    const METHOD: http::Method;
    const PATH: &'static str;
    const AUTH: auth::Flags;
}

#[macro_export]
macro_rules! operation {
    ($ty:ident, $method:ident, $path:literal) => {
        operation!($ty, $method, $path, NO_AUTH, req: (), resp: ());
    };
    ($ty:ident, $method:ident, $path:literal, req: $req:ty) => {
        operation!($ty, $method, $path, NO_AUTH, req: $req, resp: ());
    };
    ($ty:ident, $method:ident, $path:literal, resp: $resp:ty) => {
        operation!($ty, $method, $path, NO_AUTH, req: (), resp: $resp);
    };
    ($ty:ident, $method:ident, $path:literal, req: $req:ty, resp: $resp:ty) => {
        operation!($ty, $method, $path, NO_AUTH, req: $req, resp: $resp);
    };
    ($ty:ident, $method:ident, $path:literal, $first:ident $(| $other:ident)*) => {
        operation!($ty, $method, $path, $first $(| $other)*, req: (), resp: ());
    };
    ($ty:ident, $method:ident, $path:literal, $first:ident $(| $other:ident)*, req: $req:ty) => {
        operation!($ty, $method, $path, $first $(| $other)*, req: $req, resp: ());
    };
    ($ty:ident, $method:ident, $path:literal, $first:ident $(| $other:ident)*, resp: $resp:ty) => {
        operation!($ty, $method, $path, $first $(| $other)*, req: (), resp: $resp);
    };
    ($ty:ident, $method:ident, $path:literal, $first:ident $(| $other:ident)*, req: $req:ty, resp: $resp:ty) => {
        pub struct $ty;

        impl $crate::api::Operation for $ty {
            type RequestBody = $req;
            type ResponseBody = $resp;

            // TODO: Allow override once v2+ is needed
            const VERSION: $crate::api::Version = $crate::api::Version::V1;
            const METHOD: http::Method = http::Method::$method;
            const PATH: &'static str = $path;
            const AUTH: $crate::auth::Flags = $crate::auth!($first $(| $other)*);
        }
    };
}
