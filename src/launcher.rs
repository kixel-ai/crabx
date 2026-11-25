use crate::{cache, cli, install, network};
use anyhow::{Context, Result, bail, ensure};
use clap::{CommandFactory, FromArgMatches, Parser};
use serde::{Deserialize, Serialize};
use std::{
	ffi::OsString,
	fs, io,
	path::{Path, PathBuf},
};

/// Marks the background refresh crabx starts, which can start under a launcher's name: Windows
/// launchers are links to crabx.exe, and macOS reports a symlink's own path as the executable.
pub const REFRESH: &str = "CRABX_REFRESH";

const EXE: &str = std::env::consts::EXE_SUFFIX;

/// A launcher `crabx install` put on PATH, recorded under its name.
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Launcher {
	/// The launcher's file.
	path: PathBuf,
	/// crabx's options, then the `CRATE[@VERSION]` the launcher runs.
	args: Vec<String>,
}

/// What a launcher's path holds.
#[derive(PartialEq)]
enum Found {
	Missing,
	Launcher,
	Other,
}

/// The launcher crabx was started as, or None when it was started as crabx.
pub fn name() -> Result<Option<String>> {
	let Some(arg0) = std::env::args_os().next() else {
		return Ok(None);
	};
	let path = Path::new(&arg0);
	// Windows names are case-insensitive, so `CRABX.EXE` is crabx too.
	let name = match path.extension() {
		Some(extension) if cfg!(windows) && extension.eq_ignore_ascii_case("exe") => path.file_stem(),
		_ => path.file_name(),
	}
	.unwrap_or_default();
	if name.eq_ignore_ascii_case("crabx") {
		return Ok(None);
	}
	let name = name
		.to_str()
		.with_context(|| format!("crabx was started under the non-UTF-8 name {name:?}"))?;
	Ok(Some(name.to_ascii_lowercase()))
}

/// The crabx arguments that run the launcher `name`, passing `args` to its crate.
pub fn args(name: &str, args: impl Iterator<Item = OsString>) -> Result<cli::Args> {
	let launcher = match cli::Spec::parse(name) {
		Ok(spec) if spec.version.is_none() => Launcher::load(name)?,
		_ => None,
	}
	.with_context(|| {
		format!(
			"crabx was started as {name:?}, which is no launcher `crabx install` recorded; run crabx as crabx, or install a crate's launcher with `crabx install CRATE`"
		)
	})?;
	let argv = std::iter::once("crabx".into())
		.chain(launcher.args.into_iter().map(OsString::from))
		.chain(args);
	Ok(cli::Args::try_parse_from(argv)?)
}

/// Run `crabx install [OPTIONS] [CRATE[@VERSION]]`, given the arguments after `install`.
pub fn install(args: Vec<OsString>) -> Result<()> {
	let dir = directory()?;
	let Some(last) = args.last() else {
		install_self(&dir)?;
		return on_path(&dir);
	};
	let command = cli::Args::command()
		.override_usage("crabx install [OPTIONS] [CRATE[@VERSION]]")
		.about("Put crabx on PATH, and with a crate, a launcher that runs it through crabx.")
		.mut_arg("command", |arg| arg.help("A crates.io crate, optionally pinned."));
	let parsed = cli::Args::from_arg_matches(
		&command.get_matches_from(std::iter::once(OsString::from("crabx install")).chain(args.iter().cloned())),
	)?;
	ensure!(
		parsed.command.len() == 1,
		"crabx install takes options and then one crate; the launcher passes the arguments it is started with to the crate"
	);
	let arg = |arg: &OsString| arg.to_str().map(str::to_owned).context("crabx install's arguments must be UTF-8");
	let krate = arg(last)?;
	let spec = cli::Spec::parse(&krate)?;
	ensure!(
		!spec.name.eq_ignore_ascii_case("crabx"),
		"crabx is the runner's own name; `crabx install` installs crabx itself, and `crabx crabx@VERSION` runs a published crabx"
	);
	let name = spec.name.to_ascii_lowercase();
	let path = dir.join(format!("{name}{EXE}"));
	let existing = Launcher::load(&name)?;
	if let Some(existing) = &existing
		&& existing.path != path
		&& existing.found()? == Found::Launcher
	{
		bail!(
			"{name} already has a launcher at {}; delete it to install {name} into {}",
			existing.path.display(),
			dir.display()
		);
	}
	let recorded = existing.as_ref().is_some_and(|existing| existing.path == path);
	ensure!(
		found(&path, recorded)? != Found::Other,
		"{} exists and is not crabx's launcher; delete it, or set CRABX_INSTALL_DIR to install elsewhere",
		path.display()
	);
	// A typo, a crate with no usable release, or an ambiguous binary fails now, not on the first run.
	let cache = cache::Cache::new(&spec, parsed.mode())?;
	let record = network::runtime()?.block_on(install::resolve(&cache, &spec, &parsed))?;
	record.binary(parsed.bin.as_deref(), &spec.name)?;
	install_self(&dir)?;
	// Recorded before linking: a record whose launcher is missing counts as uninstalled.
	let launcher = Launcher {
		path,
		args: args.iter().map(arg).collect::<Result<_>>()?,
	};
	launcher.save(&name)?;
	link(&dir, &launcher.path)?;
	eprintln!(
		"crabx install: installed {}, which runs {krate} through crabx",
		launcher.path.display()
	);
	on_path(&dir)
}

