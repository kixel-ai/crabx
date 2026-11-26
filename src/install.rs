use crate::{
	archive,
	cache::{self, Cache, Record},
	cargo,
	cli::Args,
	fetchers::{
		self, Data, Fetcher, GhCrateMeta, QuickInstall, SignaturePolicy, TargetArchOverride, TargetData,
		TargetDataErased,
	},
	network, origin,
	ratchet::Ratchet,
	registry,
	target::TargetTriple,
	trust,
};
use anyhow::{Context, Result, bail, ensure};
use binstalk::{
	helpers::{gh_api_client::GhApiClient, remote, tasks::AutoAbortJoinHandle},
	manifests::cargo_toml_binstall::{PkgMeta, Strategy},
};
use binstalk_bins::{self as bins, BinFile};
use semver::{Version, VersionReq};
use std::{collections::BTreeSet, fs, path::Path, str::FromStr, sync::Arc, time::Duration};

/// Install exactly `name@version` into the cache, unless it is already there.
pub async fn resolve(cache: &Cache, name: &str, version: &Version, args: &Args) -> Result<Record> {
	let _lock = cache.lock()?;
	if let Some(record) = cache.load(version)? {
		return Ok(record);
	}
	install(cache, name, version, args).await
}

/// Download `name@version` again over its cached entry, which the attestation policy now refuses,
/// in case its author attested it since it was cached.
pub async fn reinstall(cache: &Cache, name: &str, version: &Version, args: &Args) -> Result<Record> {
	let _lock = cache.lock()?;
	install(cache, name, version, args).await
}

/// Install `name@version`, replacing any cached entry, while holding the installation lock.
async fn install(cache: &Cache, name: &str, version: &Version, args: &Args) -> Result<Record> {
	let stage = cache.stage()?;
	fs::create_dir(stage.path().join("bin"))?;
	// Prebuilt Linux binaries need glibc, so a musl crabx always compiles.
	let prebuilt = if args.build || cfg!(all(target_os = "linux", target_env = "musl")) {
		None
	} else {
		fetch(stage.path(), name, version, args.allow_quickinstall, &cache.ratchet).await?
	};
	let record = match prebuilt {
		Some(record) => record,
		None => {
			if !args.build {
				// A source build carries no attestation, so it would slip past the requirement.
				ensure!(
					!args.require_attestation,
					"no usable prebuilt release for {name}@{version} on {}, and --require-attestation refuses to compile it",
					binstalk::TARGET
				);
				eprintln!(
					"crabx: no usable prebuilt release for {name}@{version} on {}; compiling it with cargo install",
					binstalk::TARGET
				);
			}
			let mut record = build(stage.path(), name, version)?;
			if !args.build {
				record.source = cache::FALLBACK.into();
			}
			record
		}
	};
	// Held across publishing, so a concurrent activation refuses this release instead of caching it.
	let _admitted = cache.ratchet.admit(&record, args.require_attestation)?;
	cache::atomic(&stage.path().join("record.json"), &serde_json::to_vec(&record)?)?;
	let entry = cache.version(&record.version);
	if entry.try_exists()? {
		// Moved into a staging directory, which the next install reclaims, as a running binary may still use it.
		let replaced = tempfile::Builder::new()
			.prefix("install-")
			.tempdir_in(&cache.root)?
			.keep();
		fs::rename(&entry, replaced.join("replaced"))?;
	}
	fs::rename(stage.path(), entry)?;
	// The rename moved the directory, so there is nothing left to clean up.
	let _ = stage.keep();
	Ok(record)
}

struct Package<'a> {
	/// Activated as soon as an attestation verifies, even if installing then fails.
	ratchet: &'a Ratchet,
	name: String,
	version: Version,
	repository: Option<String>,
	meta: PkgMeta,
	bins: Vec<(String, bool)>,
}

fn download_builder(redirects: &origin::Redirects) -> reqwest::ClientBuilder {
	remote::Client::default_builder(
		concat!("crabx/", env!("CARGO_PKG_VERSION")),
		None,
		false,
		Default::default(),
		&mut std::iter::empty(),
	)
	.redirect(redirects.policy())
	.timeout(Duration::from_secs(300))
	.read_timeout(Duration::from_secs(30))
}

