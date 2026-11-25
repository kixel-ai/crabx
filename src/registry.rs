//! A release's manifest from crates.io, read within fixed budgets.
//!
//! The `.crate` is checked against the index's checksum before it is decompressed, and its tar
//! stream, manifest and binary targets are each bounded before crabx holds them.

use crate::{archive::Limited, index, network};
use anyhow::{Context, Result, ensure};
use binstalk::manifests::cargo_toml_binstall::Meta;
use cargo_toml::{AbstractFilesystem, Manifest};
use semver::Version;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
	collections::{BTreeMap, HashSet},
	fs::File,
	io::{self, BufReader, Read},
	path::{Component, Path, PathBuf},
};

/// The most bytes of a `.crate`: crates.io accepts 10 MB unless it raised a crate's limit.
const CRATE_BYTES: u64 = 64 << 20;
/// The most bytes a `.crate` may decompress to, crates.io's own limit.
const UNPACKED_BYTES: u64 = 512 << 20;
/// The most bytes of `Cargo.toml`, which runs to tens of kilobytes for the largest crates.
const MANIFEST_BYTES: u64 = 4 << 20;
/// The most binary target paths (`src/main.rs`, `src/bin/...`) kept to discover binaries.
const TARGETS: usize = 4096;

/// `name@version`'s manifest, with binaries Cargo would discover from its files.
pub async fn manifest(client: &reqwest::Client, name: &str, version: &Version, stage: &Path) -> Result<Manifest<Meta>> {
	let response = client.get(index::url(name)).send().await?;
	ensure!(
		response.status() != reqwest::StatusCode::NOT_FOUND,
		"crates.io has no crate named {name}"
	);
	let file = network::spool(response.error_for_status()?, stage, index::INDEX_BYTES).await?;
	let release = find(BufReader::new(File::open(file.path())?), version)?
		.with_context(|| format!("crates.io has no unyanked release {name}@{version}"))?;
	let url = format!(
		"https://static.crates.io/crates/{}/{}/download",
		release.name, release.vers
	);
	let response = client.get(url).send().await?.error_for_status()?;
	let file = network::spool(response, stage, CRATE_BYTES)
		.await
		.with_context(|| format!("download {name}@{version}'s .crate"))?;
	let mut digest = Sha256::new();
	let mut reader = File::open(file.path())?;
	let mut buffer = vec![0; 64 << 10];
	loop {
		match reader.read(&mut buffer)? {
			0 => break,
			read => digest.update(&buffer[..read]),
		}
	}
	ensure!(
		<[u8; 32]>::from(digest.finalize()) == release.checksum()?,
		"{name}@{version}'s .crate does not match crates.io's index"
	);
	read(
		File::open(file.path())?,
		&format!("{}-{}", release.name, release.vers),
		UNPACKED_BYTES,
	)
}

#[derive(Deserialize)]
struct Release {
	name: String,
	vers: String,
	cksum: String,
	yanked: bool,
}

impl Release {
	fn checksum(&self) -> Result<[u8; 32]> {
		let hex = self.cksum.as_bytes();
		ensure!(
			hex.len() == 64 && hex.iter().all(u8::is_ascii_hexdigit),
			"crates.io's index holds an invalid checksum"
		);
		let mut checksum = [0; 32];
		for (byte, pair) in checksum.iter_mut().zip(hex.chunks(2)) {
			*byte = u8::from_str_radix(std::str::from_utf8(pair)?, 16)?;
		}
		Ok(checksum)
	}
}

/// The unyanked index entry for `version`, ignoring build metadata as Cargo's `=` does.
fn find(reader: impl Read, version: &Version) -> Result<Option<Release>> {
	for release in serde_json::Deserializer::from_reader(reader).into_iter::<Release>() {
		let release = release.context("malformed crates.io index")?;
		if !release.yanked && Version::parse(&release.vers).is_ok_and(|vers| vers.cmp_precedence(version).is_eq()) {
			return Ok(Some(release));
		}
	}
	Ok(None)
}

/// The manifest in a `.crate`'s `root` directory, reading at most `unpacked` decompressed bytes.
fn read(crate_file: impl Read, root: &str, unpacked: u64) -> Result<Manifest<Meta>> {
	let input = flate2::read::GzDecoder::new(crate_file);
	let mut archive = tar::Archive::new(Limited::new(input, unpacked, "the .crate"));
	let mut manifest = None;
	let mut targets = Targets::default();
	for entry in archive.entries()? {
		let mut entry = entry?;
		let Some(path) = relative(&entry.path()?, root) else {
			continue;
		};
		if path == Path::new("Cargo.toml") {
			ensure!(
				entry.size() <= MANIFEST_BYTES,
				"the .crate's Cargo.toml is larger than {MANIFEST_BYTES} bytes"
			);
			// The same name can appear again in a tarball; like tar, the last one wins.
			let mut bytes = Vec::with_capacity(entry.size() as usize);
			entry.read_to_end(&mut bytes)?;
			manifest = Some(bytes);
		}
		if path == Path::new("Cargo.toml") || path == Path::new("src/main.rs") || path.starts_with("src/bin") {
			targets.add(&path)?;
		}
	}
	let manifest = manifest.context("the .crate has no Cargo.toml")?;
	let mut manifest = Manifest::<Meta>::from_slice_with_metadata(&manifest).context("invalid Cargo.toml")?;
	manifest
		.complete_from_abstract_filesystem::<cargo_toml::Value, _>(targets, None)
		.context("invalid Cargo.toml")?;
	Ok(manifest)
}

