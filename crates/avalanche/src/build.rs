use std::path::Path;

use color_eyre::eyre::{eyre, Context, OptionExt, Result};
use http::Uri;
use itertools::Itertools;
use service::{
    api::{self, v1::avalanche::PackageBuild},
    error, Endpoint, State,
};
use service_types::{collectable, Collectable, Remote};
use sha2::{Digest, Sha256};
use tokio::{
    fs::{self, File},
    process,
};
use tracing::{error, info};

use crate::Config;

#[tracing::instrument(
    skip_all,
    fields(
        build_id = request.build_id,
        endpoint = %endpoint.id,
    )
)]
pub async fn build(request: PackageBuild, endpoint: Endpoint, state: State, config: Config) {
    info!("Starting build");

    let client = service::Client::new(endpoint.host_address.clone()).with_endpoint_auth(endpoint.id, state.db.clone());

    let task_id = request.build_id;

    let status = match run(request, endpoint, state, config).await {
        Ok(collectables) => {
            info!("Build succeeded");

            client
                .send::<api::v1::summit::BuildSucceeded>(&api::v1::summit::BuildBody { task_id, collectables })
                .await
        }
        Err(e) => {
            let error = error::chain(e.as_ref() as &dyn std::error::Error);
            error!(%error, "Build failed");

            client
                .send::<api::v1::summit::BuildFailed>(&api::v1::summit::BuildBody {
                    task_id,
                    collectables: vec![],
                })
                .await
        }
    };

    if let Err(e) = status {
        let error = error::chain(e);
        error!(%error, "Failed to send build status response");
    }
}

async fn run(request: PackageBuild, _endpoint: Endpoint, state: State, config: Config) -> Result<Vec<Collectable>> {
    let uri = request.uri.parse::<Uri>().context("invalid upstream URI")?;

    let cache_dir = state.state_dir.join("cache");
    let mirror_dir = cache_dir.join(
        uri.path()
            .strip_prefix("/")
            .ok_or_eyre("path should always have leading slash")?,
    );

    if let Some(parent) = mirror_dir.parent() {
        ensure_dir_exists(parent).await.context("create mirror parent dir")?;
    }

    let work_dir = state.state_dir.join("work");
    recreate_dir(&work_dir).await.context("recreate work dir")?;

    let worktree_dir = work_dir.join("source");
    ensure_dir_exists(&worktree_dir).await.context("create worktree dir")?;

    let asset_dir = state.root.join("assets").join(request.build_id.to_string());
    recreate_dir(&asset_dir).await.context("recreate asset dir")?;

    let log_file = asset_dir.join("build.log");

    mirror_recipe_repo(&uri, &mirror_dir)
        .await
        .context("mirror recipe repo")?;

    checkout_commit_to_worktree(&mirror_dir, &worktree_dir, &request.commit_ref)
        .await
        .context("checkout commit as worktree")?;

    create_boulder_config(&work_dir, &request.remotes)
        .await
        .context("create boulder config")?;

    build_recipe(&work_dir, &asset_dir, &worktree_dir, &request.relative_path, &log_file)
        .await
        .context("build recipe")?;

    tokio::task::spawn_blocking(move || compress_file(&log_file))
        .await
        .context("spawn blocking")?
        .context("compress log file")?;

    let collectables = scan_collectables(request.build_id, &config.host_address, &asset_dir)
        .await
        .context("scan collectables")?;

    remove_worktree(&mirror_dir, &worktree_dir)
        .await
        .context("remove worktree")?;

    Ok(collectables)
}

async fn ensure_dir_exists(path: &Path) -> Result<()> {
    Ok(fs::create_dir_all(path).await?)
}

async fn recreate_dir(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path).await?;
    }

    Ok(fs::create_dir_all(path).await?)
}

fn validate_status(command: &'static str, result: Result<std::process::ExitStatus, std::io::Error>) -> Result<()> {
    let status = result.context(command)?;

    if !status.success() {
        if let Some(code) = status.code() {
            return Err(eyre!("{command} failed with exit status {code}"));
        } else {
            return Err(eyre!("{command} exited with failure"));
        }
    }

    Ok(())
}

async fn mirror_recipe_repo(uri: &Uri, mirror_dir: &Path) -> Result<()> {
    if mirror_dir.exists() {
        info!(%uri, "Updating mirror of recipe repo");

        validate_status(
            "git remote update",
            process::Command::new("git")
                .args(["remote", "update"])
                .current_dir(mirror_dir)
                .output()
                .await
                .map(|o| o.status),
        )?;
    } else {
        info!(%uri, "Creating mirror of recipe repo");

        validate_status(
            "git clone --mirror",
            process::Command::new("git")
                .args(["clone", "--mirror", "--"])
                .arg(uri.to_string())
                .arg(mirror_dir)
                .output()
                .await
                .map(|o| o.status),
        )?;
    }

    Ok(())
}

