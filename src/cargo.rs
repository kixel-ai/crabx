//! Running Cargo for `--build` away from the caller's directory, which may be an untrusted checkout.
// A registry `cargo install` reads `.cargo/config.toml` and `.cargo/config` in CARGO_HOME and its
// ancestors, and rustup picks the toolchain from `rust-toolchain.toml` in its working directory and
// ancestors, so Cargo runs from the filesystem root and a CARGO_HOME whose path holds no other
// configuration.

use anyhow::{Context, Result, bail, ensure};
use std::{
	ffi::{OsStr, OsString},
	fs,
	path::{Component, Path, PathBuf},
	process::Command,
};

/// The caller's directory and the environment that chooses Cargo.
pub struct Caller {
	pub dir: PathBuf,
	pub cargo: Option<OsString>,
	pub cargo_home: Option<OsString>,
	pub path: Option<OsString>,
	pub home: Option<PathBuf>,
}

impl Caller {
	pub fn current() -> Result<Self> {
		Ok(Self {
			dir: std::env::current_dir().context("read the current directory")?,
			cargo: std::env::var_os("CARGO"),
			cargo_home: std::env::var_os("CARGO_HOME"),
			path: std::env::var_os("PATH"),
			home: directories::BaseDirs::new().map(|base| base.home_dir().to_owned()),
		})
	}
}

/// Cargo, CARGO_HOME, and PATH resolved against the caller's directory, and checked to load no
/// configuration a checkout could have written.
#[derive(Debug)]
pub struct Toolchain {
	program: PathBuf,
	home: PathBuf,
	path: Option<OsString>,
	dir: PathBuf,
}

impl Toolchain {
	pub fn new(caller: &Caller) -> Result<Self> {
		let path = caller
			.path
			.as_ref()
			.map(|path| std::env::join_paths(std::env::split_paths(path).map(|entry| absolute(&caller.dir, &entry))))
			.transpose()
			.context("PATH holds an entry that cannot be rewritten")?;
		let program = cargo(caller, path.as_deref())?;
		// Cargo ignores an empty CARGO_HOME and joins a relative one to its working directory, which
		// is about to change, then resolves `..` by name, as `absolute` does, before reading
		// configuration in the result's ancestors.
		let (home, explicit) = match caller.cargo_home.as_ref().filter(|home| !home.is_empty()) {
			Some(home) => (absolute(&caller.dir, Path::new(home)), true),
			None => (
				absolute(
					&caller.dir,
					&caller
						.home
						.as_ref()
						.context("cannot find the home directory")?
						.join(".cargo"),
				),
				false,
			),
		};
		// A default CARGO_HOME comes from HOME, which no checkout controls, even one at HOME itself.
		if explicit && let Some(checkout) = checkout(&caller.dir)? {
			ensure!(
				!inside(&home, &checkout),
				"CARGO_HOME {} is inside the checkout {}, whose files could configure Cargo; set CARGO_HOME outside it",
				home.display(),
				checkout.display()
			);
		}
		// Configuration in a directory that also holds the caller's may come with the checkout, such as
		// from an outer one, while the rest of the home's ancestors are the user's.
		for dir in home.ancestors().filter(|dir| caller.dir.starts_with(dir)) {
			if let Some(config) = untrusted(dir, &home, &[".cargo/config.toml", ".cargo/config"])? {
				bail!(
					"cargo install would read {}, outside CARGO_HOME {}; remove it or set CARGO_HOME elsewhere",
					config.display(),
					home.display()
				);
			}
		}
		let dir = root(&caller.dir).to_owned();
		let files = [
			".cargo/config.toml",
			".cargo/config",
			"rust-toolchain.toml",
			"rust-toolchain",
		];
		if let Some(file) = untrusted(&dir, &home, &files)? {
			bail!(
				"no clean directory to run Cargo in: {} would configure it; remove it or run without --build",
				file.display()
			);
		}
		Ok(Self {
			program,
			home,
			path,
			dir,
		})
	}

	/// `cargo`, run from the clean directory with the resolved CARGO_HOME and PATH.
	pub fn command(&self) -> Command {
		let mut command = Command::new(&self.program);
		command.current_dir(&self.dir);
		self.env(&mut command);
		command
	}