/// `path` inside `root`, without `.` components, or none if it lies elsewhere.
fn relative(path: &Path, root: &str) -> Option<PathBuf> {
	let mut components = path.components().filter(|c| *c != Component::CurDir);
	if components.next()? != Component::Normal(root.as_ref()) {
		return None;
	}
	let mut relative = PathBuf::new();
	for component in components {
		let Component::Normal(part) = component else {
			return None;
		};
		relative.push(part);
	}
	Some(relative)
}

/// The `.crate`'s binary target files, by directory, for Cargo's target discovery.
#[derive(Default)]
struct Targets {
	dirs: BTreeMap<PathBuf, HashSet<Box<str>>>,
	count: usize,
}

impl Targets {
	fn add(&mut self, path: &Path) -> Result<()> {
		self.count += 1;
		ensure!(
			self.count <= TARGETS,
			"the .crate has more than {TARGETS} binary target files"
		);
		let mut path = path;
		while let Some(parent) = path.parent() {
			let Some(name) = path.file_name() else { break };
			self.dirs
				.entry(parent.to_owned())
				.or_default()
				.insert(name.to_string_lossy().into());
			path = parent;
		}
		Ok(())
	}
}

impl AbstractFilesystem for Targets {
	fn file_names_in(&self, rel_path: &str) -> io::Result<HashSet<Box<str>>> {
		let dir: PathBuf = Path::new(rel_path)
			.components()
			.filter(|c| matches!(c, Component::Normal(_)))
			.collect();
		Ok(self.dirs.get(&dir).cloned().unwrap_or_default())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::archive::tests::{gzip, tarball};

	fn manifest(bins: &str) -> Vec<u8> {
		format!("[package]\nname = \"fixture\"\nversion = \"1.0.0\"\nedition = \"2021\"\n{bins}").into_bytes()
	}

	#[test]
	fn binaries_are_discovered_from_the_crates_own_directory() {
		let toml = manifest("");
		let tar = tarball(&[
			("fixture-1.0.0/Cargo.toml", &toml),
			("fixture-1.0.0/src/main.rs", b"fn main() {}"),
			("fixture-1.0.0/src/bin/extra.rs", b"fn main() {}"),
			// Outside the crate's directory, so never a target.
			("other/src/bin/stray.rs", b"fn main() {}"),
		]);
		let manifest = read(&gzip(&tar)[..], "fixture-1.0.0", UNPACKED_BYTES).unwrap();
		let mut names: Vec<_> = manifest.bin.iter().map(|bin| bin.name.clone().unwrap()).collect();
		names.sort();
		assert_eq!(names, ["extra", "fixture"]);
		assert!(read(&gzip(&tar)[..], "other-1.0.0", UNPACKED_BYTES).is_err());
	}

	#[test]
	fn oversized_crates_and_manifests_are_refused_before_reading() {
		// A declared size past the limit is refused before any of it is buffered.
		let big = vec![b'#'; MANIFEST_BYTES as usize + 1];
		let tar = tarball(&[("fixture-1.0.0/Cargo.toml", &big)]);
		let crate_file = gzip(&tar);
		let error = format!(
			"{:#}",
			read(&crate_file[..], "fixture-1.0.0", UNPACKED_BYTES).unwrap_err()
		);
		assert!(error.contains("Cargo.toml is larger than"), "{error}");
		// The whole tar stream counts, including entries no one reads.
		let toml = manifest("");
		let padding = vec![0; 1 << 20];
		let tar = tarball(&[("fixture-1.0.0/padding", &padding), ("fixture-1.0.0/Cargo.toml", &toml)]);
		let crate_file = gzip(&tar);
		assert!(crate_file.len() < 8 << 10);
		read(&crate_file[..], "fixture-1.0.0", 2 << 20).unwrap();
		let error = format!("{:#}", read(&crate_file[..], "fixture-1.0.0", 1 << 20).unwrap_err());
		assert!(error.contains("unpacks to more than"), "{error}");
	}

	#[test]
	fn the_index_entry_must_be_unyanked_and_well_formed() {
		let line = |vers: &str, yanked: bool, cksum: &str| {
			format!(r#"{{"name":"Fixture","vers":"{vers}","cksum":"{cksum}","yanked":{yanked},"deps":[]}}"#)
		};
		let good = "ab".repeat(32);
		let index = [
			line("1.0.0", true, &good),
			line("1.0.1+build", false, &good),
			line("2.0.0", false, "zz"),
		]
		.join("\n");
		let find = |vers: &str| find(index.as_bytes(), &Version::parse(vers).unwrap()).unwrap();
		assert!(find("1.0.0").is_none());
		let release = find("1.0.1").unwrap();
		assert_eq!(
			(release.name.as_str(), release.vers.as_str()),
			("Fixture", "1.0.1+build")
		);
		assert_eq!(release.checksum().unwrap(), [0xab; 32]);
		assert!(find("2.0.0").unwrap().checksum().is_err());
		assert!(super::find(&b"{"[..], &Version::new(1, 0, 0)).is_err());
	}
}
