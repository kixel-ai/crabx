mod cache;
mod cli;
#[path = "../vendor/binstalk-fetchers/src/lib.rs"]
#[allow(dead_code)] // The imported library exposes more than the runner consumes.
mod fetchers;
mod index;
mod init;
mod install;
mod network;
#[path = "../vendor/binstalk-target.rs"]
mod target;
mod trust;

use anyhow::{Context, Result};
use clap::Parser;
use std::{path::PathBuf, process::Command, time::SystemTime};

fn main() {
	if let Err(error) = run(cli::Args::parse()) {
		eprintln!("crabx: {error:#}");
		std::process::exit(1);
	}
}

fn run(args: cli::Args) -> Result<()> {
	let krate = args
		.command
		.first()
		.and_then(|arg| arg.to_str())
		.context("the crate name must be UTF-8")?;
	// `init` is crabx's own command; `init@VERSION` still runs the crate named init.
	if krate.eq_ignore_ascii_case("init") {
		anyhow::ensure!(
			args.command.len() == 1
				&& !args.build
				&& !args.allow_quickinstall
				&& args.bin.is_none()
				&& args.minimum_release_age.is_none()
				&& !args.refresh,
			"crabx init takes no options or arguments; run the crate named init as init@VERSION"
		);
		return init::run();
	}
	let spec = cli::Spec::parse(krate)?;
	// Checked on pinned runs too, which ignore it, so a malformed value never passes silently.
	let age = args.minimum_release_age()?;
	let cache = cache::Cache::new(&spec, args.mode())?;
	if args.refresh {
		anyhow::ensure!(spec.version.is_none(), "only unpinned crates can refresh");
		network::runtime()?.block_on(network::refresh(&cache, &spec.name, &args, age))?;
		return Ok(());
	}
	let (record, refresh) = match &spec.version {
		// A pin reads no index and, once cached, makes no request.
		Some(version) => match cache.load(version)? {
			Some(record) => (record, false),
			None => (
				network::runtime()?.block_on(install::resolve(&cache, &spec.name, version, &args))?,
				false,
			),
		},
		None => {
			let run = network::unpinned(&cache, &spec.name, &args, age, SystemTime::now())?;
			(run.record, run.refresh)
		}
	};
	let binary = record.binary(args.bin.as_deref(), &spec.name)?;
	let path = cache.version(&record.version).join("bin").join(binary);
	if refresh {
		// The updater owns no terminal handles and never delays this invocation.
		if let Err(error) = network::spawn_refresh(&spec, &args, age) {
			eprintln!("crabx: could not start background update: {error:#}");
		}
	}
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
