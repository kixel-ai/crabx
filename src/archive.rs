//! Unpacking a verified release archive within fixed budgets.
//!
//! Nothing here runs until every signature and attestation the archive publishes has passed, and
//! even then the archive is hostile input: each budget is checked before the bytes or paths it
//! guards are written.

use anyhow::{Context, Result, bail};
use binstalk::manifests::cargo_toml_binstall::{CompressionFmt, PkgFmt, PkgFmtDecomposed, TarBasedFmt};
use std::{
	collections::HashSet,
	fs::{self, File},
	io::{self, BufRead, BufReader, Read},
	path::{Component, Path, PathBuf},
};

/// How much one installation may stage and unpack.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
	/// Bytes of the downloaded archive.
	pub archive: u64,
	/// Bytes decompressed from it, which bounds what extraction writes.
	pub extracted: u64,
	/// Files and directories extraction creates, implicit parents included.
	pub paths: usize,
}

impl Limits {
	/// Far above any Rust CLI's release, and far below a cache volume.
	pub const RELEASE: Self = Self {
		archive: 512 << 20,
		extracted: 2 << 30,
		paths: 10_000,
	};
}

/// Unpack `archive`, a `fmt` package, to `dst`, which must not exist yet.
pub fn extract(archive: tempfile::NamedTempFile, fmt: PkgFmt, dst: &Path, limits: Limits) -> Result<()> {
	match fmt.decompose() {
		// The archive budget already bounds it.
		PkgFmtDecomposed::Bin => {
			archive.persist_noclobber(dst)?;
		}
		PkgFmtDecomposed::CompressedBin(compression) => {
			let input = decoder(Some(compression), BufReader::new(archive.reopen()?))?;
			let mut input = Limited::new(input, limits.extracted, "release archive");
			io::copy(&mut input, &mut File::create_new(dst)?)?;
		}
		PkgFmtDecomposed::Tar(fmt) => {
			let compression = match fmt {
				TarBasedFmt::Tar => None,
				TarBasedFmt::Tbz2 => Some(CompressionFmt::Bz2),
				TarBasedFmt::Tgz => Some(CompressionFmt::Gz),
				TarBasedFmt::Txz => Some(CompressionFmt::Xz),
				TarBasedFmt::Tzstd => Some(CompressionFmt::Zst),
			};
			let input = decoder(compression, BufReader::new(archive.reopen()?))?;
			tar(input, dst, limits)?;
		}
		PkgFmtDecomposed::Zip => zip(archive.as_file(), dst, limits)?,
	}
	Ok(())
}

fn decoder<'a>(compression: Option<CompressionFmt>, input: impl BufRead + 'a) -> Result<Box<dyn Read + 'a>> {
	// Concatenated streams, as `cat a.gz b.gz` and parallel compressors write, are one file.
	Ok(match compression {
		None => Box::new(input),
		Some(CompressionFmt::Gz) => Box::new(flate2::bufread::MultiGzDecoder::new(input)),
		Some(CompressionFmt::Bz2) => Box::new(bzip2::bufread::MultiBzDecoder::new(input)),
		Some(CompressionFmt::Xz) => Box::new(liblzma::bufread::XzDecoder::new_multi_decoder(input)),
		Some(CompressionFmt::Zst) => Box::new(zstd::stream::read::Decoder::with_buffer(input)?),
	})
}

fn tar(input: impl Read, dst: &Path, limits: Limits) -> Result<()> {
	let mut paths = Paths::new(dst, limits.paths)?;
	// Counting the whole decompressed stream also bounds headers and entries that are skipped.
	let mut archive = tar::Archive::new(Limited::new(input, limits.extracted, "release archive"));
	for entry in archive.entries()? {
		let mut entry = entry?;
		let kind = entry.header().entry_type();
		// Links and special files are never unpacked, as binstalk never did.
		if kind.is_file() {
			let mut file = paths.file(&entry.path()?)?;
			io::copy(&mut entry, &mut file)?;
		} else if kind.is_dir() {
			paths.dir(&entry.path()?)?;
		}
	}
	Ok(())
}

