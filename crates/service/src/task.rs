use std::{collections::HashMap, future::IntoFuture, time::Duration};

use tokio::{
    select,
    sync::broadcast,
    task::{Id, JoinSet},
    time::timeout,
};
use tracing::{debug, error};

pub use tokio_util::sync::CancellationToken;

const DEFAULT_GRACEFUL_SHUTDOWN: Duration = Duration::from_secs(5);

type BoxError = Box<dyn std::error::Error + Send>;
type Output = Result<(), BoxError>;

pub struct Runner {
    cancellation_token: CancellationToken,
    graceful_shutdown: Duration,
    begin: broadcast::Sender<()>,
    names: HashMap<Id, &'static str>,
    set: JoinSet<Output>,
}

impl Runner {
    pub fn new() -> Self {
        Self {
            cancellation_token: CancellationToken::new(),
            graceful_shutdown: DEFAULT_GRACEFUL_SHUTDOWN,
            begin: broadcast::Sender::new(1),
            names: HashMap::default(),
            set: JoinSet::default(),
        }
    }

    pub fn with_graceful_shutdown(self, duration: Duration) -> Self {
        Self {
            graceful_shutdown: duration,
            ..self
        }
    }

    pub fn with_task<F, E>(self, name: &'static str, task: F) -> Self
    where
        F: IntoFuture<Output = Result<(), E>>,
        F::IntoFuture: Send + 'static,
        E: std::error::Error + Send + 'static,
    {
        let task = task.into_future();

        self.with_cancellation_task(name, |token| async move {
            select! {
                _ = token.cancelled() => Ok(()),
                res = task => res,
            }
        })
    }

    pub fn with_cancellation_task<F, E>(mut self, name: &'static str, f: impl FnOnce(CancellationToken) -> F) -> Self
    where
        F: IntoFuture<Output = Result<(), E>>,
        F::IntoFuture: Send + 'static,
        E: std::error::Error + Send + 'static,
    {
        let task = f(self.cancellation_token.child_token()).into_future();

        let mut wait = self.begin.subscribe();

        let handle = self.set.spawn(async move {
            // Wait until runner is run
            let _ = wait.recv().await;

            let id = tokio::task::id();
            debug!(%id, name, "Task started");

            // Run task
            task.await.map_err(|e| Box::new(e) as BoxError)
        });

        self.names.insert(handle.id(), name);

        self
    }

    pub async fn run(mut self) {
        if self.set.is_empty() {
            return;
        }

        // Begin all tasks
        let _ = self.begin.send(());

        // Wait for first task to exit
        let Some(result) = self.set.join_next_with_id().await else {
            return;
        };

        // Log it
        log_result(result, &self.names);

        // Notify remaining tasks of shutdown
        self.cancellation_token.cancel();

        // Give graceful shutdown duration for tasks to exit
        let _ = timeout(self.graceful_shutdown, async {
            while let Some(result) = self.set.join_next_with_id().await {
                log_result(result, &self.names);
            }
        })
        .await;

        // If all tasks exited within graceful shutdown
        // we can return
        if self.set.is_empty() {
            return;
        }

        // Abort remaining tasks
        self.set.abort_all();

        // Log each one, then exit
        while let Some(result) = self.set.join_next_with_id().await {
            log_result(result, &self.names);
        }
    }
}

fn log_result(result: Result<(Id, Output), tokio::task::JoinError>, names: &HashMap<Id, &'static str>) {
    match result {
        Ok((id, Ok(_))) => {
            let name = names.get(&id).expect("unique task id");
            debug!(%id, name, "Task exited successfully");
        }
        Ok((id, Err(e))) => {
            let name = names.get(&id).expect("unique task id");
            let error = crate::error::chain(&*e);
            error!(%id, name, %error, "Task exited with error");
        }
        Err(e) => {
            let id = e.id();
            let name = names.get(&id).expect("unique task id");
            let error = crate::error::chain(e);
            error!(%id, name, %error, "Task failed to execute to completion");
        }
    }
}
