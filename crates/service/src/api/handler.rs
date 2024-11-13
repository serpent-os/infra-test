//! Define a handler for an API [`Operation`]
use futures::Future;
use service_core::api::Operation;

use super::Request;

/// Handle an API [`Operation`]
pub trait Handler<O, S>
where
    O: Operation,
{
    /// Handler error
    type Error;

    /// Handle an incoming request and return a response
    fn handle(
        self,
        req: Request<O>,
        state: S,
    ) -> impl Future<Output = Result<<O as Operation>::ResponseBody, Self::Error>> + Send;
}

impl<O, FN, F, E, S> Handler<O, S> for FN
where
    O: Operation,
    FN: Fn(Request<O>, S) -> F,
    F: Future<Output = Result<<O as Operation>::ResponseBody, E>> + Send,
{
    type Error = E;

    fn handle(
        self,
        req: Request<O>,
        state: S,
    ) -> impl Future<Output = Result<<O as Operation>::ResponseBody, Self::Error>> + Send {
        (self)(req, state)
    }
}