/// Install the first usable prebuilt release of `name@version`, or None when none runs here.
async fn fetch(
	stage: &Path,
	name: &str,
	version: &Version,
	quickinstall: bool,
	ratchet: &Ratchet,
) -> Result<Option<Record>> {
	let redirects = origin::Redirects::default();
	let client = remote::Client::from_builder(download_builder(&redirects), 1000.try_into()?, 100.try_into()?)?;
	let req = VersionReq::parse(&format!("={version}"))?;
	let manifest = registry::manifest(client.get_inner(), &name.to_ascii_lowercase(), version, stage).await?;
	let mut package = manifest.package.context("crate manifest has no package")?;
	ensure!(
		package.name.eq_ignore_ascii_case(name),
		"registry returned a different crate"
	);
	let version = package.version().clone();
	ensure!(req.matches(&version), "registry returned a different version");
	let repository = package.repository().map(str::to_owned);
	let meta = package.metadata.take().and_then(|m| m.binstall).unwrap_or_default();
	let mut names = BTreeSet::new();
	let mut binaries = Vec::new();
	for bin in manifest.bin {
		let name = bin.name.context("crate manifest has an unnamed binary")?;
		ensure!(
			bins::is_valid_bin_name(&name) && names.insert(name.clone()),
			"invalid or duplicate binary name {name:?}"
		);
		binaries.push((name, bin.required_features.is_empty()));
	}
	ensure!(!binaries.is_empty(), "crate contains no binaries");
	let package = Package {
		ratchet,
		name: package.name,
		version,
		repository,
		meta,
		bins: binaries,
	};
	let gh = GhApiClient::new(
		client.clone(),
		network::github_token()?.map(|token| token.into_boxed_str().into()),
	);
	// With a token binstalk prefers GraphQL and never retries on REST, so an exhausted GraphQL
	// budget would fail discovery; REST shares the budget trust.rs already spends on attestations.
	gh.set_only_use_restful_api();
	let data = Arc::new(Data::new(
		package.name.as_str().into(),
		package.version.to_string().into(),
		package.repository.clone(),
	));
	// The author can name a single binary differently from its package.
	let alternate = match package.bins.as_slice() {
		[(name, _)] if name != &package.name => Some((
			name.clone(),
			Arc::new(Data::new(
				name.as_str().into(),
				package.version.to_string().into(),
				package.repository.clone(),
			)),
		)),
		_ => None,
	};
	let tiers = host_targets()
		.into_iter()
		.map(|tier| {
			tier.into_iter()
				.map(|target| -> Result<Arc<TargetDataErased>> {
					let target_related_info = TargetTriple::from_str(&target)?;
					let meta = package.meta.merge_overrides(
						package
							.meta
							.overrides
							.get_matching(&target, &target_related_info.cfgs()),
					);
					Ok(Arc::new(TargetData {
						target,
						meta,
						target_related_info,
					}))
				})
				.collect::<Result<Vec<_>>>()
		})
		.collect::<Result<Vec<_>>>()?;
	let names: Vec<_> = std::iter::once((package.name.clone(), data.clone()))
		.chain(alternate)
		.collect();
	let mut candidates = Vec::new();
	for (name, data, target) in ordered(&names, &tiers) {
		if !disabled(target, Strategy::CrateMetaData) {
			candidates.push((
				name.clone(),
				GhCrateMeta::new(
					client.clone(),
					gh.clone(),
					data.clone(),
					target.clone(),
					SignaturePolicy::IfPresent,
				),
			));
		}
	}
	let targets: Vec<_> = tiers.into_iter().flatten().collect();
	if quickinstall {
		for target in &targets {
			if !disabled(target, Strategy::QuickInstall) {
				candidates.push((
					package.name.clone(),
					QuickInstall::new(
						client.clone(),
						gh.clone(),
						data.clone(),
						target.clone(),
						SignaturePolicy::Require,
					),
				));
			}
		}
	}
	for (name, fetcher) in candidates {
		let found = tokio::time::timeout(
			Duration::from_secs(30),
			AutoAbortJoinHandle::new(fetcher.clone().find()),
		)
		.await
		.context("release discovery timed out")?;
		// Checked after each candidate, even a failed one, so only discovery that reached GitHub's
		// API needs the token.
		ensure!(!fetchers::token_rejected(), network::REJECTED_TOKEN);
		let found = found?.with_context(|| format!("look up {name}'s {} release", fetcher.target()))?;
		if !found {
			continue;
		}
		if let Some(record) = package.install(stage, &name, fetcher.as_ref(), &redirects).await? {
			return Ok(Some(record));
		}
	}
	Ok(None)
}