	/// Pass the resolved Cargo, CARGO_HOME, and PATH to a process that runs elsewhere.
	pub fn env(&self, command: &mut Command) {
		command.env("CARGO", &self.program).env("CARGO_HOME", &self.home);
		if let Some(path) = &self.path {
			command.env("PATH", path);
		}
	}
}

/// The filesystem root holding `dir`, where crabx runs what must not hold the caller's directory.
pub fn root(dir: &Path) -> &Path {
	dir.ancestors().last().unwrap_or(dir)
}

/// The absolute path of CARGO, or `cargo`, as the caller would run it.
fn cargo(caller: &Caller, path: Option<&OsStr>) -> Result<PathBuf> {
	let name = caller.cargo.as_deref().unwrap_or(OsStr::new("cargo"));
	if Path::new(name).components().count() > 1 || Path::new(name).is_absolute() {
		return Ok(absolute(&caller.dir, Path::new(name)));
	}
	let mut names = vec![name.to_owned()];
	if !std::env::consts::EXE_SUFFIX.is_empty() {
		let mut exe = name.to_owned();
		exe.push(std::env::consts::EXE_SUFFIX);
		names.push(exe);
	}
	path.into_iter()
		.flat_map(std::env::split_paths)
		.flat_map(|dir| names.iter().map(move |name| dir.join(name)))
		.find(|candidate| executable(candidate))
		.with_context(|| format!("--build needs cargo and a Rust toolchain; found no {name:?} on PATH"))
}

/// `path` from `dir`, with `.` and `..` resolved by name, as Windows resolves them.
fn absolute(dir: &Path, path: &Path) -> PathBuf {
	let mut absolute = PathBuf::new();
	for component in dir.join(path).components() {
		match component {
			Component::ParentDir => {
				absolute.pop();
			}
			Component::CurDir => (),
			component => absolute.push(component),
		}
	}
	absolute
}

fn executable(path: &Path) -> bool {
	let Ok(metadata) = fs::metadata(path) else {
		return false;
	};
	#[cfg(unix)]
	{
		use std::os::unix::fs::PermissionsExt;
		metadata.is_file() && metadata.permissions().mode() & 0o111 != 0
	}
	#[cfg(not(unix))]
	metadata.is_file()
}

/// The git checkout holding `dir`, canonicalized.
fn checkout(dir: &Path) -> Result<Option<PathBuf>> {
	for ancestor in dir.ancestors() {
		if present(&ancestor.join(".git"))? {
			return Ok(Some(fs::canonicalize(ancestor)?));
		}
	}
	Ok(None)
}

/// Whether `path`, or the part of it that exists, resolves inside `dir`.
fn inside(path: &Path, dir: &Path) -> bool {
	path.ancestors()
		.find_map(|ancestor| fs::canonicalize(ancestor).ok())
		.is_some_and(|path| path.starts_with(dir))
}

/// The first of `files` in `dir`, skipping configuration that is CARGO_HOME's own.
fn untrusted(dir: &Path, home: &Path, files: &[&str]) -> Result<Option<PathBuf>> {
	for file in files {
		let path = dir.join(file);
		if path.parent() != Some(home) && present(&path)? {
			return Ok(Some(path));
		}
	}
	Ok(None)
}

