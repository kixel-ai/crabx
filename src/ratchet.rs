//! The attestation ratchet: once crabx verifies a GitHub attestation for one of a crate's author
//! releases, it never again runs or caches an unattested author release of that crate. Without it,
//! an archive swapped for an unattested one would read as a release that was never attested, since
//! GitHub answers 404 for a digest it holds no attestation for.

use crate::cache::{self, Record};
use anyhow::{Context, Result, bail};
use std::{
	fs::{self, File},
	path::{Path, PathBuf},
};

/// One crate's ratchet, shared by every install mode.
pub struct Ratchet {
	/// Where the crate's markers and lock live.
	pub dir: PathBuf,
	/// The crate's lowercase name.
	pub name: String,
	/// The cache roots, one directory per version, the one-time migration scans.
	pub caches: Vec<PathBuf>,
}

impl Ratchet {
	/// Present once any author release of the crate was verified; never deleted by crabx.
	pub fn marker(&self) -> PathBuf {
		self.dir.join(format!("{}.attested", self.name))
	}

	/// Present once the caches that existed before the ratchet were scanned for attested releases.
	fn migrated(&self) -> PathBuf {
		self.dir.join(format!("{}.migrated", self.name))
	}

	fn lock(&self) -> Result<File> {
		fs::create_dir_all(&self.dir)?;
		let path = self.dir.join(format!("{}.lock", self.name));
		// Rust opens files close-on-exec, so an `exec` of the admitted binary releases the lock.
		File::options()
			.create(true)
			.truncate(false)
			.read(true)
			.write(true)
			.open(&path)
			.with_context(|| format!("open {}", path.display()))
	}

	/// Record that an author release of the crate was verified.
	pub fn activate(&self) -> Result<()> {
		if exists(&self.marker())? && exists(&self.migrated())? {
			return Ok(());
		}
		let lock = self.lock()?;
		// Waits for every admitted unattested release to finish publishing or launching.
		lock.lock()?;
		cache::atomic(&self.marker(), b"")?;
		// No scan is needed now, and one would undo a later deletion of the marker.
		cache::atomic(&self.migrated(), b"")
	}

	/// Whether the ratchet is active, seeding it once from the caches earlier crabx versions wrote.
	pub fn active(&self) -> Result<bool> {
		let marker = self.marker();
		if exists(&marker)? {
			return Ok(true);
		}
		if exists(&self.migrated())? {
			return Ok(false);
		}
		let lock = self.lock()?;
		lock.lock()?;
		if exists(&marker)? {
			return Ok(true);
		}
		if exists(&self.migrated())? {
			return Ok(false);
		}
		let attested = self.scan()?;
		if attested {
			cache::atomic(&marker, b"")?;
		}
		cache::atomic(&self.migrated(), b"")?;
		Ok(attested)
	}

	/// Whether any cached entry records a verified attestation.
	fn scan(&self) -> Result<bool> {
		for root in &self.caches {
			let entries = match fs::read_dir(root) {
				Ok(entries) => entries,
				Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
				Err(e) => return Err(e).with_context(|| format!("list {}", root.display())),
			};
			for entry in entries {
				let entry = entry?;
				// Published entries have semver names; staging directories and state files do not.
				let versioned = entry
					.file_name()
					.to_str()
					.is_some_and(|name| semver::Version::parse(name).is_ok());
				if !versioned || !entry.file_type()?.is_dir() {
					continue;
				}
				if source(&entry.path().join("record.json"))?.is_some_and(|source| attested(&source)) {
					return Ok(true);
				}
			}
		}
		Ok(false)
	}

	/// Admit `record` to be published or run under `required`, refusing an unattested author
	/// release once the ratchet is active. The returned lock, if any, must be held until the release
	/// is published or launched, so an activation in between cannot let it through.
	pub fn admit(&self, record: &Record, required: bool) -> Result<Option<File>> {
		if record.attested() || !record.author() {
			return Ok(None);
		}
		let refuse = || self.refuse(record, required);
		if required || self.active()? {
			return Err(refuse());
		}
		let lock = self.lock()?;
		lock.lock_shared()?;
		if exists(&self.marker())? {
			return Err(refuse());
		}
		Ok(Some(lock))
	}