fn disabled(target: &TargetDataErased, strategy: Strategy) -> bool {
	target
		.meta
		.disabled_strategies
		.as_deref()
		.is_some_and(|strategies| strategies.contains(&strategy))
}

impl Package<'_> {
	async fn install(
		&self,
		stage: &Path,
		discovery_name: &str,
		fetcher: &dyn Fetcher,
		redirects: &origin::Redirects,
	) -> Result<Option<Record>> {
		let mut staged = tempfile::NamedTempFile::new_in(stage)?;
		fetcher.download(staged.as_file_mut()).await?;
		// quickinstall's archive is only ever minisign-checked; its releases carry no GitHub attestation.
		let source = if fetcher.is_third_party() {
			"quickinstall".to_owned()
		} else {
			let receipt = fetcher.receipt().context("missing archive digest")?;
			let origin = origin::resolve(receipt, redirects)?;
			trust::verify(
				receipt,
				trust::Subject {
					name: &self.name,
					version: &self.version,
					repo: origin.repo,
					asset: origin.asset,
				},
				stage,
			)
			.await?
		};
		if crate::ratchet::attested(&source) {
			self.ratchet.activate()?;
		}
		// Only now, with every published proof checked, does crabx decompress a byte.
		let download = stage.join("download");
		archive::extract(staged, fetcher.pkg_fmt(), &download, archive::Limits::RELEASE)?;
		let version = self.version.to_string();
		let install = stage.join("bin");
		// `bin-dir` must spell `{ target-arch }` as the URL discovery found did.
		let target_related_info = TargetArchOverride {
			values: &fetcher.target_data().target_related_info,
			target_arch: fetcher.resolved_target_arch(),
		};
		let data = bins::Data {
			name: &self.name,
			target: fetcher.target(),
			version: &version,
			repo: self.repository.as_deref(),
			meta: fetcher.target_meta(),
			bin_path: &download,
			install_path: &install,
			target_related_info: &target_related_info,
		};
		let bin_dir = layout(&data, &self.bins, discovery_name)?;
		let template = leon::Template::parse(&bin_dir)?;
		let mut files = Vec::new();
		let mut sources = BTreeSet::new();
		for (name, required) in &self.bins {
			let file = BinFile::new(&data, name, &template, &mut |p| download.join(p).is_dir(), true, &[])?;
			// Extraction creates no links, so this is a file the archive holds.
			if !file.source.is_file() {
				ensure!(
					!required,
					"release archive omits required binary {name:?} at {}",
					file.archive_source_path.display()
				);
				continue;
			}
			ensure!(
				sources.insert(file.source.clone()),
				"release maps several binaries to the same file"
			);
			// Only a static musl build for this machine runs on a glibc host; a dynamic one needs a loader it lacks.
			if fetcher.target().ends_with("-linux-musl") {
				let dynamic = elf_interpreter(&file.source, fetcher.target()).with_context(|| {
					format!(
						"{}@{}'s {} build is not a usable executable; use --build to compile explicitly",
						self.name,
						self.version,
						fetcher.target()
					)
				})?;
				// It needs a musl loader this host lacks, so the next candidate, or a source build, runs instead.
				if dynamic {
					remove(&download)?;
					return Ok(None);
				}
			}
			files.push(file);
		}
		ensure!(!files.is_empty(), "release contains no runnable binary");
		let mut binaries = Vec::new();
		for file in files {
			file.install_bin_noclobber()?;
			binaries.push(file.base_name.to_string());
		}
		remove(&download)?;
		Ok(Some(Record {
			version: self.version.clone(),
			bins: binaries,
			source,
		}))
	}
}

/// Remove the unpacked download, a directory or a single file.
fn remove(download: &Path) -> Result<()> {
	if download.is_dir() {
		fs::remove_dir_all(download)?;
	} else {
		fs::remove_file(download)?;
	}
	Ok(())
}

