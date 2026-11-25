//! Running Cargo for `--build` away from the caller's directory, which may be an untrusted checkout.
// A registry `cargo install` reads `.cargo/config.toml` and `.cargo/config` in CARGO_HOME and its
// ancestors, and rustup picks the toolchain from `rust-toolchain.toml` in its working directory and
// ancestors, so Cargo runs from the filesystem root and a CARGO_HOME whose path holds no other
// configuration.

use anyhow::{Context, Result, bail, ensure};
use std::{
	ffi::OsString,
	fs,
	path::{Component, Path, PathBuf},
	process::Command,
};

/// The caller's directory and the environment that chooses Cargo.
pub struct Caller {
	pub dir: PathBuf,
	pub cargo: Option<OsString>,
	pub cargo_home: Option<OsString>,
	pub target_dir: Option<OsString>,
	pub path: Option<OsString>,
	pub home: Option<PathBuf>,
}

impl Caller {
	pub fn current() -> Result<Self> {
		Ok(Self {
			dir: std::env::current_dir().context("read the current directory")?,
			cargo: std::env::var_os("CARGO"),
			cargo_home: std::env::var_os("CARGO_HOME"),
			target_dir: std::env::var_os("CARGO_TARGET_DIR"),
			path: std::env::var_os("PATH"),
			home: directories::BaseDirs::new().map(|base| base.home_dir().to_owned()),
		})
	}
}

/// CARGO, CARGO_HOME, CARGO_TARGET_DIR, and PATH resolved against the caller's directory, and
/// checked to load no configuration or build output a checkout could have written.
#[derive(Debug)]
pub struct Toolchain {
	/// CARGO, from the caller's directory when it is a path, or a name to find on PATH.
	cargo: Option<PathBuf>,
	home: PathBuf,
	target_dir: Option<PathBuf>,
	path: Option<OsString>,
	dir: PathBuf,
}

impl Toolchain {
	pub fn new(caller: &Caller) -> Result<Self> {
		let nonempty = |value: &Option<OsString>| value.clone().filter(|value| !value.is_empty());
		// The OS resolves `..` in these after any symlink before it, so they are joined, not normalized.
		let path = caller
			.path
			.as_ref()
			.map(|path| std::env::join_paths(std::env::split_paths(path).map(|entry| caller.dir.join(entry))))
			.transpose()
			.context("PATH holds an entry that cannot be rewritten")?;
		let cargo = nonempty(&caller.cargo).map(|cargo| {
			let cargo = PathBuf::from(cargo);
			if cargo.components().count() > 1 {
				caller.dir.join(cargo)
			} else {
				cargo
			}
		});
		// Cargo ignores an empty CARGO_HOME and joins a relative one to its working directory, which
		// is about to change, then resolves `..` by name, as `absolute` does, before reading
		// configuration in the result's ancestors.
		let (home, explicit) = match nonempty(&caller.cargo_home) {
			Some(home) => (absolute(&caller.dir, Path::new(&home)), true),
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
		let target_dir = nonempty(&caller.target_dir).map(|dir| caller.dir.join(dir));
		if let Some(checkout) = checkout(&caller.dir)? {
			// A default CARGO_HOME comes from HOME, which no checkout controls, even one at HOME itself.
			ensure!(
				!explicit || !inside(&home, &checkout),
				"CARGO_HOME {} is inside the checkout {}, whose files could configure Cargo; set CARGO_HOME outside it",
				home.display(),
				checkout.display()
			);
			if let Some(target_dir) = &target_dir {
				ensure!(
					!inside(target_dir, &checkout),
					"CARGO_TARGET_DIR {} is inside the checkout {}, whose build output Cargo could reuse; set it outside or unset it",
					target_dir.display(),
					checkout.display()
				);
			}
		}
		// Configuration in a directory that also holds the caller's may come with the checkout, such as
		// from an outer one, while the rest of the home's ancestors are the user's. Compared by what
		// they resolve to, since the same directory can have several names, such as /var on macOS.
		let here = fs::canonicalize(&caller.dir).context("resolve the current directory")?;
		let shared = |dir: &Path| fs::canonicalize(dir).is_ok_and(|dir| here.starts_with(dir));
		for dir in home.ancestors().filter(|dir| shared(dir)) {
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
			cargo,
			home,
			target_dir,
			path,
			dir,
		})
	}

	/// `cargo`, run from the clean directory with the resolved environment.
	pub fn command(&self) -> Result<Command> {
		let mut command = Command::new(self.program()?);
		command.current_dir(&self.dir);
		self.env(&mut command);
		Ok(command)
	}

	/// Pass the resolved environment to a process that runs elsewhere and may never need Cargo.
	pub fn env(&self, command: &mut Command) {
		command.env("CARGO_HOME", &self.home);
		if let Some(cargo) = &self.cargo {
			command.env("CARGO", cargo);
		}
		if let Some(target_dir) = &self.target_dir {
			command.env("CARGO_TARGET_DIR", target_dir);
		}
		if let Some(path) = &self.path {
			command.env("PATH", path);
		}
	}

