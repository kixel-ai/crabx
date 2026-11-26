use crate::{cli::Spec, ratchet::Ratchet};
use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
	collections::BTreeSet,
	fs::{self, File, TryLockError},
	io::Write,
	path::{Path, PathBuf},
};

pub struct Cache {
	pub root: PathBuf,
	pub ratchet: Ratchet,
}

/// The `source` of a release Cargo compiled from crates.io.
pub const BUILT: &str = "cargo install --locked";

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
	pub version: Version,
	pub bins: Vec<String>,
	pub source: String,
}

impl Record {
	/// Whether crabx verified a GitHub attestation for this release.
	pub fn attested(&self) -> bool {
		crate::ratchet::attested(&self.source)
	}

	/// Whether this is the author's own release, rather than quickinstall's or a source build.
	// An unknown source counts as the author's, so the ratchet covers it.
	pub fn author(&self) -> bool {
		!matches!(self.source.as_str(), "quickinstall" | BUILT)
	}

	pub fn binary(&self, requested: Option<&str>, krate: &str) -> Result<&str> {
		let extension = std::env::consts::EXE_SUFFIX;
		if let Some(requested) = requested {
			let name = format!("{requested}{extension}");
			return self
				.bins
				.iter()
				.find(|bin| **bin == name)
				.map(String::as_str)
				.with_context(|| format!("no binary {requested:?}; available: {}", self.bins.join(", ")));
		}
		let name = format!("{krate}{extension}");
		let mut matching = self.bins.iter().filter(|bin| bin.eq_ignore_ascii_case(&name));
		if let Some(bin) = matching.next() {
			ensure!(
				matching.next().is_none(),
				"several binaries match the crate name; choose --bin NAME: {}",
				self.bins.join(", ")
			);
			return Ok(bin);
		}
		match self.bins.as_slice() {
			[bin] => Ok(bin),
			[] => bail!("the crate contains no binaries"),
			bins => bail!(
				"the crate contains several binaries; choose --bin NAME: {}",
				bins.join(", ")
			),
		}
	}
}

impl Cache {
	pub fn new(spec: &Spec, mode: &str) -> Result<Self> {
		let base = directories::BaseDirs::new().context("cannot find the user's cache directory")?;
		Ok(Self::under(base.cache_dir(), spec, mode))
	}

	// Every mode lives under `v2` so no 0.1 entry runs: those were never checked against 0.2's
	// attestation policy, or were built by Cargo in the caller's directory. They stay as 0.1 wrote them.
	fn under(cache_dir: &Path, spec: &Spec, mode: &str) -> Self {
		let name = spec.name.to_ascii_lowercase();
		let host = |layout: &str| cache_dir.join("crabx").join(layout).join(binstalk::TARGET);
		let root = host("v2").join(mode).join(&name);
		// Every layout and mode that can hold a verified author release, 0.1's included.
		let caches = ["v2", "v1"]
			.into_iter()
			.flat_map(|layout| ["author", "quickinstall"].map(|mode| host(layout).join(mode).join(&name)))
			.collect();
		let ratchet = Ratchet {
			dir: host("v2").join("attested"),
			name,
			caches,
		};
		Self { root, ratchet }
	}

	/// A cache at `root` whose ratchet lives inside it.
	#[cfg(test)]
	pub fn at(root: PathBuf) -> Self {
		let ratchet = Ratchet {
			dir: root.join("attested"),
			name: "fixture".into(),
			caches: vec![root.clone()],
		};
		Self { root, ratchet }
	}

	pub fn version(&self, version: &Version) -> PathBuf {
		self.root.join(version.to_string())
	}

