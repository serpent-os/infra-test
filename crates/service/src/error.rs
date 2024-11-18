//! Handle errors
use itertools::Itertools;

/// Format an error chain
pub fn chain<E: std::error::Error>(err: E) -> String {
    let mut chain = vec![err.to_string()];
    let mut source = err.source();
    while let Some(cause) = source {
        chain.push(cause.to_string());
        source = cause.source();
    }
    chain.into_iter().join(": ")
}