	/// The absolute path of CARGO, or `cargo`, as the caller would run it.
	fn program(&self) -> Result<PathBuf> {
		let name = self.cargo.as_deref().unwrap_or(Path::new("cargo"));
		if name.is_absolute() {
			return Ok(name.to_owned());
		}
		let mut names = vec![name.as_os_str().to_owned()];
		if !std::env::consts::EXE_SUFFIX.is_empty() {
			let mut exe = name.as_os_str().to_owned();
			exe.push(std::env::consts::EXE_SUFFIX);
			names.push(exe);
		}
		self.path
			.iter()
			.flat_map(std::env::split_paths)
			.flat_map(|dir| names.iter().map(move |name| dir.join(name)))
			.find(|candidate| executable(candidate))
			.with_context(|| {
				format!(
					"--build needs cargo and a Rust toolchain; found no {:?} on PATH",
					name.display()
				)
			})
	}
}

/// The filesystem root holding `dir`, where crabx runs what must not hold the caller's directory.
pub fn root(dir: &Path) -> &Path {
	dir.ancestors().last().unwrap_or(dir)
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

/// The outermost git checkout holding `dir`, such as the superproject of a submodule, canonicalized.
fn checkout(dir: &Path) -> Result<Option<PathBuf>> {
	let mut outermost = None;
	for ancestor in dir.ancestors() {
		if present(&ancestor.join(".git"))? {
			outermost = Some(ancestor);
		}
	}
	Ok(outermost.map(fs::canonicalize).transpose()?)
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
			target_dir: None,
			path: Some(temp.join("bin").into_os_string()),
			home: Some(temp.to_owned()),
		}
	}

	fn error(caller: &Caller) -> String {
		Toolchain::new(caller).unwrap_err().to_string()
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
		caller.target_dir = Some("../target".into());
		let toolchain = Toolchain::new(&caller).unwrap();
		let command = toolchain.command().unwrap();
		assert_eq!(command.get_program(), work.join("checkout/../toolchain/bin/cargo"));
		assert_eq!(command.get_current_dir(), Some(root(temp.path())));
		// Cargo resolves `..` in CARGO_HOME by name; the OS resolves the rest.
		assert_eq!(toolchain.home, work.join("trusted"));
		assert_eq!(toolchain.target_dir.unwrap(), work.join("checkout/../target"));
		assert_eq!(
			toolchain.path.unwrap(),
			std::env::join_paths([work.join("checkout/../toolchain/bin"), elsewhere]).unwrap()
		);
		// A relative CARGO is a path from the caller's directory, and an empty CARGO_HOME is unset.
		caller.dir = work.join("toolchain");
		caller.cargo = Some("./bin/cargo".into());
		caller.cargo_home = Some("".into());
		let toolchain = Toolchain::new(&caller).unwrap();
		assert_eq!(
			toolchain.command().unwrap().get_program(),
			work.join("toolchain/./bin/cargo")
		);
		assert_eq!(toolchain.home, temp.path().join(".cargo"));
		// A missing Cargo matters only to what runs it.
		caller.cargo = Some("missing".into());
		let toolchain = Toolchain::new(&caller).unwrap();
		let error = toolchain.command().unwrap_err().to_string();
		assert!(error.contains("no \"missing\" on PATH"), "{error}");
	}

	#[test]
	fn rejects_what_a_checkout_controls_or_configures() {
		// tools is a submodule of the checkout.
		let temp = tree(&["bin/cargo", "work/checkout/.git/HEAD", "work/checkout/tools/.git"]);
		let work = temp.path().join("work");
		let mut caller = caller(temp.path(), "work/checkout/tools");
		for home in [
			work.join("checkout/.cargo").into_os_string(),
			"../.cargo".into(),
			"../missing/home".into(),
		] {
			caller.cargo_home = Some(home);
			assert!(error(&caller).contains("CARGO_HOME"), "{}", error(&caller));
			assert!(error(&caller).contains("is inside the checkout"));
		}
		caller.cargo_home = None;
		caller.target_dir = Some("target".into());
		assert!(error(&caller).contains("CARGO_TARGET_DIR"), "{}", error(&caller));
		caller.target_dir = Some(work.join("target").into_os_string());
		Toolchain::new(&caller).unwrap();
		// A default home comes from HOME, which a checkout cannot set.
		caller.home = Some(work.join("checkout"));
		Toolchain::new(&caller).unwrap();
		// Cargo would read work/.cargo/config.toml beside this home, whatever name reaches it.
		fs::create_dir(work.join(".cargo")).unwrap();
		fs::write(work.join(".cargo/config.toml"), "").unwrap();
		let mut homes = vec![work.join("cargo-home")];
		#[cfg(unix)]
		{
			std::os::unix::fs::symlink(&work, temp.path().join("alias")).unwrap();
			homes.push(temp.path().join("alias/cargo-home"));
		}
		for home in homes {
			let config = home.parent().unwrap().join(".cargo/config.toml").display().to_string();
			caller.cargo_home = Some(home.into_os_string());
			assert!(error(&caller).contains(&config), "{}", error(&caller));
		}
		// The home's own configuration is the user's, as is one outside the caller's directories.
		caller.cargo_home = Some(work.join(".cargo").into_os_string());
		Toolchain::new(&caller).unwrap();
		fs::remove_file(work.join(".cargo/config.toml")).unwrap();
		fs::create_dir_all(work.join("opt/.cargo")).unwrap();
		fs::write(work.join("opt/.cargo/config"), "").unwrap();
		caller.cargo_home = Some(work.join("opt/cargo").into_os_string());
		Toolchain::new(&caller).unwrap();
		caller.dir = work.join("opt");
		assert!(error(&caller).contains(&work.join("opt/.cargo/config").display().to_string()));
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
