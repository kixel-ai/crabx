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
	/// On Windows, the SHA-256 of the crabx.exe the launcher links to, which tells it apart from a
	/// program installed over it later. While a relink is under way, the old and new ones.
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	sha256: Vec<String>,
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
		let _lock = Launcher::lock()?;
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
	let arg = |arg: &OsString| {
		arg.to_str()
			.map(str::to_owned)
			.context("crabx install's arguments must be UTF-8")
	};
	let krate = arg(last)?;
	let spec = cli::Spec::parse(&krate)?;
	ensure!(
		!spec.name.eq_ignore_ascii_case("crabx"),
		"crabx is the runner's own name; `crabx install` installs crabx itself, and `crabx crabx@VERSION` runs a published crabx"
	);
	let name = spec.name.to_ascii_lowercase();
	let path = dir.join(format!("{name}{EXE}"));
	// Checked before resolving too, so a conflict fails before a long --build. Under the lock, so
	// a concurrent relink can't pair a new link with its old digest; released before resolving.
	Launcher::lock().and_then(|_lock| check(&dir, &name, &path))?;
	// A typo, a crate with no usable release, or an ambiguous binary fails now, not on the first run.
	let cache = cache::Cache::new(&spec, parsed.mode())?;
	let record = network::runtime()?.block_on(install::resolve(&cache, &spec, &parsed))?;
	record.binary(parsed.bin.as_deref(), &spec.name)?;
	// One install at a time, so each record names the file its launcher holds. Taken only after
	// resolving, so a build script that runs `crabx install` can't wait on its own parent.
	let _lock = Launcher::lock()?;
	check(&dir, &name, &path)?;
	install_self(&dir)?;
	// Read after install_self, whose relink changes a Windows launcher's digest.
	let sha256 = Launcher::load(&name)?
		.filter(|recorded| same_file(&recorded.path, &path))
		.map(|recorded| recorded.sha256)
		.unwrap_or_default();
	let mut launcher = Launcher {
		// Resolved, so a later install finds it through any alias, even once this one is gone.
		path: fs::canonicalize(&dir)?.join(path.file_name().context("a launcher has no name")?),
		args: args.iter().map(arg).collect::<Result<_>>()?,
		sha256,
	};
	link(&dir, &name, &mut launcher)?;
	eprintln!(
		"crabx install: installed {}, which runs {krate} through crabx",
		launcher.path.display()
	);
	on_path(&dir)
}

/// Fail if installing `name` at `path` would replace another launcher or a file crabx doesn't own.
fn check(dir: &Path, name: &str, path: &Path) -> Result<()> {
	let existing = Launcher::load(name)?;
	if let Some(existing) = &existing
		&& !same_file(&existing.path, path)
		&& existing.found()? == Found::Launcher
	{
		bail!(
			"{name} already has a launcher at {}; delete it to install {name} into {}",
			existing.path.display(),
			dir.display()
		);
	}
	let recorded = existing.as_ref().filter(|existing| same_file(&existing.path, path));
	ensure!(
		found(path, recorded)? != Found::Other,
		"{} exists and is not crabx's launcher; delete it, or set CRABX_INSTALL_DIR to install elsewhere",
		path.display()
	);
	Ok(())
}

/// Whether two paths name the same directory, such as through a different case on NTFS or a symlink.
fn same_dir(a: &Path, b: &Path) -> bool {
	a == b || fs::canonicalize(a).is_ok_and(|a| fs::canonicalize(b).is_ok_and(|b| a == b))
}

/// Whether two paths name the same file in the same directory, even when the file is missing.
fn same_file(a: &Path, b: &Path) -> bool {
	a.file_name() == b.file_name() && a.parent().zip(b.parent()).is_some_and(|(a, b)| same_dir(a, b))
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
		found(&self.path, Some(self))
	}

	/// The lock every install holds while it changes launchers and their records.
	fn lock() -> Result<fs::File> {
		let dir = records()?;
		fs::create_dir_all(&dir).with_context(|| format!("create {}", dir.display()))?;
		let file = fs::File::options()
			.create(true)
			.truncate(false)
			.write(true)
			.open(dir.join("lock"))?;
		file.lock()?;
		Ok(file)
	}
}

/// What `path` holds, given the record naming it. A Unix launcher is a symlink to `crabx` beside
/// it. A Windows one is a hard link that a replaced crabx.exe no longer shares, so it is the file
/// whose SHA-256 its record holds.
fn found(path: &Path, record: Option<&Launcher>) -> Result<Found> {
	let metadata = match fs::symlink_metadata(path) {
		Ok(metadata) => metadata,
		Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Found::Missing),
		Err(error) => return Err(error).with_context(|| format!("inspect {}", path.display())),
	};
	let launcher = if cfg!(unix) {
		metadata.is_symlink() && fs::read_link(path)? == Path::new("crabx")
	} else {
		match record.filter(|record| !record.sha256.is_empty()) {
			Some(record) => metadata.is_file() && record.sha256.contains(&sha256(path)?),
			None => false,
		}
	};
	Ok(if launcher { Found::Launcher } else { Found::Other })
}

