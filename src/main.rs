mod archive;
mod cache;
mod cargo;
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
mod ratchet;
mod registry;
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
	let options = args.build
		|| args.allow_quickinstall
		|| args.require_attestation
		|| args.bin.is_some()
		|| args.minimum_release_age.is_some();
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
	// Taken after the refresh starts, so the updater never inherits it.
	let admitted = cache.ratchet.admit(&record, args.require_attestation)?;
	execute(path, args.command.into_iter().skip(1).collect(), admitted)
}

/// Run `path`, holding `admitted`, the ratchet's lock, until the binary has replaced or started
/// beside crabx.
fn execute(path: PathBuf, args: Vec<std::ffi::OsString>, admitted: Option<std::fs::File>) -> Result<()> {
	let mut command = Command::new(&path);
	command.args(args);
	#[cfg(unix)]
	{
		use std::os::unix::process::CommandExt;
		// The lock's descriptor is close-on-exec, so the kernel releases it as the binary replaces crabx.
		let error = command.exec();
		drop(admitted);
		Err(error).with_context(|| format!("execute {}", path.display()))
	}
	#[cfg(windows)]
	{
		// Windows sends Ctrl-C to every process on the console; crabx keeps waiting so the prompt
		// returns only once the crate has handled it and exited.
		console::wait_out_ctrl_c().context("handle Ctrl-C while the crate runs")?;
		let child = command.spawn().with_context(|| format!("execute {}", path.display()));
		drop(admitted);
		let status = child?.wait().with_context(|| format!("wait for {}", path.display()))?;
		std::process::exit(status.code().unwrap_or(1));
	}
}

#[cfg(windows)]
mod console {
	type Handler = unsafe extern "system" fn(u32) -> i32;

	#[link(name = "kernel32")]
	unsafe extern "system" {
		fn SetConsoleCtrlHandler(handler: Option<Handler>, add: i32) -> i32;
	}

	/// Handle Ctrl-C and Ctrl-Break, the events the crate also receives, by ignoring them.
	unsafe extern "system" fn ignore(event: u32) -> i32 {
		// CTRL_C_EVENT and CTRL_BREAK_EVENT; closing the console or logging off still ends crabx.
		(event <= 1) as i32
	}

	/// Keep crabx alive through Ctrl-C. Not `SetConsoleCtrlHandler(NULL, TRUE)`: children
	/// inherit that, so the crate would ignore Ctrl-C too.
	pub fn wait_out_ctrl_c() -> std::io::Result<()> {
		// SAFETY: `ignore` is a valid handler for the life of the process.
		match unsafe { SetConsoleCtrlHandler(Some(ignore), 1) } {
			0 => Err(std::io::Error::last_os_error()),
			_ => Ok(()),
		}
	}
}