fn layout(data: &bins::Data<'_>, binaries: &[(String, bool)], discovery_name: &str) -> Result<String> {
	if let Some(template) = &data.meta.bin_dir {
		return Ok(template.clone());
	}
	let mut candidates = Vec::new();
	for name in [data.name, discovery_name] {
		let named = bins::Data {
			name,
			target: data.target,
			version: data.version,
			repo: data.repo,
			meta: data.meta.clone(),
			bin_path: data.bin_path,
			install_path: data.install_path,
			target_related_info: data.target_related_info,
		};
		let direct = bins::infer_bin_dir_template(&named, &mut |p| data.bin_path.join(p).is_dir()).into_owned();
		// Templates separate with `/` on every host, and leon reads Windows' `\` as an escape.
		let nested = match direct.rsplit_once('/') {
			Some((parent, _)) => format!("{parent}/bin/{{ bin }}{{ binary-ext }}"),
			None => "bin/{ bin }{ binary-ext }".to_owned(),
		};
		candidates.extend([direct, nested]);
	}
	for candidate in &candidates {
		let template = leon::Template::parse(candidate)?;
		let mut found = false;
		let mut complete = true;
		for (name, required) in binaries {
			let file = BinFile::new(
				data,
				name,
				&template,
				&mut |p| data.bin_path.join(p).is_dir(),
				true,
				&[],
			)?;
			let exists = file.source.is_file();
			found |= exists;
			complete &= exists || !required;
		}
		if found && complete {
			return Ok(candidate.to_owned());
		}
	}
	// Let installation report precisely which required binary is absent.
	Ok(candidates.remove(0))
}

fn build(stage: &Path, name: &str, version: &Version) -> Result<Record> {
	let mut command = cargo::Toolchain::new(&cargo::Caller::current()?)?.command()?;
	command
		.arg("install")
		.arg(name)
		.arg("--locked")
		.arg("--target")
		.arg(binstalk::TARGET)
		.arg("--root")
		.arg(stage)
		.arg("--index")
		.arg("sparse+https://index.crates.io/")
		.env_remove("GITHUB_TOKEN")
		.env_remove("GH_TOKEN")
		// The exact release the caller chose, never the newest Cargo would pick.
		.arg("--version")
		.arg(format!("={version}"));
	ensure!(
		command
			.status()
			.context("compiling needs cargo and a Rust toolchain")?
			.success(),
		"cargo install failed"
	);
	#[derive(serde::Deserialize)]
	struct Installs {
		installs: std::collections::BTreeMap<String, serde_json::Value>,
	}
	let installs: Installs = serde_json::from_reader(fs::File::open(stage.join(".crates2.json"))?)?;
	ensure!(
		installs.installs.len() == 1,
		"cargo wrote unexpected installation metadata"
	);
	let key = installs.installs.keys().next().context("cargo installed no crate")?;
	let mut parts = key.split_whitespace();
	ensure!(
		parts
			.next()
			.is_some_and(|installed| installed.eq_ignore_ascii_case(name)),
		"cargo installed the wrong crate"
	);
	let actual = Version::parse(parts.next().context("cargo omitted the version")?)?;
	ensure!(version == &actual, "cargo installed a different version");
	let mut bins = Vec::new();
	for entry in fs::read_dir(stage.join("bin"))? {
		let entry = entry?;
		ensure!(entry.file_type()?.is_file(), "cargo installed a non-file binary");
		let name = entry
			.file_name()
			.into_string()
			.map_err(|_| anyhow::anyhow!("non-UTF-8 binary name"))?;
		ensure!(cache::valid_binary(&name), "invalid installed binary name");
		bins.push(name);
	}
	bins.sort();
	Ok(Record {
		version: actual,
		bins,
		source: cache::BUILT.into(),
	})
}

