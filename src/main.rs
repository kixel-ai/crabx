mod cache;
mod cli;
#[path = "../vendor/binstalk-fetchers/src/lib.rs"]
#[allow(dead_code)] // The imported library exposes more than the runner consumes.
mod fetchers;
mod init;
mod install;
mod launcher;
mod network;
#[path = "../vendor/binstalk-target.rs"]
mod target;
mod trust;

use anyhow::{Context, Result};
use clap::Parser;
use std::{path::PathBuf, process::Command};

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
	let options = args.build || args.allow_quickinstall || args.bin.is_some();
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
	let cache = cache::Cache::new(&spec, args.mode())?;
	if refresh {
		network::runtime()?.block_on(network::refresh(&cache, &spec, &args))?;
		return Ok(());
	}
	if spec.version.is_none() {
		network::report_refresh_error(&cache)?;
	}
	let version = match &spec.version {
		Some(version) => Some(version.clone()),
		None => cache.latest()?,
	};
	let cached = version.as_ref().map(|v| cache.load(v)).transpose()?.flatten();
	let record = match cached {
		Some(record) => record,
		None => network::runtime()?.block_on(install::resolve(&cache, &spec, &args))?,
	};
	let binary = record.binary(args.bin.as_deref(), &spec.name)?;
	let path = cache.version(&record.version).join("bin").join(binary);
	if spec.version.is_none() {
		// The updater owns no terminal handles and never delays this invocation.
		if let Err(error) = network::spawn_refresh(&spec, &args) {
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