fn zip(archive: &File, dst: &Path, limits: Limits) -> Result<()> {
	use rc_zip_sync::{ReadZip, rc_zip::parse::EntryKind};
	let mut paths = Paths::new(dst, limits.paths)?;
	let mut remaining = limits.extracted;
	for entry in archive.read_zip()?.entries() {
		let name = entry
			.sanitized_name()
			.with_context(|| format!("archive entry {:?} leaves the extraction directory", entry.name))?;
		match entry.kind() {
			EntryKind::Directory => paths.dir(Path::new(name))?,
			EntryKind::File => {
				let mut file = paths.file(Path::new(name))?;
				let mut input = Limited {
					remaining,
					..Limited::new(entry.reader(), limits.extracted, "release archive")
				};
				io::copy(&mut input, &mut file)?;
				remaining = input.remaining;
			}
			// Like a tarball's, a zip's links are never unpacked.
			EntryKind::Symlink => (),
		}
	}
	Ok(())
}

/// The files and directories extraction has created under its root.
struct Paths {
	root: PathBuf,
	created: HashSet<PathBuf>,
	limit: usize,
}

impl Paths {
	fn new(root: &Path, limit: usize) -> Result<Self> {
		fs::create_dir(root)?;
		Ok(Self {
			root: root.to_owned(),
			created: HashSet::new(),
			limit,
		})
	}

	/// Count `name` and each parent it implies, then return where it goes, or none for the root.
	fn claim(&mut self, name: &Path) -> Result<Option<PathBuf>> {
		let mut relative = PathBuf::new();
		for component in name.components() {
			match component {
				Component::Normal(part) => relative.push(part),
				Component::CurDir => (),
				_ => bail!("archive entry {} leaves the extraction directory", name.display()),
			}
		}
		if relative.as_os_str().is_empty() {
			return Ok(None);
		}
		for path in relative.ancestors().filter(|path| !path.as_os_str().is_empty()) {
			if self.created.insert(path.to_owned()) && self.created.len() > self.limit {
				bail!("release archive holds more than {} files and directories", self.limit);
			}
		}
		Ok(Some(self.root.join(relative)))
	}

	fn dir(&mut self, name: &Path) -> Result<()> {
		if let Some(path) = self.claim(name)? {
			fs::create_dir_all(path)?;
		}
		Ok(())
	}

	fn file(&mut self, name: &Path) -> Result<File> {
		let path = self.claim(name)?.context("archive file entry has no name")?;
		if let Some(parent) = path.parent() {
			fs::create_dir_all(parent)?;
		}
		File::create_new(&path).with_context(|| format!("archive repeats {}", name.display()))
	}
}

/// A reader that fails once it would pass `limit` bytes, rather than truncating.
pub struct Limited<R> {
	inner: R,
	remaining: u64,
	limit: u64,
	what: &'static str,
}

impl<R> Limited<R> {
	pub fn new(inner: R, limit: u64, what: &'static str) -> Self {
		Self {
			inner,
			remaining: limit,
			limit,
			what,
		}
	}
}

impl<R: Read> Read for Limited<R> {
	fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
		// Ask for one byte past the budget, so a stream that exceeds it is an error.
		let want = usize::try_from(self.remaining.saturating_add(1)).map_or(buf.len(), |most| buf.len().min(most));
		let read = self.inner.read(&mut buf[..want])?;
		self.remaining = self
			.remaining
			.checked_sub(read as u64)
			.ok_or_else(|| io::Error::other(format!("{} unpacks to more than {} bytes", self.what, self.limit)))?;
		Ok(read)
	}
}

#[cfg(test)]
pub mod tests {
	use super::*;
	use std::io::Write;

	const SMALL: Limits = Limits {
		archive: 1 << 20,
		extracted: 64 << 10,
		paths: 8,
	};