/// `CRABX_INSTALL_DIR`, else `~/.local/bin`, as install.sh and install.ps1 use.
fn directory() -> Result<PathBuf> {
	let dir = match std::env::var_os("CRABX_INSTALL_DIR").filter(|dir| !dir.is_empty()) {
		Some(dir) => PathBuf::from(dir),
		None => directories::BaseDirs::new()
			.context("cannot find the user's home directory; set CRABX_INSTALL_DIR")?
			.home_dir()
			.join(".local")
			.join("bin"),
	};
	// Launchers are recorded by path, so a relative directory must not depend on the working directory.
	Ok(std::path::absolute(dir)?)
}

/// Where launcher records live: the state directory on Linux, local data elsewhere.
fn records() -> Result<PathBuf> {
	let base = directories::BaseDirs::new().context("cannot find the user's home directory")?;
	Ok(base
		.state_dir()
		.unwrap_or(base.data_local_dir())
		.join("crabx")
		.join("launchers"))
}

impl Launcher {
	fn record(name: &str) -> Result<PathBuf> {
		Ok(records()?.join(format!("{name}.json")))
	}

	fn load(name: &str) -> Result<Option<Self>> {
		let path = Self::record(name)?;
		let file = match fs::File::open(&path) {
			Ok(file) => file,
			Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
			Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
		};
		let launcher: Self = serde_json::from_reader(file)
			.with_context(|| format!("corrupt launcher record {}; delete it and reinstall", path.display()))?;
		Ok(Some(launcher))
	}

	/// Every recorded launcher, by name.
	fn all() -> Result<Vec<(String, Self)>> {
		let dir = records()?;
		let entries = match fs::read_dir(&dir) {
			Ok(entries) => entries,
			Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
			Err(error) => return Err(error).with_context(|| format!("read {}", dir.display())),
		};
		let mut launchers = Vec::new();
		for entry in entries {
			let file = entry?.file_name();
			// Skips an interrupted write's temporary file.
			if let Some(name) = file.to_str().and_then(|file| file.strip_suffix(".json")) {
				let launcher = Self::load(name)?.with_context(|| format!("{name}'s launcher record vanished"))?;
				launchers.push((name.to_owned(), launcher));
			}
		}
		Ok(launchers)
	}

	fn save(&self, name: &str) -> Result<()> {
		cache::atomic(&Self::record(name)?, &serde_json::to_vec_pretty(self)?)
	}

	fn forget(name: &str) -> Result<()> {
		let path = Self::record(name)?;
		match fs::remove_file(&path) {
			Err(error) if error.kind() != io::ErrorKind::NotFound => {
				Err(error).with_context(|| format!("delete {}", path.display()))
			}
			_ => Ok(()),
		}
	}

	fn found(&self) -> Result<Found> {
		found(&self.path, true)
	}
}

/// What `path` holds. A Unix launcher is a symlink to `crabx` beside it. A Windows one is a hard
/// link that a replaced crabx.exe no longer shares, so only its record identifies it.
fn found(path: &Path, recorded: bool) -> Result<Found> {
	let metadata = match fs::symlink_metadata(path) {
		Ok(metadata) => metadata,
		Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Found::Missing),
		Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
	};
	let launcher = if cfg!(unix) {
		metadata.is_symlink() && fs::read_link(path)? == Path::new("crabx")
	} else {
		recorded && metadata.is_file()
	};
	Ok(if launcher { Found::Launcher } else { Found::Other })
}

