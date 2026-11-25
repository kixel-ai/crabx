mod cache;
mod cli;
#[path = "../vendor/binstalk-fetchers/src/lib.rs"]
#[allow(dead_code)] // The imported library exposes more than the runner consumes.
mod fetchers;
mod index;
mod init;
mod install;
mod launcher;
mod network;
mod origin;
#[path = "../vendor/binstalk-target.rs"]
mod target;
mod trust;

use anyhow::{Context, Result};
use clap::Parser;
use std::{path::PathBuf, process::Command, time::SystemTime};

fn main() {
	let refresh = std::env::var_os(launcher::REFRESH).is_some();
	// SAFETY: no other thread exists yet to read the environment. Cleared so nothing crabx runs inherits it.
	unsafe { std::env::remove_var(launcher::REFRESH) };
	if let Err(error) = start(refresh) {
		eprintln!("crabx: {error:#}");
		std::process::exit(1);
	}
}

fn start(refresh: bool) -> Result<()> {
	// The refresh marker wins over the name, which a launcher's refresh can carry.
	if !refresh && let Some(name) = launcher::name()? {
		return run(launcher::args(&name, std::env::args_os().skip(1))?, false);
	}
	let args = cli::Args::parse();
	// crabx's own commands; `init@VERSION` and `install@VERSION` still run the crates so named.
	let command = args.command.first().and_then(|arg| arg.to_str()).unwrap_or_default();
	let options = args.build || args.allow_quickinstall || args.bin.is_some() || args.minimum_release_age.is_some();
	if !refresh && command.eq_ignore_ascii_case("init") {
		anyhow::ensure!(
			args.command.len() == 1 && !options,
			"crabx init takes no options or arguments; run the crate named init as init@VERSION"
		);
		return init::run();
	}
	if !refresh && command.eq_ignore_ascii_case("install") {
		anyhow::ensure!(
			!options,
			"crabx install takes its options after install; run the crate named install as install@VERSION"
		);
		return launcher::install(args.command.into_iter().skip(1).collect());
	}
	run(args, refresh)
}

fn run(args: cli::Args, refresh: bool) -> Result<()> {
	let krate = args
		.command
		.first()
		.and_then(|arg| arg.to_str())
		.context("the crate name must be UTF-8")?;
	let spec = cli::Spec::parse(krate)?;
	// Checked on pinned runs too, which ignore it, so a malformed value never passes silently.
	let age = args.minimum_release_age()?;
	let cache = cache::Cache::new(&spec, args.mode())?;
	if refresh {
		anyhow::ensure!(spec.version.is_none(), "only unpinned crates can refresh");
		network::runtime()?.block_on(network::refresh(&cache, &spec.name, &args, age))?;
		return Ok(());
	}
	let record = match &spec.version {
		// A pin reads no index and, once cached, makes no request.
		Some(version) => match cache.load(version)? {
			Some(record) => record,
			None => network::runtime()?.block_on(install::resolve(&cache, &spec.name, version, &args))?,
		},
		None => {
			let run = network::unpinned(&cache, &spec.name, &args, age, SystemTime::now())?;
			// The updater owns no terminal handles and never delays this invocation.
			if run.refresh
				&& let Err(error) = network::spawn_refresh(&spec, &args, age)
			{
				eprintln!("crabx: could not start background update: {error:#}");
			}
			run.record?
		}
	};
	let binary = record.binary(args.bin.as_deref(), &spec.name)?;
	let path = cache.version(&record.version).join("bin").join(binary);
	execute(path, args.command.into_iter().skip(1).collect())
}

fn execute(path: PathBuf, args: Vec<std::ffi::OsString>) -> Result<()> {
	let mut command = Command::new(&path);
	command.args(args);
	#[cfg(unix)]
	{
		use std::os::unix::process::CommandExt;
		Err(command.exec()).with_context(|| format!("execute {}", path.display()))
	}
	#[cfg(windows)]
	{
		let status = command
			.status()
			.with_context(|| format!("execute {}", path.display()))?;
		std::process::exit(status.code().unwrap_or(1));
	}
}
