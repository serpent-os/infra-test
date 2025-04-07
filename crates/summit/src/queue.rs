use std::collections::HashMap;

use color_eyre::eyre::{self, Context, OptionExt, Result};
use dag::Dag;
use futures_util::{StreamExt, TryStreamExt, stream};
use moss::{db::meta, package::Meta};
use sqlx::SqliteConnection;
use tokio::task::spawn_blocking;

use crate::{Profile, Project, Repository, Task, repository, task};

#[derive(Default)]
pub struct Queue(Vec<Task>);

impl Queue {
    #[tracing::instrument(name = "recompute_queue", skip_all)]
    pub async fn recompute(
        &mut self,
        conn: &mut SqliteConnection,
        projects: &[Project],
        repo_dbs: &HashMap<repository::Id, meta::Database>,
    ) -> Result<()> {
        let open_tasks = task::list(conn, task::Status::open())
            .await
            .context("list open tasks")?;

        let mapped_tasks = stream::iter(open_tasks)
            .then(|task| async {
                let project = projects
                    .iter()
                    .find(|p| p.id == task.project_id)
                    .ok_or_eyre("task has missing project")?;
                let profile = project
                    .profiles
                    .iter()
                    .find(|p| p.id == task.profile_id)
                    .ok_or_eyre("task has missing profile")?;
                let repo = project
                    .repositories
                    .iter()
                    .find(|r| r.id == task.repository_id)
                    .ok_or_eyre("task has missing repo")?;
                let db = repo_dbs.get(&repo.id).ok_or_eyre("repo has missing meta db")?.clone();

                let package_id = task.package_id.clone();

                let meta = spawn_blocking(move || db.get(&package_id))
                    .await
                    .context("join handle")?
                    .context("find meta in repo db for task")?;

                Result::<_, eyre::Report>::Ok((
                    task.id,
                    Mapper {
                        task,
                        project,
                        profile,
                        repo,
                        meta,
                    },
                ))
            })
            .try_collect::<HashMap<_, _>>()
            .await?;

        let mut dag = Dag::<task::Id>::new();

        for current in mapped_tasks.values() {
            let current_node = dag.add_node_or_get_index(current.task.id);

            // All other tasks which share the same arch & index
            let common_tasks = mapped_tasks
                .values()
                .filter(|a| {
                    current.task.id != a.task.id
                        && current.task.arch == a.task.arch
                        && (current.profile.index_uri == a.profile.index_uri
                            || current
                                .profile
                                .remotes
                                .iter()
                                .any(|r| r.index_uri == a.profile.index_uri))
                })
                .collect::<Vec<_>>();

            // for dep in current
            for dep in &current.meta.dependencies {
                common_tasks
                    .iter()
                    .filter(|a| {
                        a.meta
                            .providers
                            .iter()
                            .any(|p| p.kind == dep.kind && p.name == dep.name)
                    })
                    .for_each(|provider| {
                        let provider_node = dag.add_node_or_get_index(provider.task.id);

                        dag.add_edge(provider_node, current_node);
                    });
            }
        }

        let topo = dag
            .topo()
            .map(|id| &mapped_tasks.get(id).unwrap().task.build_id)
            .collect::<Vec<_>>();

        dbg!(&topo);
        dbg!(topo.len());
        dbg!(mapped_tasks.len());

        Ok(())
    }
}

struct Mapper<'a> {
    task: Task,
    project: &'a Project,
    profile: &'a Profile,
    repo: &'a Repository,
    meta: Meta,
}