/// Whether a 64-bit ELF executable for `target`'s machine names a program interpreter, so it links dynamically.
fn elf_interpreter(path: &Path, target: &str) -> Result<bool> {
	use std::io::{Read, Seek, SeekFrom};
	const PT_INTERP: u32 = 3;
	let machine = match target.split('-').next() {
		Some("x86_64") => 62,
		Some("aarch64") => 183,
		_ => bail!("no ELF machine is known for {target}"),
	};
	let mut file = fs::File::open(path)?;
	let length = file.metadata()?.len();
	let mut header = [0; 64];
	file.read_exact(&mut header).context("binary is too short for ELF")?;
	ensure!(header.starts_with(b"\x7fELF"), "binary is not ELF");
	ensure!(
		header[4] == 2 && header[5] == 1,
		"binary is not 64-bit little-endian ELF"
	);
	let u16_at = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]) as u64;
	ensure!(u16_at(0x12) == machine, "binary is not built for {target}");
	let offset = u64::from_le_bytes(header[0x20..0x28].try_into().unwrap());
	let (size, count, entry_size) = (u16_at(0x36), u16_at(0x38), 56);
	ensure!(
		size == entry_size && count > 0 && offset.checked_add(size * count).is_some_and(|end| end <= length),
		"binary has malformed or truncated ELF program headers"
	);
	// Linux's loader requires exactly its own entry size and at least one entry.
	let mut entry = [0; 4];
	for index in 0..count {
		file.seek(SeekFrom::Start(offset + index * size))?;
		file.read_exact(&mut entry)
			.context("binary has truncated ELF program headers")?;
		if u32::from_le_bytes(entry) == PT_INTERP {
			return Ok(true);
		}
	}
	Ok(false)
}

/// Pairs every name with every target, trying a tier's targets under each name before the next tier's.
fn ordered<'a, N, D, T>(names: &'a [(N, D)], tiers: &'a [Vec<T>]) -> impl Iterator<Item = (&'a N, &'a D, &'a T)> {
	tiers.iter().flat_map(move |tier| {
		names
			.iter()
			.flat_map(move |(name, data)| tier.iter().map(move |target| (name, data, target)))
	})
}

