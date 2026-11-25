use crate::{
	cache::{self, Cache, Record},
	cargo,
	cli::{Args, Spec},
	index::{Index, MinimumAge},
	install,
};
use anyhow::{Context, Result, ensure};
use reqwest::{Client, Response};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
	fs::{self, File},
	io::Write,
	path::PathBuf,
	process::{Command, Stdio},
	time::{Duration, SystemTime},
};

pub fn runtime() -> Result<tokio::runtime::Runtime> {
	Ok(tokio::runtime::Builder::new_current_thread().enable_all().build()?)
}

pub fn client() -> Result<Client> {
	Ok(Client::builder()
		.https_only(true)
		.user_agent(concat!("crabx/", env!("CARGO_PKG_VERSION")))
		.timeout(Duration::from_secs(30))
		.build()?)
}

/// The `GH_TOKEN` or `GITHUB_TOKEN` credential, which raises GitHub's API rate limit.
// It is sent only to the fixed GitHub API origin, never to archive hosts, sparse
// indexes, or Sigstore's trust-root services.
pub fn github_token() -> Result<Option<String>> {
	for name in ["GH_TOKEN", "GITHUB_TOKEN"] {
		match std::env::var(name) {
			Ok(token) if token.is_empty() => (),
			// Release discovery would silently drop a token it can't send in a header.
			Ok(token) => {
				ensure!(
					token.bytes().all(|b| b.is_ascii_graphic()),
					"{name} holds characters a token cannot"
				);
				return Ok(Some(token));
			}
			Err(std::env::VarError::NotPresent) => (),
			Err(_) => anyhow::bail!("{name} is not UTF-8"),
		}
	}
	Ok(None)
}

/// Whether GitHub accepts the token, asked once with the rate-limit endpoint, which spends no
/// quota. Anything but a 401 counts as accepted, leaving the error to the query that needed it.
pub async fn github_token_accepted() -> bool {
	static ACCEPTED: tokio::sync::OnceCell<bool> = tokio::sync::OnceCell::const_new();
	*ACCEPTED
		.get_or_init(|| async {
			let (Ok(Some(token)), Ok(client)) = (github_token(), client()) else {
				return true;
			};
			let response = client
				.get("https://api.github.com/rate_limit")
				.header("Accept", "application/vnd.github+json")
				.bearer_auth(token)
				// Leaves the 30 s discovery deadline room for the query this check precedes.
				.timeout(Duration::from_secs(10))
				.send()
				.await;
			!matches!(response, Ok(r) if r.status() == reqwest::StatusCode::UNAUTHORIZED)
		})
		.await
}

/// The error for a token GitHub rejects.
pub const REJECTED_TOKEN: &str = "GitHub rejected the GH_TOKEN or GITHUB_TOKEN token; replace or unset it";

// Spool responses to disk instead of retaining the sparse index or pages of bundles.
pub async fn spool(mut response: Response, directory: &std::path::Path) -> Result<tempfile::NamedTempFile> {
	let mut file = tempfile::NamedTempFile::new_in(directory)?;
	while let Some(chunk) = response.chunk().await? {
		file.write_all(&chunk)?;
	}
	Ok(file)
}

/// What an unpinned run executes, or why it cannot, and whether to start a background refresh either way.
pub struct Unpinned {
	pub record: Result<Record>,
	pub refresh: bool,
}

/// The release an unpinned run executes: the newest installed one `age` allows at `now` by the cached
/// index, else the newest one it allows, installed first. Only a cache without an index fetches it.
pub fn unpinned(cache: &Cache, name: &str, args: &Args, age: MinimumAge, now: SystemTime) -> Result<Unpinned> {
	report_refresh_error(cache)?;
	let index = match Index::load(cache)? {
		Some(index) => index,
		// Nothing cached says which release is old enough and not yanked.
		None => runtime()?.block_on(Index::fetch(cache, name, None, now))?,
	};
	let chosen = (|| -> Result<_> {
		let choice = index
			.choose(&cache.installed()?, age, now)
			.with_context(|| format!("choose a release of {name}"))?;
		let record = match &choice.installed {
			Some(version) => cache
				.load(version)?
				.with_context(|| format!("{} vanished", cache.version(version).display()))?,
			None => {
				let version = choice.newest.as_ref().ok_or_else(|| index.too_young(name, age))?;
				runtime()?.block_on(install::resolve(cache, name, version, args))?
			}
		};
		Ok((record, choice.newest))
	})();
	Ok(match chosen {
		Ok((record, newest)) => Unpinned {
			refresh: index.due(now) || newest.as_ref() != Some(&record.version),
			record: Ok(record),
		},
		// A stale index may be why nothing runs, so a due check still starts for the next run.
		Err(error) => Unpinned {
			refresh: index.due(now),
			record: Err(error),
		},
	})
}