async fn checkout_commit_to_worktree(mirror_dir: &Path, worktree_dir: &Path, commit_ref: &str) -> Result<()> {
    info!(commit_ref, "Checking out commit ref to worktree");

    validate_status(
        "git worktree add",
        process::Command::new("git")
            .args(["worktree", "add"])
            .arg(worktree_dir)
            .arg(commit_ref)
            .current_dir(mirror_dir)
            .output()
            .await
            .map(|o| o.status),
    )
}

async fn remove_worktree(mirror_dir: &Path, worktree_dir: &Path) -> Result<()> {
    info!("Removing worktree");

    validate_status(
        "git worktree remove",
        process::Command::new("git")
            .args(["worktree", "remove"])
            .arg(worktree_dir)
            .current_dir(mirror_dir)
            .output()
            .await
            .map(|o| o.status),
    )
}

async fn create_boulder_config(work_dir: &Path, remotes: &[Remote]) -> Result<()> {
    info!("Creating boulder config");

    let remotes = remotes
        .iter()
        .map(|remote| {
            format!(
                "
        {}:
            uri: \"{}\"
            description: \"Remotely configured repository\"
            priority: {}
                ",
                remote.name, remote.index_uri, remote.priority,
            )
        })
        .join("\n");

    let config = format!(
        "
avalanche:
    repositories:
{remotes}
        "
    );

    let config_dir = work_dir.join("etc/boulder/profile.d");
    ensure_dir_exists(&config_dir)
        .await
        .context("create boulder config dir")?;

    fs::write(config_dir.join("avalanche.yaml"), config)
        .await
        .context("write boulder config")?;

    Ok(())
}

async fn build_recipe(
    work_dir: &Path,
    asset_dir: &Path,
    worktree_dir: &Path,
    relative_path: &str,
    log_path: &Path,
) -> Result<()> {
    let log_file = File::create(log_path)
        .await
        .context("create log file")?
        .into_std()
        .await;

    info!("Building recipe");

    validate_status(
        "boulder",
        process::Command::new("sudo")
            .args(["nice", "-n20", "boulder", "build", "-p", "avalanche", "--update", "-o"])
            .arg(asset_dir)
            .arg("--config-dir")
            .arg(work_dir.join("etc/boulder"))
            .arg("--")
            .arg(relative_path)
            .current_dir(worktree_dir)
            .stdout(log_file.try_clone()?)
            .stderr(log_file)
            .status()
            .await,
    )
}

fn compress_file(file: &Path) -> Result<()> {
    use flate2::write::GzEncoder;
    use std::fs::{self, File};
    use std::io::{self, Write};

    let mut plain_file = File::open(file).context("open plain file")?;
    let mut gz_file = File::create(format!("{}.gz", file.display())).context("create compressed file")?;

    let mut encoder = GzEncoder::new(&mut gz_file, flate2::Compression::new(9));

    io::copy(&mut plain_file, &mut encoder)?;

    encoder.finish()?;
    gz_file.flush()?;

    fs::remove_file(file).context("remove plain file")?;

    Ok(())
}

async fn scan_collectables(build_id: u64, host_address: &Uri, asset_dir: &Path) -> Result<Vec<Collectable>> {
    let mut collectables = vec![];

    let mut contents = fs::read_dir(asset_dir).await.context("read asset dir")?;

    while let Some(entry) = contents.next_entry().await.context("get next assets dir entry")? {
        let path = entry.path();

        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        let mut kind = collectable::Kind::Unknown;

        if file_name.ends_with(".bin") {
            kind = collectable::Kind::BinaryManifest;
        } else if file_name.ends_with(".jsonc") {
            kind = collectable::Kind::JsonManifest;
        } else if file_name.ends_with(".log.gz") {
            kind = collectable::Kind::Log;
        } else if file_name.ends_with(".stone") {
            kind = collectable::Kind::Package;
        }

        let uri = format!("{host_address}assets/{build_id}/{file_name}")
            .parse()
            .context("invalid asset URI")?;

        let sha256sum = tokio::task::spawn_blocking(move || compute_sha256(&path))
            .await
            .context("spawn blocking")?
            .context("compute asset sha256")?;

        collectables.push(Collectable { kind, uri, sha256sum })
    }

    Ok(collectables)
}

fn compute_sha256(file: &Path) -> Result<String> {
    use std::fs::File;
    use std::io;

    let file = File::open(file).context("open file")?;
    let mut hasher = Sha256::default();

    io::copy(&mut &file, &mut hasher)?;

    Ok(hex::encode(hasher.finalize()))
}