fn sha256(path: &Path) -> Result<String> {
	use sha2::Digest;
	use std::io::Read;
	let mut file = fs::File::open(path).with_context(|| format!("read {}", path.display()))?;
	let mut hasher = sha2::Sha256::new();
	let mut buffer = vec![0; 1 << 16];
	loop {
		match file.read(&mut buffer)? {
			0 => break,
			read => hasher.update(&buffer[..read]),
		}
	}
	Ok(hasher.finalize().iter().map(|byte| format!("{byte:02x}")).collect())
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
	for (name, mut launcher) in Launcher::all()? {
		if !launcher.path.parent().is_some_and(|parent| same_dir(parent, dir)) {
			continue;
		}
		match launcher.found()? {
			// Deleting a launcher uninstalls it.
			Found::Missing => Launcher::forget(&name)?,
			// A Windows launcher is a link to the crabx.exe it was made from.
			Found::Launcher if cfg!(windows) => link(dir, &name, &mut launcher)?,
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

/// Point `launcher` at the crabx in `dir`, recording it as `name` first: a record whose launcher
/// is missing counts as uninstalled.
fn link(dir: &Path, name: &str, launcher: &mut Launcher) -> Result<()> {
	#[cfg(unix)]
	let link = |temp: &Path| std::os::unix::fs::symlink("crabx", temp);
	// A hard link starts no shell, as a script would, so Ctrl-C asks nothing.
	#[cfg(windows)]
	let link = |temp: &Path| fs::hard_link(dir.join("crabx.exe"), temp);
	let temp = tempfile::Builder::new()
		.prefix(".crabx-")
		.make_in(dir, link)
		.with_context(|| format!("link {} to crabx", launcher.path.display()))?;
	if cfg!(unix) {
		launcher.save(name)?;
		return replace(temp, &launcher.path);
	}
	// The link's own digest, whichever crabx.exe it caught. The record recognizes the old one
	// too until the link replaces it, so an interrupted relink leaves either file recognized.
	let digest = sha256(temp.path())?;
	if !launcher.sha256.contains(&digest) {
		launcher.sha256.push(digest.clone());
	}
	launcher.save(name)?;
	replace(temp, &launcher.path)?;
	launcher.sha256 = vec![digest];
	launcher.save(name)
}

/// Rename `file` over `target`, so a running crabx keeps its old file.
fn replace<F>(file: tempfile::NamedTempFile<F>, target: &Path) -> Result<()> {
	#[cfg(windows)]
	if fs::symlink_metadata(target).is_ok() {
		// Windows can't replace a running program, but can rename it out of the way.
		let old = target.parent().context("a launcher has no directory")?.join(OLD);
		fs::create_dir_all(&old).with_context(|| format!("create {}", old.display()))?;
		let old = tempfile::Builder::new()
			.make_in(&old, |path| fs::rename(target, path))
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

/// The directory, inside the install directory so a rename stays on its volume, where Windows
/// installs move replaced programs aside.
#[cfg(windows)]
const OLD: &str = ".crabx-old";

/// Delete the programs earlier installs moved aside, except those still running.
#[cfg(windows)]
fn sweep(dir: &Path) -> Result<()> {
	let old = dir.join(OLD);
	let entries = match fs::read_dir(&old) {
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
		entries => entries.with_context(|| format!("read {}", old.display()))?,
	};
	for entry in entries {
		// A program still running can't be deleted; a later install retries.
		let _ = fs::remove_file(entry?.path());
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
		// As install.ps1 does: the registry's raw value keeps entries such as %JAVA_HOME%\bin
		// expandable, and setting a user variable through .NET tells running programs, such as
		// Explorer, that the environment changed.
		const SCRIPT: &str = "$ErrorActionPreference = 'Stop'
$dir = $env:CRABX_PATH_DIR
$key = [Microsoft.Win32.Registry]::CurrentUser.CreateSubKey('Environment')
$path = $key.GetValue('Path', '', [Microsoft.Win32.RegistryValueOptions]::DoNotExpandEnvironmentNames)
if (($path -split ';') -notcontains $dir) {
	$kind = if ($path) { $key.GetValueKind('Path') } else { [Microsoft.Win32.RegistryValueKind]::ExpandString }
	$key.SetValue('Path', ((@($path, $dir) | Where-Object { $_ }) -join ';'), $kind)
	[Environment]::SetEnvironmentVariable('CRABX_PATH_CHANGED', '1', 'User')
	[Environment]::SetEnvironmentVariable('CRABX_PATH_CHANGED', $null, 'User')
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

#[cfg(test)]
mod tests {
	#[test]
	fn sha256_is_lowercase_hex() {
		let temp = tempfile::tempdir().unwrap();
		let path = temp.path().join("abc");
		std::fs::write(&path, "abc").unwrap();
		assert_eq!(
			super::sha256(&path).unwrap(),
			"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
		);
	}

	#[cfg(unix)]
	#[test]
	fn same_file_sees_through_directory_aliases() {
		let temp = tempfile::tempdir().unwrap();
		let real = temp.path().join("bin");
		std::fs::create_dir(&real).unwrap();
		let alias = temp.path().join("alias");
		std::os::unix::fs::symlink(&real, &alias).unwrap();
		// The file itself need not exist: a deleted launcher still has its record's path.
		assert!(super::same_file(&real.join("moq-cli"), &alias.join("moq-cli")));
		assert!(!super::same_file(&real.join("moq-cli"), &alias.join("moq-relay")));
		assert!(!super::same_file(&real.join("moq-cli"), &temp.path().join("moq-cli")));
	}
}