/// Put this crabx in `dir`, then bring every launcher there up to date with it.
fn install_self(dir: &Path) -> Result<()> {
	fs::create_dir_all(dir).with_context(|| format!("create {}", dir.display()))?;
	#[cfg(windows)]
	sweep(dir)?;
	let target = dir.join(format!("crabx{EXE}"));
	let exe = std::env::current_exe()?;
	if fs::canonicalize(&exe)? != fs::canonicalize(&target).unwrap_or_default() {
		// npx's copy is temporary, so a launcher can't link to it.
		let copy = tempfile::Builder::new()
			.prefix(".crabx-")
			.make_in(dir, |path| fs::copy(&exe, path))
			.with_context(|| format!("copy crabx into {}", dir.display()))?;
		replace(copy, &target)?;
		eprintln!("crabx install: installed {}", target.display());
	}
	for (name, launcher) in Launcher::all()? {
		if launcher.path.parent() != Some(dir) {
			continue;
		}
		match launcher.found()? {
			// Deleting a launcher uninstalls it.
			Found::Missing => Launcher::forget(&name)?,
			// A Windows launcher is a link to the crabx.exe it was made from.
			Found::Launcher if cfg!(windows) => link(dir, &launcher.path)?,
			Found::Launcher => {}
			Found::Other => {
				eprintln!(
					"crabx install: {} is no longer crabx's launcher; forgetting it",
					launcher.path.display()
				);
				Launcher::forget(&name)?;
			}
		}
	}
	Ok(())
}

/// Point the launcher at `path` to the crabx in `dir`.
fn link(dir: &Path, path: &Path) -> Result<()> {
	#[cfg(unix)]
	let link = |temp: &Path| std::os::unix::fs::symlink("crabx", temp);
	// A hard link starts no shell, as a script would, so Ctrl-C asks nothing.
	#[cfg(windows)]
	let link = |temp: &Path| fs::hard_link(dir.join("crabx.exe"), temp);
	let temp = tempfile::Builder::new()
		.prefix(".crabx-")
		.make_in(dir, link)
		.with_context(|| format!("link {} to crabx", path.display()))?;
	replace(temp, path)
}

/// Rename `file` over `target`, so a running crabx keeps its old file.
fn replace<F>(file: tempfile::NamedTempFile<F>, target: &Path) -> Result<()> {
	#[cfg(windows)]
	if fs::symlink_metadata(target).is_ok() {
		// Windows can't replace a running program, but can rename it out of the way.
		let dir = target.parent().context("a launcher has no directory")?;
		let old = tempfile::Builder::new()
			.prefix(".crabx-old-")
			.make_in(dir, |path| fs::rename(target, path))
			.with_context(|| format!("move {} aside", target.display()))?
			.into_temp_path();
		if let Err(error) = file.persist(target) {
			old.persist(target).map_err(|error| error.error)?;
			return Err(error.error).with_context(|| format!("write {}", target.display()));
		}
		// Dropping deletes it; one still running stays for the next install's sweep.
		return Ok(());
	}
	file.persist(target)
		.map_err(|error| error.error)
		.with_context(|| format!("write {}", target.display()))?;
	Ok(())
}

/// Delete the programs earlier installs moved aside, except those still running.
#[cfg(windows)]
fn sweep(dir: &Path) -> Result<()> {
	for entry in fs::read_dir(dir)? {
		let entry = entry?;
		if entry.file_name().to_string_lossy().starts_with(".crabx-old-") {
			// A program still running can't be deleted; a later install retries.
			let _ = fs::remove_file(entry.path());
		}
	}
	Ok(())
}

/// Say how to put `dir` on PATH, or on Windows, add it to the user's `Path`, as the installers do.
fn on_path(dir: &Path) -> Result<()> {
	#[cfg(unix)]
	if !std::env::var_os("PATH").is_some_and(|path| std::env::split_paths(&path).any(|entry| entry == dir)) {
		eprintln!(
			"crabx install: add {} to PATH to run crabx and the crates it installs",
			dir.display()
		);
	}
	#[cfg(windows)]
	{
		// .NET writes the user's Path and tells running programs, such as Explorer, it changed.
		const SCRIPT: &str = "$ErrorActionPreference = 'Stop'
$dir = $env:CRABX_PATH_DIR
$path = [Environment]::GetEnvironmentVariable('Path', 'User')
if (($path -split ';') -notcontains $dir) {
	[Environment]::SetEnvironmentVariable('Path', ((@($path, $dir) | Where-Object { $_ }) -join ';'), 'User')
	'added'
}";
		let output = std::process::Command::new("powershell")
			.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT])
			.env("CRABX_PATH_DIR", dir)
			.stdin(std::process::Stdio::null())
			.stderr(std::process::Stdio::inherit())
			.output()
			.context("run PowerShell to add crabx's directory to your Path")?;
		ensure!(output.status.success(), "could not add {} to your Path", dir.display());
		if output.stdout.starts_with(b"added") {
			eprintln!(
				"crabx install: added {} to your Path; new terminals will find crabx and the crates it installs",
				dir.display()
			);
		}
	}
	Ok(())
}