/// The detached refresh: the daily index check when due, then installing the newest release `age`
/// allows, so the next unpinned run finds it.
pub async fn refresh(cache: &Cache, name: &str, args: &Args, age: MinimumAge) -> Result<()> {
	let Some(_lock) = cache.try_refresh_lock()? else {
		return Ok(());
	};
	let previous = Failure::load(cache)?;
	let now = SystemTime::now();
	if previous.as_ref().is_some_and(|failure| now < failure.retry_at) {
		return Ok(());
	}
	let mut pending = None;
	let result = async {
		let index = match Index::load(cache)? {
			Some(index) if !index.due(now) => index,
			// A failed check leaves the index due, so only the backoff above delays its retry.
			previous => Index::fetch(cache, name, previous, now).await?,
		};
		let Some(version) = index
			.newest(age, now)
			.with_context(|| format!("choose a release of {name}"))?
		else {
			return Ok(());
		};
		pending = Some(version.clone());
		install::resolve(cache, name, version, args).await?;
		Ok(())
	}
	.await;
	Failure::save(
		cache,
		Failure::next(previous, result.as_ref().err(), pending, SystemTime::now()).as_ref(),
	)?;
	result
}

/// A background refresh's last error, kept until a refresh succeeds.
// 0.1's `refresh.json` rejects unknown fields, so this state lives in a file of its own.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Failure {
	message: String,
	/// The release the refresh was installing, if it got that far.
	version: Option<Version>,
	reported: bool,
	delay: Duration,
	retry_at: SystemTime,
}

impl Failure {
	fn path(cache: &Cache) -> PathBuf {
		cache.root.join("failure.json")
	}

	fn load(cache: &Cache) -> Result<Option<Self>> {
		let path = Self::path(cache);
		let file = match File::open(&path) {
			Ok(file) => file,
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
			Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
		};
		serde_json::from_reader(file).with_context(|| format!("corrupt refresh state {}", path.display()))
	}

	fn save(cache: &Cache, failure: Option<&Self>) -> Result<()> {
		let path = Self::path(cache);
		match failure {
			Some(failure) => cache::atomic(&path, &serde_json::to_vec(failure)?),
			None => match fs::remove_file(&path) {
				Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error.into()),
				_ => Ok(()),
			},
		}
	}

	/// The state after a refresh that ended at `now`: none after a success, and a retry after 30
	/// seconds that doubles, up to 30 minutes, while the same error repeats for the same release.
	fn next(
		previous: Option<Self>,
		error: Option<&anyhow::Error>,
		version: Option<Version>,
		now: SystemTime,
	) -> Option<Self> {
		let message = format!("{:#}", error?);
		let previous = previous.filter(|failure| failure.message == message && failure.version == version);
		let delay = previous.as_ref().map_or(Duration::from_secs(30), |failure| {
			failure.delay.saturating_mul(2).min(Duration::from_secs(30 * 60))
		});
		Some(Self {
			message,
			version,
			reported: previous.is_some_and(|failure| failure.reported),
			delay,
			retry_at: now + delay,
		})
	}
}

pub fn spawn_refresh(spec: &Spec, args: &Args, age: MinimumAge) -> Result<()> {
	let caller = cargo::Caller::current()?;
	let mut command = Command::new(std::env::current_exe()?);
	command.env(crate::launcher::REFRESH, "1");
	// The updater outlives this invocation, so it must not keep the caller's directory in use.
	command.current_dir(cargo::root(&caller.dir));
	// The effective delay, whether it came from the flag, the environment or the default.
	command.arg("--minimum-release-age").arg(age.to_string());
	if args.build {
		command.arg("--build");
		// Resolved here, since the updater runs from another directory.
		cargo::Toolchain::new(&caller)?.env(&mut command);
	}
	if args.allow_quickinstall {
		command.arg("--allow-quickinstall");
	}
	command
		.arg(&spec.name)
		.stdin(Stdio::null())
		.stdout(Stdio::null())
		.stderr(Stdio::null());
	#[cfg(unix)]
	{
		use std::os::unix::process::CommandExt;
		// Reparent the updater so a long-running CLI never inherits a zombie.
		// Only async-signal-safe libc calls run between fork and exec.
		unsafe {
			command.pre_exec(|| {
				if libc::setsid() < 0 {
					return Err(std::io::Error::last_os_error());
				}
				match libc::fork() {
					-1 => Err(std::io::Error::last_os_error()),
					0 => Ok(()),
					_ => libc::_exit(0),
				}
			});
		}
	}
	// The forked updater outlives this short-lived child, whose exit reports the fork.
	#[cfg(unix)]
	ensure!(
		command.status().context("start sparse-index refresh")?.success(),
		"start sparse-index refresh failed"
	);
	#[cfg(windows)]
	{
		use std::os::windows::process::CommandExt;
		// DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP: no console, and no console Ctrl-C.
		command.creation_flags(0x00000008 | 0x00000200);
		command.spawn().context("start sparse-index refresh")?;
	}
	Ok(())
}

pub fn report_refresh_error(cache: &Cache) -> Result<()> {
	if let Some(error) = take_refresh_error(cache)? {
		eprintln!("crabx: the last background update failed: {error}");
	}
	Ok(())
}