	/// The versions with a cache entry, which `load` then checks is complete.
	// 0.1 also wrote `latest`, which another delay may have chosen, so nothing reads it.
	pub fn installed(&self) -> Result<BTreeSet<Version>> {
		let entries = match fs::read_dir(&self.root) {
			Ok(entries) => entries,
			Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeSet::new()),
			Err(e) => return Err(e).with_context(|| format!("list {}", self.root.display())),
		};
		let mut versions = BTreeSet::new();
		for entry in entries {
			let name = entry?.file_name();
			if let Some(version) = name.to_str().and_then(|name| Version::parse(name).ok()) {
				versions.insert(version);
			}
		}
		Ok(versions)
	}

	pub fn load(&self, version: &Version) -> Result<Option<Record>> {
		let path = self.version(version);
		// Check the atomically published directory first. A missing record cannot
		// then be confused with a complete directory published after that read.
		if !path
			.try_exists()
			.with_context(|| format!("inspect cache entry {}", path.display()))?
		{
			return Ok(None);
		}
		let file = File::open(path.join("record.json"))
			.with_context(|| format!("incomplete cache entry {}", path.display()))?;
		let record: Record =
			serde_json::from_reader(file).with_context(|| format!("corrupt cache record in {}", path.display()))?;
		ensure!(
			&record.version == version && !record.bins.is_empty(),
			"invalid cache record in {}",
			path.display()
		);
		for bin in &record.bins {
			ensure!(
				valid_binary(bin) && path.join("bin").join(bin).is_file(),
				"missing or invalid cached binary {bin:?} in {}",
				path.display()
			);
		}
		Ok(Some(record))
	}

	pub fn lock(&self) -> Result<File> {
		fs::create_dir_all(&self.root)?;
		let file = File::options()
			.create(true)
			.truncate(false)
			.read(true)
			.write(true)
			.open(self.root.join("lock"))?;
		file.lock()?;
		Ok(file)
	}

	/// Create staging space while holding the installation lock.
	pub fn stage(&self) -> Result<tempfile::TempDir> {
		// The caller holds the install lock, so any earlier staging directory
		// has lost its owner. Published version directories have semver names.
		for entry in fs::read_dir(&self.root)? {
			let entry = entry?;
			if entry
				.file_name()
				.to_str()
				.is_some_and(|name| name.starts_with("install-"))
				&& entry.file_type()?.is_dir()
			{
				fs::remove_dir_all(entry.path())?;
			}
		}
		Ok(tempfile::Builder::new().prefix("install-").tempdir_in(&self.root)?)
	}

	pub fn try_refresh_lock(&self) -> Result<Option<File>> {
		let file = File::options()
			.create(true)
			.truncate(false)
			.read(true)
			.write(true)
			.open(self.root.join("refresh.lock"))?;
		match file.try_lock() {
			Ok(()) => Ok(Some(file)),
			Err(TryLockError::WouldBlock) => Ok(None),
			Err(TryLockError::Error(e)) => Err(e.into()),
		}
	}
}

pub fn valid_binary(name: &str) -> bool {
	!name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', ':', '\0'])
}