	/// A tarball of `(path, contents)` regular files, written byte-for-byte so tests can hold
	/// paths the tar crate's builder refuses.
	pub fn tarball(entries: &[(&str, &[u8])]) -> Vec<u8> {
		let mut builder = tar::Builder::new(Vec::new());
		for (path, contents) in entries {
			let mut header = tar::Header::new_old();
			header.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
			header.set_size(contents.len() as u64);
			header.set_entry_type(if path.ends_with('/') {
				tar::EntryType::Directory
			} else {
				tar::EntryType::Regular
			});
			header.set_mode(0o755);
			header.set_cksum();
			builder.append(&header, *contents).unwrap();
		}
		builder.into_inner().unwrap()
	}

	pub fn gzip(bytes: &[u8]) -> Vec<u8> {
		let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
		encoder.write_all(bytes).unwrap();
		encoder.finish().unwrap()
	}

	fn staged(dir: &Path, bytes: &[u8]) -> tempfile::NamedTempFile {
		let mut file = tempfile::NamedTempFile::new_in(dir).unwrap();
		file.write_all(bytes).unwrap();
		file
	}

	/// Every regular file under `root`, with the bytes written in total.
	fn written(root: &Path) -> (usize, u64) {
		let mut found = (0, 0);
		let mut pending = vec![root.to_owned()];
		while let Some(dir) = pending.pop() {
			for entry in fs::read_dir(dir).unwrap() {
				let entry = entry.unwrap();
				let kind = entry.file_type().unwrap();
				found.0 += 1;
				if kind.is_dir() {
					pending.push(entry.path());
				} else {
					found.1 += entry.metadata().unwrap().len();
				}
			}
		}
		found
	}

	fn unpack(fmt: PkgFmt, bytes: &[u8], limits: Limits) -> (tempfile::TempDir, Result<()>) {
		let temp = tempfile::tempdir().unwrap();
		let archive = staged(temp.path(), bytes);
		let result = extract(archive, fmt, &temp.path().join("out"), limits);
		(temp, result)
	}

	#[test]
	fn tarballs_unpack_files_and_directories_inside_the_root() {
		let tar = tarball(&[
			("./", b""),
			("./pkg/", b""),
			("./pkg/bin/tool", b"binary"),
			("README", b"hi"),
		]);
		let (temp, result) = unpack(PkgFmt::Tgz, &gzip(&tar), SMALL);
		result.unwrap();
		let out = temp.path().join("out");
		assert_eq!(fs::read(out.join("pkg/bin/tool")).unwrap(), b"binary");
		assert_eq!(written(&out), (4, 8));
		// The staged archive is gone once unpacked.
		assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
		// A plain binary is moved, not copied.
		let (temp, result) = unpack(PkgFmt::Bin, b"binary", SMALL);
		result.unwrap();
		assert_eq!(fs::read(temp.path().join("out")).unwrap(), b"binary");
		assert_eq!(fs::read_dir(temp.path()).unwrap().count(), 1);
	}

	#[test]
	fn unsafe_or_repeated_paths_are_errors() {
		for path in ["../escape", "a/../../escape", "/rooted"] {
			let (temp, result) = unpack(PkgFmt::Tar, &tarball(&[(path, b"x")]), SMALL);
			let error = format!("{:#}", result.unwrap_err());
			assert!(error.contains("leaves the extraction directory"), "{error}");
			assert_eq!(written(&temp.path().join("out")), (0, 0), "{path}");
		}
		let (_, result) = unpack(PkgFmt::Tar, &tarball(&[("tool", b"1"), ("./tool", b"2")]), SMALL);
		assert!(format!("{:#}", result.unwrap_err()).contains("archive repeats"));
	}

	#[test]
	fn the_path_budget_counts_implicit_parents_before_creating_anything() {
		// Five paths: a, a/b, a/b/c, a/b/c/d and the file.
		let deep = tarball(&[("a/b/c/d/tool", b"x")]);
		unpack(PkgFmt::Tar, &deep, Limits { paths: 5, ..SMALL }).1.unwrap();
		let (temp, result) = unpack(PkgFmt::Tar, &deep, Limits { paths: 4, ..SMALL });
		assert!(format!("{:#}", result.unwrap_err()).contains("more than 4 files and directories"));
		assert_eq!(written(&temp.path().join("out")), (0, 0));
		// Many empty files stop at the budget.
		let names: Vec<_> = (0..100).map(|i| format!("f{i}")).collect();
		let many: Vec<_> = names.iter().map(|name| (name.as_str(), &b""[..])).collect();
		let (temp, result) = unpack(PkgFmt::Tgz, &gzip(&tarball(&many)), SMALL);
		assert!(format!("{:#}", result.unwrap_err()).contains("more than 8 files"));
		assert_eq!(written(&temp.path().join("out")), (8, 0));
	}