fn take_refresh_error(cache: &Cache) -> Result<Option<String>> {
	if !cache.root.try_exists()? {
		return Ok(None);
	}
	let Some(_lock) = cache.try_refresh_lock()? else {
		return Ok(None);
	};
	let Some(mut failure) = Failure::load(cache)?.filter(|failure| !failure.reported) else {
		return Ok(None);
	};
	failure.reported = true;
	Failure::save(cache, Some(&failure))?;
	Ok(Some(failure.message))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn unpinned_runs_refresh_only_when_a_check_is_due_or_a_newer_release_qualifies() {
		use clap::Parser;
		let temp = tempfile::tempdir().unwrap();
		let cache = Cache {
			root: temp.path().to_owned(),
		};
		let entry = cache.version(&Version::new(1, 2, 3));
		fs::create_dir_all(entry.join("bin")).unwrap();
		fs::write(entry.join("bin/tool"), "fixture").unwrap();
		fs::write(
			entry.join("record.json"),
			r#"{"version":"1.2.3","bins":["tool"],"source":"fixture"}"#,
		)
		.unwrap();
		let checked = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
		let hour = Duration::from_secs(3600);
		fs::write(
			cache.root.join("index.json"),
			serde_json::to_vec(&serde_json::json!({
				"etag": null,
				"checked": checked,
				"releases": [
					{ "version": "1.3.0", "published": checked - hour },
					{ "version": "1.2.3", "published": checked - hour * 48 },
				],
			}))
			.unwrap(),
		)
		.unwrap();
		let args = Args::parse_from(["crabx", "fixture"]);
		let run = |age: &str, now| {
			let run = unpinned(&cache, "fixture", &args, age.parse().unwrap(), now).unwrap();
			(run.record.unwrap().version.to_string(), run.refresh)
		};
		// Neither a check nor an install is due.
		assert_eq!(run("24h", checked), ("1.2.3".into(), false));
		assert_eq!(run("24h", checked + hour * 22), ("1.2.3".into(), false));
		// 1.3.0 qualifies before the next check: install it in the background.
		assert_eq!(run("24h", checked + hour * 23), ("1.2.3".into(), true));
		assert_eq!(run("0s", checked), ("1.2.3".into(), true));
		// The daily check is due, whatever is installed.
		assert_eq!(run("2d", checked + hour * 24), ("1.2.3".into(), true));
		// A run that cannot choose still starts the due check, which may be what it lacks.
		let failed = |now| {
			let run = unpinned(&cache, "fixture", &args, "1w".parse().unwrap(), now).unwrap();
			(run.record.unwrap_err().to_string(), run.refresh)
		};
		let (error, refresh) = failed(checked);
		assert!(error.starts_with("no release of fixture is 1w old yet"), "{error}");
		assert!(!refresh);
		assert!(failed(checked + hour * 24).1);
	}

	#[test]
	fn failures_back_off_and_each_diagnostic_has_one_claim() {
		let temp = tempfile::tempdir().unwrap();
		let cache = Cache {
			root: temp.path().to_owned(),
		};
		let now = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
		let error = anyhow::anyhow!("no release for host");
		let failure = Failure::next(None, Some(&error), None, now).unwrap();
		assert_eq!(failure.retry_at, now + Duration::from_secs(30));
		Failure::save(&cache, Some(&failure)).unwrap();
		let lock = cache.try_refresh_lock().unwrap().unwrap();
		assert!(take_refresh_error(&cache).unwrap().is_none());
		drop(lock);
		assert_eq!(
			take_refresh_error(&cache).unwrap().as_deref(),
			Some("no release for host")
		);
		assert!(take_refresh_error(&cache).unwrap().is_none());
		// A repeat keeps its report and doubles its delay.
		let later = now + Duration::from_secs(30);
		let failure = Failure::next(Failure::load(&cache).unwrap(), Some(&error), None, later).unwrap();
		assert_eq!(failure.retry_at, later + Duration::from_secs(60));
		Failure::save(&cache, Some(&failure)).unwrap();
		assert!(take_refresh_error(&cache).unwrap().is_none());
		let mut failure = Some(failure);
		for _ in 0..20 {
			failure = Failure::next(failure, Some(&error), None, now);
		}
		assert_eq!(failure.as_ref().unwrap().delay, Duration::from_secs(1800));
		// A newly chosen release, or a different error, starts over and reports again.
		let failure = Failure::next(failure, Some(&error), Some(Version::new(2, 0, 0)), now).unwrap();
		assert_eq!(failure.delay, Duration::from_secs(30));
		assert!(!failure.reported);
		Failure::save(&cache, Some(&failure)).unwrap();
		assert!(take_refresh_error(&cache).unwrap().is_some());
		assert!(Failure::next(Some(failure), None, None, now).is_none());
		Failure::save(&cache, None).unwrap();
		assert!(Failure::load(&cache).unwrap().is_none());
		Failure::save(&cache, None).unwrap();
	}
}
