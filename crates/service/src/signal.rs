use std::io;

use futures::{future, FutureExt};
use tokio::signal::unix::signal;
pub use tokio::signal::unix::SignalKind as Kind;

pub async fn capture(signals: impl IntoIterator<Item = Kind>) -> io::Result<()> {
    let mut signals = signals
        .into_iter()
        .map(signal)
        .collect::<Result<Vec<_>, _>>()?;

    future::select_all(signals.iter_mut().map(|signal| signal.recv().boxed())).await;

    Ok(())
}