	#[test]
	fn decompression_bombs_stop_at_the_byte_budget() {
		let zeros = vec![0; 1 << 20];
		let bomb = gzip(&tarball(&[("tool", &zeros)]));
		assert!(bomb.len() < 4 << 10);
		let (temp, result) = unpack(PkgFmt::Tgz, &bomb, SMALL);
		assert!(format!("{:#}", result.unwrap_err()).contains("unpacks to more than 65536 bytes"));
		assert!(written(&temp.path().join("out")).1 <= SMALL.extracted);
		// A compressed single binary, and the same within budget.
		let (temp, result) = unpack(PkgFmt::Gz, &gzip(&zeros), SMALL);
		assert!(result.is_err());
		assert!(fs::metadata(temp.path().join("out")).unwrap().len() <= SMALL.extracted);
		let (temp, result) = unpack(PkgFmt::Gz, &gzip(&zeros[..1000]), SMALL);
		result.unwrap();
		assert_eq!(fs::read(temp.path().join("out")).unwrap(), &zeros[..1000]);
	}

	/// A zip of stored `(name, contents)` files.
	fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
		let (mut out, mut central) = (Vec::new(), Vec::new());
		for (name, data) in entries {
			let offset = out.len() as u32;
			let mut crc = flate2::Crc::new();
			crc.update(data);
			let fields = |out: &mut Vec<u8>| {
				out.extend(20u16.to_le_bytes()); // version needed
				out.extend(0u16.to_le_bytes()); // flags
				out.extend(0u16.to_le_bytes()); // stored
				out.extend(0u32.to_le_bytes()); // time and date
				out.extend(crc.sum().to_le_bytes());
				out.extend((data.len() as u32).to_le_bytes());
				out.extend((data.len() as u32).to_le_bytes());
				out.extend((name.len() as u16).to_le_bytes());
				out.extend(0u16.to_le_bytes()); // extra
			};
			out.extend(0x0403_4b50u32.to_le_bytes());
			fields(&mut out);
			out.extend(name.as_bytes());
			out.extend(*data);
			central.extend(0x0201_4b50u32.to_le_bytes());
			central.extend(20u16.to_le_bytes()); // made by
			fields(&mut central);
			central.extend([0; 8]); // comment, disk, internal and external attributes
			central.extend(0u16.to_le_bytes());
			central.extend(offset.to_le_bytes());
			central.extend(name.as_bytes());
		}
		let (start, size, count) = (out.len() as u32, central.len() as u32, entries.len() as u16);
		out.extend(central);
		out.extend(0x0605_4b50u32.to_le_bytes());
		out.extend([0; 4]);
		out.extend(count.to_le_bytes());
		out.extend(count.to_le_bytes());
		out.extend(size.to_le_bytes());
		out.extend(start.to_le_bytes());
		out.extend(0u16.to_le_bytes());
		out
	}

	#[test]
	fn zips_share_one_byte_budget_across_entries() {
		let half = vec![7; 40 << 10];
		let (temp, result) = unpack(PkgFmt::Zip, &zip(&[("pkg/tool", b"binary")]), SMALL);
		result.unwrap();
		assert_eq!(fs::read(temp.path().join("out/pkg/tool")).unwrap(), b"binary");
		let (temp, result) = unpack(PkgFmt::Zip, &zip(&[("a", &half), ("b", &half)]), SMALL);
		assert!(format!("{:#}", result.unwrap_err()).contains("unpacks to more than"));
		assert!(written(&temp.path().join("out")).1 <= SMALL.extracted);
	}
}