/// The targets this host runs, in tiers from most to least preferred.
fn host_targets() -> Vec<Vec<String>> {
	let mut tiers = vec![vec![binstalk::TARGET.to_owned()]];
	if cfg!(target_os = "macos") {
		// A crate-named universal archive still beats a binary-named native one.
		tiers[0].extend([
			"universal-apple-darwin".to_owned(),
			"universal2-apple-darwin".to_owned(),
		]);
	}
	// A static musl build runs on any glibc host, so it serves crates that ship no gnu one.
	let musl = match binstalk::TARGET {
		"x86_64-unknown-linux-gnu" => Some("x86_64-unknown-linux-musl"),
		"aarch64-unknown-linux-gnu" => Some("aarch64-unknown-linux-musl"),
		_ => None,
	};
	tiers.extend(musl.map(|target| vec![target.to_owned()]));
	tiers
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn every_name_tries_a_tier_before_the_next() {
		let names = [("crate", ()), ("bin", ())];
		let tiers = [vec!["gnu"], vec!["musl"]];
		let order: Vec<_> = ordered(&names, &tiers).map(|(n, _, t)| (*n, *t)).collect();
		assert_eq!(
			order,
			[("crate", "gnu"), ("bin", "gnu"), ("crate", "musl"), ("bin", "musl")]
		);
		let tiers = [vec!["native", "universal"]];
		let order: Vec<_> = ordered(&names, &tiers).map(|(n, _, t)| (*n, *t)).collect();
		assert_eq!(
			order,
			[
				("crate", "native"),
				("crate", "universal"),
				("bin", "native"),
				("bin", "universal")
			]
		);
		let expected: &[&[&str]] = match binstalk::TARGET {
			"x86_64-unknown-linux-gnu" => &[&["x86_64-unknown-linux-gnu"], &["x86_64-unknown-linux-musl"]],
			"aarch64-unknown-linux-gnu" => &[&["aarch64-unknown-linux-gnu"], &["aarch64-unknown-linux-musl"]],
			"aarch64-apple-darwin" => &[&[
				"aarch64-apple-darwin",
				"universal-apple-darwin",
				"universal2-apple-darwin",
			]],
			"x86_64-apple-darwin" => &[&[
				"x86_64-apple-darwin",
				"universal-apple-darwin",
				"universal2-apple-darwin",
			]],
			target => &[&[target]],
		};
		assert_eq!(host_targets(), expected);
		for target in host_targets().iter().flatten() {
			TargetTriple::from_str(target).unwrap();
		}
	}

	#[test]
	fn only_dynamic_elf_names_an_interpreter() {
		let temp = tempfile::tempdir().unwrap();
		// A 64-bit little-endian header with two program headers at offset 64, 56 bytes each.
		let elf = |second: u32| {
			let mut bytes = vec![0u8; 64 + 2 * 56];
			bytes[..6].copy_from_slice(b"\x7fELF\x02\x01");
			bytes[0x12..0x14].copy_from_slice(&62u16.to_le_bytes());
			bytes[0x20..0x28].copy_from_slice(&64u64.to_le_bytes());
			bytes[0x36..0x38].copy_from_slice(&56u16.to_le_bytes());
			bytes[0x38..0x3a].copy_from_slice(&2u16.to_le_bytes());
			bytes[64..68].copy_from_slice(&1u32.to_le_bytes());
			bytes[120..124].copy_from_slice(&second.to_le_bytes());
			bytes
		};
		let path = temp.path().join("bin");
		let musl = "x86_64-unknown-linux-musl";
		fs::write(&path, elf(3)).unwrap();
		assert!(elf_interpreter(&path, musl).unwrap());
		fs::write(&path, elf(1)).unwrap();
		assert!(!elf_interpreter(&path, musl).unwrap());
		// Built for another machine, or 32-bit.
		assert!(elf_interpreter(&path, "aarch64-unknown-linux-musl").is_err());
		let mut narrow = elf(1);
		narrow[4] = 1;
		fs::write(&path, narrow).unwrap();
		assert!(elf_interpreter(&path, musl).is_err());
		// Truncated anywhere inside the program-header table, even past the last type field.
		fs::write(&path, &elf(1)[..64 + 56 + 4]).unwrap();
		assert!(elf_interpreter(&path, musl).is_err());
		// Linux loads only its own entry size, and only a non-empty table.
		for (at, value) in [(0x36, 4u16), (0x36, 64), (0x38, 0)] {
			let mut malformed = elf(1);
			malformed.resize(64 + 2 * 64, 0);
			malformed[at..at + 2].copy_from_slice(&value.to_le_bytes());
			fs::write(&path, malformed).unwrap();
			assert!(elf_interpreter(&path, musl).is_err());
		}
		fs::write(&path, b"#!/bin/sh\n").unwrap();
		assert!(elf_interpreter(&path, musl).is_err());
		if cfg!(all(target_os = "linux", target_env = "gnu")) {
			// This test binary links glibc dynamically.
			assert!(elf_interpreter(&std::env::current_exe().unwrap(), binstalk::TARGET).unwrap());
		}
	}

	#[test]
	fn stalled_downloads_have_read_and_total_deadlines() {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};
		for (total, deadline) in [(false, 30), (true, 300)] {
			crate::network::runtime().unwrap().block_on(async {
				let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
				let address = listener.local_addr().unwrap();
				let (done, wait) = tokio::sync::oneshot::channel::<()>();
				let server = tokio::spawn(async move {
					let (mut stream, _) = listener.accept().await.unwrap();
					let mut request = Vec::new();
					while !request.ends_with(b"\r\n\r\n") {
						request.push(stream.read_u8().await.unwrap());
					}
					stream
						.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nx")
						.await
						.unwrap();
					let _ = wait.await;
				});
				let mut builder = download_builder(&origin::Redirects::default())
					.https_only(false)
					.no_proxy();
				if total {
					// Isolate the total deadline from the shorter read deadline.
					builder = builder.read_timeout(Duration::from_secs(600));
				}
				let mut response = builder
					.build()
					.unwrap()
					.get(format!("http://{address}"))
					.send()
					.await
					.unwrap();
				assert_eq!(response.chunk().await.unwrap().unwrap().as_ref(), b"x");
				tokio::time::pause();
				let chunk = response.chunk();
				tokio::pin!(chunk);
				assert!(std::future::poll_fn(|cx| std::task::Poll::Ready(chunk.as_mut().poll(cx).is_pending())).await);
				// Tokio rounds deadlines to its next millisecond tick. Poll directly
				// after that tick so auto-advance cannot hide a missing deadline.
				tokio::time::advance(Duration::from_secs(deadline) + Duration::from_millis(1)).await;
				let result = std::future::poll_fn(|cx| std::task::Poll::Ready(chunk.as_mut().poll(cx))).await;
				assert!(matches!(result, std::task::Poll::Ready(Err(error)) if error.is_timeout()));
				done.send(()).unwrap();
				server.await.unwrap();
			});
		}
	}

	#[test]
	fn author_layouts_include_nested_bin_and_respect_metadata() {
		let temp = tempfile::tempdir().unwrap();
		let target = TargetTriple::from_str(binstalk::TARGET).unwrap();
		let directory = format!("moq-cli-v0.14.16-{}", binstalk::TARGET);
		let nested = temp.path().join(&directory).join("bin");
		fs::create_dir_all(&nested).unwrap();
		let name = format!("moq{}", std::env::consts::EXE_SUFFIX);
		fs::write(nested.join(&name), b"native binary fixture").unwrap();
		let mut data = bins::Data {
			name: "moq-cli",
			target: binstalk::TARGET,
			version: "0.14.16",
			repo: None,
			meta: PkgMeta::default(),
			bin_path: temp.path(),
			install_path: temp.path(),
			target_related_info: &target,
		};
		let binaries = [("moq".to_owned(), true)];
		let source = |data: &bins::Data<'_>| {
			let chosen = layout(data, &binaries, "moq").unwrap();
			let template = leon::Template::parse(&chosen).unwrap();
			BinFile::new(
				data,
				"moq",
				&template,
				&mut |p| data.bin_path.join(p).is_dir(),
				true,
				&[],
			)
			.unwrap()
			.source
		};
		assert_eq!(source(&data), nested.join(&name));
		// A direct executable wins even when the archive also has a bin directory.
		let direct = temp.path().join(&directory).join(&name);
		fs::write(&direct, b"direct native fixture").unwrap();
		assert_eq!(source(&data), direct);
		// A differently named binary can also name the release's enclosing directory.
		fs::remove_dir_all(temp.path().join(&directory)).unwrap();
		let alternate = temp.path().join(format!("moq-v0.14.16-{}", binstalk::TARGET));
		fs::create_dir_all(alternate.join("bin")).unwrap();
		fs::write(alternate.join("bin").join(&name), b"alternate nested fixture").unwrap();
		assert_eq!(source(&data), alternate.join("bin").join(&name));
		fs::write(alternate.join(&name), b"alternate direct fixture").unwrap();
		assert_eq!(source(&data), alternate.join(&name));
		data.meta.bin_dir = Some("explicit/{ bin }{ binary-ext }".into());
		assert_eq!(source(&data), temp.path().join("explicit").join(&name));
	}

	/// A fetcher whose download writes `archive`, then fails its signature check unless `signed`.
	struct Fake {
		archive: Vec<u8>,
		signed: bool,
		third_party: bool,
		receipt: Option<fetchers::Receipt>,
		target: Arc<TargetDataErased>,
	}

	#[async_trait::async_trait]
	impl Fetcher for Fake {
		fn new(
			_: remote::Client,
			_: GhApiClient,
			_: Arc<Data>,
			_: Arc<TargetDataErased>,
			_: SignaturePolicy,
		) -> Arc<dyn Fetcher> {
			unreachable!()
		}
		async fn download(&self, archive: &mut fs::File) -> Result<(), fetchers::FetchError> {
			use std::io::Write;
			archive.write_all(&self.archive).unwrap();
			match self.signed {
				true => Ok(()),
				false => Err(fetchers::FetchError::InvalidSignature),
			}
		}
		fn find(self: Arc<Self>) -> tokio::task::JoinHandle<Result<bool, fetchers::FetchError>> {
			unreachable!()
		}
		fn receipt(&self) -> Option<&fetchers::Receipt> {
			self.receipt.as_ref()
		}
		fn pkg_fmt(&self) -> binstalk::manifests::cargo_toml_binstall::PkgFmt {
			binstalk::manifests::cargo_toml_binstall::PkgFmt::Tgz
		}
		fn target_meta(&self) -> PkgMeta {
			self.target.meta.clone()
		}
		fn source_name(&self) -> compact_str::CompactString {
			"fake".into()
		}
		fn fetcher_name(&self) -> &'static str {
			"Fake"
		}
		fn strategy(&self) -> Strategy {
			Strategy::CrateMetaData
		}
		fn is_third_party(&self) -> bool {
			self.third_party
		}
		fn target(&self) -> &str {
			&self.target.target
		}
		fn target_data(&self) -> &Arc<TargetDataErased> {
			&self.target
		}
	}

	/// Install `fetcher`'s release of `tool`, returning the staging directory and the result.
	fn install(fetcher: Fake) -> (tempfile::TempDir, Result<Option<Record>>) {
		let stage = tempfile::tempdir().unwrap();
		fs::create_dir(stage.path().join("bin")).unwrap();
		let ratchet = crate::ratchet::Ratchet {
			dir: stage.path().join("attested"),
			name: "tool".into(),
			caches: Vec::new(),
		};
		let package = Package {
			ratchet: &ratchet,
			name: "tool".into(),
			version: Version::new(1, 0, 0),
			repository: None,
			meta: PkgMeta::default(),
			bins: vec![("tool".into(), true)],
		};
		let result = network::runtime().unwrap().block_on(package.install(
			stage.path(),
			"tool",
			&fetcher,
			&origin::Redirects::default(),
		));
		(stage, result)
	}

	fn fake(third_party: bool, signed: bool, receipt: Option<(&str, Option<&str>)>) -> Fake {
		let name = format!("tool{}", std::env::consts::EXE_SUFFIX);
		let tar = crate::archive::tests::tarball(&[(&name, b"binary")]);
		let meta = PkgMeta {
			bin_dir: Some("{ bin }{ binary-ext }".into()),
			..PkgMeta::default()
		};
		Fake {
			archive: crate::archive::tests::gzip(&tar),
			signed,
			third_party,
			receipt: receipt.map(|(url, repository)| fetchers::Receipt {
				declared: repository.map(str::to_owned),
				repository: repository.map(str::to_owned),
				explicit: true,
				url: url.parse().unwrap(),
				sha256: [0; 32],
			}),
			target: Arc::new(TargetData {
				target: binstalk::TARGET.to_owned(),
				meta,
				target_related_info: TargetTriple::from_str(binstalk::TARGET).unwrap(),
			}),
		}
	}

	#[test]
	fn archives_unpack_only_after_their_proofs_pass() {
		let unpacked = |stage: &Path| stage.join("download").exists() || stage.join("bin/tool").exists();
		// quickinstall's minisign check is its only proof; GitHub attestations are never queried.
		let (stage, record) = install(fake(true, true, None));
		assert_eq!(record.unwrap().unwrap().source, "quickinstall");
		assert!(
			stage
				.path()
				.join("bin")
				.join(format!("tool{}", std::env::consts::EXE_SUFFIX))
				.is_file()
		);
		let (stage, result) = install(fake(true, false, None));
		assert!(matches!(
			result.unwrap_err().downcast_ref(),
			Some(fetchers::FetchError::InvalidSignature)
		));
		assert!(!unpacked(stage.path()));
		// An author release outside GitHub has no attestation to check, and says so.
		let (_, record) = install(fake(false, true, Some(("https://example.com/tool.tgz", None))));
		assert_eq!(
			record.unwrap().unwrap().source,
			"author release (repository does not support GitHub attestations)"
		);
		let (stage, result) = install(fake(false, false, Some(("https://example.com/tool.tgz", None))));
		assert!(result.is_err());
		assert!(!unpacked(stage.path()));
		// An archive whose attestation cannot be bound to one repository is never unpacked.
		let conflict = (
			"https://github.com/c/d/releases/download/v1.0.0/tool.tgz",
			Some("https://github.com/a/b"),
		);
		let (stage, result) = install(fake(false, true, Some(conflict)));
		let error = format!("{:#}", result.unwrap_err());
		assert!(
			error.contains("the archive comes from GitHub repository c/d"),
			"{error}"
		);
		assert!(!unpacked(stage.path()));
		let (stage, result) = install(fake(false, true, None));
		assert!(result.is_err());
		assert!(!unpacked(stage.path()));
	}
}