pub fn atomic(path: &Path, bytes: &[u8]) -> Result<()> {
	let parent = path.parent().context("cache file has no parent")?;
	fs::create_dir_all(parent)?;
	let mut file = tempfile::NamedTempFile::new_in(parent)?;
	file.write_all(bytes)?;
	file.as_file().sync_all()?;
	file.persist(path)?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn selection_rejects_ambiguity_and_preserves_names() {
		let mut record = Record {
			version: Version::new(1, 2, 3),
			bins: vec![
				format!("first{}", std::env::consts::EXE_SUFFIX),
				format!("second{}", std::env::consts::EXE_SUFFIX),
			],
			source: "author".into(),
		};
		assert!(record.binary(None, "other").is_err());
		assert!(record.binary(Some("missing"), "first").is_err());
		assert_eq!(record.binary(Some("second"), "other").unwrap(), record.bins[1]);
		assert_eq!(record.binary(None, "first").unwrap(), record.bins[0]);
		assert_eq!(record.binary(None, "FiRsT").unwrap(), record.bins[0]);
		record.bins.pop();
		assert_eq!(record.binary(None, "other").unwrap(), record.bins[0]);
		record.bins.push(format!("FIRST{}", std::env::consts::EXE_SUFFIX));
		assert!(record.binary(None, "first").is_err());
		assert_eq!(record.binary(Some("first"), "first").unwrap(), record.bins[0]);
	}

	#[test]
	fn entries_are_atomic_and_incomplete_entries_fail() {
		let temp = tempfile::tempdir().unwrap();
		let cache = Cache::at(temp.path().to_owned());
		let version = Version::new(2, 3, 4);
		assert!(cache.load(&version).unwrap().is_none());
		assert!(cache.installed().unwrap().is_empty());
		fs::write(cache.root.join("latest"), "9.9.9").unwrap();
		fs::create_dir(cache.root.join("install-1.0.0")).unwrap();
		fs::create_dir(cache.version(&version)).unwrap();
		assert_eq!(cache.installed().unwrap(), BTreeSet::from([version.clone()]));
		let path = cache.version(&version);
		assert!(format!("{:#}", cache.load(&version).unwrap_err()).contains(&path.display().to_string()));
		fs::write(path.join("record.json"), "malformed").unwrap();
		assert!(format!("{:#}", cache.load(&version).unwrap_err()).contains(&path.display().to_string()));
		fs::write(
			path.join("record.json"),
			r#"{"version":"2.3.4","bins":["tool"],"source":"fixture"}"#,
		)
		.unwrap();
		assert!(format!("{:#}", cache.load(&version).unwrap_err()).contains(&path.display().to_string()));
		for name in ["../evil", "C:evil", "a/b", "a\\b", ".", "..", ""] {
			assert!(!valid_binary(name));
		}
	}

	#[test]
	fn verified_entries_never_share_a_directory_with_0_1s() {
		let temp = tempfile::tempdir().unwrap();
		let spec = Spec::parse("Tool@1.2.3").unwrap();
		let version = Version::new(1, 2, 3);
		// What 0.1 left behind: a complete, executable-looking entry nothing verified under 0.2.
		let legacy = |mode: &str| {
			temp.path()
				.join("crabx/v1")
				.join(binstalk::TARGET)
				.join(mode)
				.join("tool")
		};
		for mode in ["author", "quickinstall", "build"] {
			let entry = legacy(mode).join("1.2.3");
			fs::create_dir_all(entry.join("bin")).unwrap();
			fs::write(entry.join("bin/tool"), "unverified").unwrap();
			fs::write(
				entry.join("record.json"),
				r#"{"version":"1.2.3","bins":["tool"],"source":"fixture"}"#,
			)
			.unwrap();
			let cache = Cache::under(temp.path(), &spec, mode);
			assert_eq!(
				cache.root,
				temp.path()
					.join("crabx/v2")
					.join(binstalk::TARGET)
					.join(mode)
					.join("tool")
			);
			assert!(cache.load(&version).unwrap().is_none());
			assert!(cache.installed().unwrap().is_empty());
			// Publishing beside it renames into a directory of its own and leaves the old one intact.
			let _lock = cache.lock().unwrap();
			let stage = cache.stage().unwrap();
			fs::write(stage.path().join("record.json"), "{}").unwrap();
			fs::rename(stage.path(), cache.version(&version)).unwrap();
			assert!(entry.join("record.json").is_file() && entry.join("bin/tool").is_file());
			assert!(!legacy(mode).join("lock").exists());
		}
	}

	#[test]
	fn staging_reclaims_interrupted_installs_and_preserves_versions() {
		let temp = tempfile::tempdir().unwrap();
		let cache = Cache::at(temp.path().to_owned());
		let _lock = cache.lock().unwrap();
		fs::create_dir_all(cache.root.join("install-orphan/bin")).unwrap();
		fs::create_dir(cache.root.join("1.2.3")).unwrap();
		let stage = cache.stage().unwrap();
		assert!(stage.path().is_dir());
		assert!(!cache.root.join("install-orphan").exists());
		assert!(cache.root.join("1.2.3").is_dir());
	}
}