/// Whether anything, even a dangling link, is at `path`.
fn present(path: &Path) -> Result<bool> {
	match fs::symlink_metadata(path) {
		Ok(_) => Ok(true),
		Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
		Err(error) => Err(error).with_context(|| format!("inspect {}", path.display())),
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn tree(files: &[&str]) -> tempfile::TempDir {
		let temp = tempfile::tempdir().unwrap();
		for file in files {
			let path = temp.path().join(file);
			fs::create_dir_all(path.parent().unwrap()).unwrap();
			fs::write(&path, "").unwrap();
			#[cfg(unix)]
			fs::set_permissions(&path, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
		}
		temp
	}

	fn caller(temp: &Path, dir: &str) -> Caller {
		Caller {
			dir: temp.join(dir),
			cargo: None,
			cargo_home: None,
			path: Some(temp.join("bin").into_os_string()),
			home: Some(temp.to_owned()),
		}
	}

	#[test]
	fn relative_paths_resolve_against_the_caller() {
		let temp = tree(&[
			"work/toolchain/bin/cargo",
			"work/checkout/.git/HEAD",
			"work/trusted/config.toml",
		]);
		let work = temp.path().join("work");
		let mut caller = caller(temp.path(), "work/checkout");
		let elsewhere = temp.path().join("elsewhere");
		caller.path = Some(std::env::join_paths([Path::new("../toolchain/bin"), &elsewhere]).unwrap());
		caller.cargo_home = Some("../trusted".into());
		let toolchain = Toolchain::new(&caller).unwrap();
		assert_eq!(toolchain.program, work.join("toolchain/bin/cargo"));
		assert_eq!(toolchain.home, work.join("trusted"));
		assert_eq!(
			toolchain.path.unwrap(),
			std::env::join_paths([work.join("toolchain/bin"), elsewhere]).unwrap()
		);
		assert_eq!(toolchain.dir, root(temp.path()));
		// A relative CARGO is a path from the caller's directory, and an empty CARGO_HOME is unset.
		caller.dir = work.join("toolchain");
		caller.cargo = Some("./bin/cargo".into());
		caller.cargo_home = Some("".into());
		let toolchain = Toolchain::new(&caller).unwrap();
		assert_eq!(toolchain.program, work.join("toolchain/bin/cargo"));
		assert_eq!(toolchain.home, temp.path().join(".cargo"));
		caller.cargo = Some("missing".into());
		let error = Toolchain::new(&caller).unwrap_err().to_string();
		assert!(error.contains("no \"missing\" on PATH"), "{error}");
	}

	#[test]
	fn rejects_a_home_a_checkout_controls_or_configures() {
		let temp = tree(&["bin/cargo", "work/checkout/.git/HEAD", "work/checkout/tools/main.rs"]);
		let work = temp.path().join("work");
		let mut caller = caller(temp.path(), "work/checkout/tools");
		for home in [
			work.join("checkout/.cargo").into_os_string(),
			"../.cargo".into(),
			"../missing/home".into(),
		] {
			caller.cargo_home = Some(home);
			let error = Toolchain::new(&caller).unwrap_err().to_string();
			assert!(error.contains("is inside the checkout"), "{error}");
		}
		// A default home comes from HOME, which a checkout cannot set.
		caller.cargo_home = None;
		caller.home = Some(work.join("checkout"));
		Toolchain::new(&caller).unwrap();
		// Cargo would read work/.cargo/config.toml beside this home.
		fs::create_dir(work.join(".cargo")).unwrap();
		fs::write(work.join(".cargo/config.toml"), "").unwrap();
		caller.cargo_home = Some(work.join("cargo-home").into_os_string());
		let error = Toolchain::new(&caller).unwrap_err().to_string();
		assert!(
			error.contains(&work.join(".cargo/config.toml").display().to_string()),
			"{error}"
		);
		// The home's own configuration is the user's, as is one outside the caller's directories.
		caller.cargo_home = Some(work.join(".cargo").into_os_string());
		Toolchain::new(&caller).unwrap();
		fs::remove_file(work.join(".cargo/config.toml")).unwrap();
		fs::create_dir_all(work.join("opt/.cargo")).unwrap();
		fs::write(work.join("opt/.cargo/config"), "").unwrap();
		caller.cargo_home = Some(work.join("opt/cargo").into_os_string());
		Toolchain::new(&caller).unwrap();
		caller.dir = work.join("opt/checkout");
		let error = Toolchain::new(&caller).unwrap_err().to_string();
		assert!(
			error.contains(&work.join("opt/.cargo/config").display().to_string()),
			"{error}"
		);
	}

	#[test]
	fn a_root_with_configuration_leaves_no_clean_directory() {
		for file in [
			".cargo/config.toml",
			".cargo/config",
			"rust-toolchain.toml",
			"rust-toolchain",
		] {
			let temp = tree(&[file]);
			let found = untrusted(temp.path(), &temp.path().join("home"), &[file]).unwrap();
			assert_eq!(found, Some(temp.path().join(file)));
			// Unless the root's .cargo is CARGO_HOME itself, as with HOME=/.
			if file.starts_with(".cargo/") {
				assert_eq!(
					untrusted(temp.path(), &temp.path().join(".cargo"), &[file]).unwrap(),
					None
				);
			}
		}
	}
}
