use std::path::{Path, PathBuf};

use color_eyre::eyre::{Context, OptionExt, Result};
use moss::{db::meta, package::Meta};
use service::{State, git};
use sqlx::SqliteConnection;
use tokio::{fs, task};
use tracing::{info, trace};

use super::{Repository, Status, set_description, set_status};

#[tracing::instrument(name = "reindex_repository", skip_all, fields(commit_ref = repo.commit_ref))]
pub async fn reindex(
    conn: &mut SqliteConnection,
    state: &State,
    repo: &mut Repository,
    db: meta::Database,
) -> Result<()> {
    info!("Reindexing repository");

    set_status(conn, repo, Status::Indexing)
        .await
        .context("set status to indexing")?;

    let repo_dir = state.cache_dir.join("repository").join(repo.id.to_string());
    let clone_dir = repo_dir.join("clone");
    let work_dir = repo_dir.join("work");

    checkout_commit(repo, &clone_dir, &work_dir)
        .await
        .context("checkout commit")?;

    update_readme(conn, repo, &work_dir).await.context("update readme")?;

    let num_indexed = task::spawn_blocking(move || update_manifests(work_dir, db))
        .await
        .context("join handle")?
        .context("update manifests")?;

    set_status(conn, repo, Status::Idle)
        .await
        .context("set status to idle")?;

    info!(num_indexed, "Indexing finished");

    Ok(())
}

async fn checkout_commit(repo: &Repository, clone_dir: &Path, work_dir: &Path) -> Result<()> {
    let _ = fs::remove_dir_all(&work_dir).await;

    git::checkout_worktree(
        clone_dir,
        work_dir,
        repo.commit_ref.as_ref().ok_or_eyre("no commit checked out")?,
    )
    .await
    .context("git checkout worktree")?;

    Ok(())
}

async fn update_readme(conn: &mut SqliteConnection, repo: &mut Repository, work_dir: &Path) -> Result<()> {
    let path = work_dir.join("README.md");

    if !fs::try_exists(&path).await.unwrap_or_default() {
        return Ok(());
    }

    let content = fs::read_to_string(&path).await.context("read README.md")?;

    set_description(conn, repo, content).await.context("set description")?;

    Ok(())
}

fn update_manifests(work_dir: PathBuf, db: meta::Database) -> Result<usize> {
    let manifests = enumerate_manifests(&work_dir).context("enumerate manifests")?;

    db.wipe().context("wipe meta db")?;

    for manifest in &manifests {
        install_manifest(&db, manifest).context("install manifest")?;
    }

    Ok(manifests.len())
}

fn enumerate_manifests(dir: &Path) -> Result<Vec<PathBuf>> {
    use std::fs;

    let contents = fs::read_dir(dir).context("read dir")?;

    let mut manifests = vec![];

    for result in contents {
        let entry = result.context("read dir entry")?;
        let meta = entry.metadata().context("entry metadata")?;
        let path = entry.path();
        let name = path.file_name().and_then(|s| s.to_str()).unwrap_or_default();

        if meta.is_file() && name.starts_with("manifest.") && name.ends_with(".bin") {
            manifests.push(path);
        } else if meta.is_dir() {
            manifests.extend(enumerate_manifests(&path)?);
        }
    }

    Ok(manifests)
}

#[tracing::instrument(skip_all, fields(manifest = %manifest.display()))]
fn install_manifest(db: &meta::Database, manifest: &Path) -> Result<()> {
    use std::fs;

    let file = fs::File::open(manifest).context("open manifest reader")?;

    let mut reader = stone::read(&file).context("read stone header")?;

    let payloads = reader
        .payloads()
        .context("read stone payloads")?
        .collect::<Result<Vec<_>, _>>()
        .context("read stone payloads")?;

    let meta_payload = payloads
        .iter()
        .find_map(stone::read::PayloadKind::meta)
        .ok_or_eyre("missing meta payload")?;

    let meta = Meta::from_stone_payload(&meta_payload.body).context("convert meta payload to metadata")?;

    db.add(meta.id().into(), meta).context("add meta to db")?;

    trace!("Manifest installed");

    Ok(())
}