	/// Whether an unpinned run may choose `record` under `required`.
	pub fn eligible(&self, record: &Record, required: bool) -> Result<bool> {
		Ok(record.attested() || !record.author() || !(required || self.active()?))
	}

	fn refuse(&self, record: &Record, required: bool) -> anyhow::Error {
		let release = format!("{}@{}", self.name, record.version);
		if required {
			return anyhow::anyhow!(
				"{release} has no verified GitHub attestation, which --require-attestation requires; run it without --require-attestation, or use --build to compile it"
			);
		}
		anyhow::anyhow!(
			"{release} has no verified GitHub attestation, though crabx verified one for an earlier {} release, so its archive may have been swapped; if the author stopped attesting on purpose, delete {} to accept unattested releases again, or use --build to compile it",
			self.name,
			self.marker().display()
		)
	}
}

/// Whether a record's `source` says crabx verified a GitHub attestation for it.
pub fn attested(source: &str) -> bool {
	source.starts_with("GitHub attestations")
}

fn exists(path: &Path) -> Result<bool> {
	path.try_exists().with_context(|| format!("inspect {}", path.display()))
}

/// The `source` an entry's record names, read loosely since 0.1 wrote it, or None when the entry
/// has no record, as an interrupted 0.1 install can leave.
fn source(path: &Path) -> Result<Option<String>> {
	let file = match File::open(path) {
		Ok(file) => file,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
		Err(e) => return Err(e).with_context(|| format!("read {}", path.display())),
	};
	#[derive(serde::Deserialize)]
	struct Source {
		source: String,
	}
	match serde_json::from_reader::<_, Source>(std::io::BufReader::new(file)) {
		Ok(record) => Ok(Some(record.source)),
		Err(error) => bail!(
			"corrupt cache record {}, which crabx reads once for verified attestations: {error}; delete its directory",
			path.display()
		),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use semver::Version;

	const ATTESTED: &str = "GitHub attestations (1) sha256:00";

	fn record(version: &str, source: &str) -> Record {
		Record {
			version: Version::parse(version).unwrap(),
			bins: vec!["tool".into()],
			source: source.into(),
		}
	}

	/// A ratchet over a 0.2 and a 0.1 cache root.
	fn ratchet(temp: &Path) -> Ratchet {
		Ratchet {
			dir: temp.join("attested"),
			name: "tool".into(),
			caches: vec![temp.join("v2"), temp.join("v1")],
		}
	}

	fn entry(root: &Path, version: &str, record: &str) {
		fs::create_dir_all(root.join(version)).unwrap();
		fs::write(root.join(version).join("record.json"), record).unwrap();
	}

	#[test]
	fn migration_seeds_once_from_any_layout() {
		let temp = tempfile::tempdir().unwrap();
		let ratchet = ratchet(temp.path());
		// 0.1's records, staging directories, and state files; only the attested record counts.
		entry(
			&temp.path().join("v2"),
			"2.0.0",
			r#"{"source":"author release (no published attestation)"}"#,
		);
		fs::create_dir_all(temp.path().join("v1/install-x")).unwrap();
		fs::write(temp.path().join("v1/index.json"), "not a record").unwrap();
		fs::create_dir_all(temp.path().join("v1/0.9.0")).unwrap();
		entry(
			&temp.path().join("v1"),
			"1.0.0",
			&format!(r#"{{"version":"1.0.0","bins":[],"source":"{ATTESTED}","extra":1}}"#),
		);
		assert!(ratchet.active().unwrap());
		assert!(ratchet.marker().is_file() && ratchet.migrated().is_file());
		// The marker outlives every cache entry.
		fs::remove_dir_all(temp.path().join("v1")).unwrap();
		assert!(ratchet.active().unwrap());
	}

	#[test]
	fn migration_never_rescans() {
		let temp = tempfile::tempdir().unwrap();
		let ratchet = ratchet(temp.path());
		assert!(!ratchet.active().unwrap());
		assert!(ratchet.migrated().is_file() && !ratchet.marker().exists());
		// Written after the scan by a crabx without the ratchet: never seen, as the README says.
		entry(
			&temp.path().join("v1"),
			"1.0.0",
			&format!(r#"{{"source":"{ATTESTED}"}}"#),
		);
		assert!(!ratchet.active().unwrap());
		ratchet.activate().unwrap();
		assert!(ratchet.active().unwrap());
	}

	#[test]
	fn deleting_the_marker_resets_a_live_activation() {
		let temp = tempfile::tempdir().unwrap();
		let ratchet = ratchet(temp.path());
		// The verified release, cached once activation recorded it.
		entry(
			&temp.path().join("v2"),
			"1.0.0",
			&format!(r#"{{"source":"{ATTESTED}"}}"#),
		);
		ratchet.activate().unwrap();
		fs::remove_file(ratchet.marker()).unwrap();
		assert!(!ratchet.active().unwrap());
	}

	#[test]
	fn migration_fails_on_a_corrupt_record() {
		let temp = tempfile::tempdir().unwrap();
		let ratchet = ratchet(temp.path());
		entry(&temp.path().join("v1"), "1.0.0", "malformed");
		let error = format!("{:#}", ratchet.active().unwrap_err());
		let record = Path::new("v1").join("1.0.0").join("record.json");
		assert!(error.contains(&*record.to_string_lossy()), "{error}");
		assert!(!ratchet.migrated().exists());
	}

	#[test]
	fn an_active_ratchet_refuses_only_unattested_author_releases() {
		let temp = tempfile::tempdir().unwrap();
		let ratchet = ratchet(temp.path());
		let unattested = record("2.0.0", "author release (no published attestation)");
		let elsewhere = record(
			"2.0.0",
			"author release (repository does not support GitHub attestations)",
		);
		let attested = record("1.0.0", ATTESTED);
		let others = [
			record("2.0.0", "quickinstall"),
			record("2.0.0", "cargo install --locked"),
		];
		// --require-attestation refuses before anything was verified.
		for record in [&unattested, &elsewhere] {
			let error = ratchet.admit(record, true).unwrap_err().to_string();
			assert!(
				error.contains("tool@2.0.0") && error.contains("--require-attestation"),
				"{error}"
			);
			assert!(!ratchet.eligible(record, true).unwrap());
			assert!(ratchet.admit(record, false).unwrap().is_some());
			assert!(ratchet.eligible(record, false).unwrap());
		}
		ratchet.activate().unwrap();
		for record in [&unattested, &elsewhere] {
			let error = ratchet.admit(record, false).unwrap_err().to_string();
			assert!(error.contains(&ratchet.marker().display().to_string()), "{error}");
			assert!(!ratchet.eligible(record, false).unwrap());
		}
		for record in others.iter().chain([&attested]) {
			for required in [false, true] {
				assert!(ratchet.admit(record, required).unwrap().is_none());
				assert!(ratchet.eligible(record, required).unwrap());
			}
		}
	}

	#[test]
	fn activation_waits_for_admitted_releases() {
		let temp = tempfile::tempdir().unwrap();
		let ratchet = ratchet(temp.path());
		let unattested = record("2.0.0", "author release (no published attestation)");
		let admitted = ratchet.admit(&unattested, false).unwrap().unwrap();
		// Another admission shares the lock; an activation would wait for both.
		let second = ratchet.admit(&unattested, false).unwrap().unwrap();
		let probe = ratchet.lock().unwrap();
		assert!(matches!(probe.try_lock(), Err(fs::TryLockError::WouldBlock)));
		drop((admitted, second));
		probe.try_lock().unwrap();
		drop(probe);
		ratchet.activate().unwrap();
		assert!(ratchet.admit(&unattested, false).is_err());
	}
}
