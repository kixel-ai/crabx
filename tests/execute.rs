#![cfg(unix)]

use std::{
	fs,
	io::{BufRead, BufReader, Read},
	os::{
		fd::AsRawFd,
		unix::{ffi::OsStringExt, process::CommandExt, process::ExitStatusExt},
	},
	path::Path,
	process::{Command, Stdio},
};

fn cache(temp: &Path) -> std::path::PathBuf {
	if cfg!(target_os = "macos") {
		temp.join("Library/Caches")
	} else {
		temp.join("cache")
	}
}

fn crabx(temp: &Path) -> Command {
	let mut command = Command::new(env!("CARGO_BIN_EXE_crabx"));
	// A developer's own delay would change which seeded release qualifies.
	command
		.env("HOME", temp)
		.env("XDG_CACHE_HOME", cache(temp))
		.env_remove("CRABX_MINIMUM_RELEASE_AGE")
		// Cargo sets CARGO for tests; the developer's CARGO_HOME would escape HOME.
		.env_remove("CARGO")
		.env_remove("CARGO_HOME")
		.env_remove("CARGO_TARGET_DIR");
	command
}

fn setup(temp: &Path, name: &str) -> std::path::PathBuf {
	let helper = temp.join("helper");
	let source = temp.join("helper.rs");
	fs::write(
		&source,
		r##"
        use std::{io::{self, Write}, os::unix::ffi::OsStrExt};
        fn main() {
            let args: Vec<_> = std::env::args_os().skip(1).collect();
            if args.first().is_some_and(|arg| arg == "install") {
                let expected = std::env::var_os("CRABX_TEST_CRATE").unwrap_or_else(|| "fixture".into());
                assert_eq!(args[1], expected);
                assert!(args.iter().any(|arg| arg == "--locked"));
                let target = args.iter().position(|arg| arg == "--target").unwrap();
                assert_eq!(args[target + 1], std::env::var_os("CRABX_TEST_TARGET").unwrap());
                // crabx always chooses the exact version, even for an unpinned run.
                let version = args.iter().position(|arg| arg == "--version").unwrap();
                let version = args[version + 1].to_str().unwrap().strip_prefix('=').unwrap();
                let root = args.iter().position(|arg| arg == "--root").unwrap();
                let root = std::path::Path::new(&args[root + 1]);
                std::fs::copy(std::env::current_exe().unwrap(), root.join("bin/tool")).unwrap();
                std::fs::write(root.join(".crates2.json"), format!(
                    r#"{{"installs":{{"fixture {version} (registry+https://github.com/rust-lang/crates.io-index)":{{}}}}}}"#)).unwrap();
                // Where Cargo ran, for the isolation tests: its directory, its parent's, and its home.
                if let Some(log) = std::env::var_os("CRABX_TEST_LOG") {
                    assert!(std::process::Command::new("rustc").status().unwrap().success());
                    let parent = std::fs::read_link(format!("/proc/{}/cwd", std::os::unix::process::parent_id()))
                        .unwrap_or_default();
                    let line = format!("{version} {} {} {}\n", std::env::current_dir().unwrap().display(),
                        parent.display(), std::env::var("CARGO_HOME").unwrap());
                    std::fs::OpenOptions::new().create(true).append(true).open(log).unwrap()
                        .write_all(line.as_bytes()).unwrap();
                }
                return;
            }
            if args.first().is_some_and(|arg| arg == "signal") {
                println!("ready");
                io::stdout().flush().unwrap();
                let mut line = String::new();
                io::stdin().read_line(&mut line).unwrap();
                return;
            }
            println!("pid={}", std::process::id());
            for arg in args { println!("{:?}", arg.as_bytes()); }
            eprintln!("stderr survives");
            std::process::exit(37);
        }
    "##,
	)
	.unwrap();
	assert!(
		Command::new("rustc")
			.arg(&source)
			.arg("-o")
			.arg(&helper)
			.status()
			.unwrap()
			.success()
	);
	let root = cache(temp)
		.join("crabx/v2")
		.join(binstalk::TARGET)
		.join("author")
		.join(name);
	let entry = root.join("1.2.3");
	fs::create_dir_all(entry.join("bin")).unwrap();
	fs::copy(&helper, entry.join("bin/tool")).unwrap();
	fs::write(
		entry.join("record.json"),
		r#"{"version":"1.2.3","bins":["tool"],"source":"fixture"}"#,
	)
	.unwrap();
	root
}

#[test]
fn legacy_cache_entries_never_run() {
	let temp = tempfile::tempdir().unwrap();
	let mut entry = setup(temp.path(), "fixture");
	// 0.1 wrote its entries, before any 0.2 attestation policy, under `v1`.
	for mode in ["author", "quickinstall"] {
		let legacy = cache(temp.path())
			.join("crabx/v1")
			.join(binstalk::TARGET)
			.join(mode)
			.join("fixture");
		fs::create_dir_all(legacy.parent().unwrap()).unwrap();
		fs::rename(&entry, &legacy).unwrap();
		entry = legacy.clone();
		let mut command = crabx(temp.path());
		if mode == "quickinstall" {
			command.arg("--allow-quickinstall");
		}
		command
			.arg("fixture@1.2.3")
			.env("PATH", "")
			.env("HTTPS_PROXY", "http://127.0.0.1:1");
		let output = command.output().unwrap();
		// It is not the entry's exit status, and the entry is left as it was.
		assert_ne!(output.status.code(), Some(37), "{mode}");
		assert!(output.stdout.is_empty(), "{mode}");
		assert!(legacy.join("1.2.3/record.json").is_file() && legacy.join("1.2.3/bin/tool").is_file());
	}
}

#[test]
fn build_is_explicit_host_targeted_and_separate_from_author_cache() {
	let temp = tempfile::tempdir().unwrap();
	setup(temp.path(), "fixture");
	// A build from before Cargo left the caller's directory, which must never run again.
	let legacy = cache(temp.path())
		.join("crabx/v1")
		.join(binstalk::TARGET)
		.join("build/fixture/1.2.3");
	fs::create_dir_all(legacy.join("bin")).unwrap();
	fs::write(legacy.join("bin/tool"), "#!/bin/sh\nexit 38\n").unwrap();
	fs::set_permissions(
		legacy.join("bin/tool"),
		std::os::unix::fs::PermissionsExt::from_mode(0o755),
	)
	.unwrap();
	fs::write(
		legacy.join("record.json"),
		r#"{"version":"1.2.3","bins":["tool"],"source":"fixture"}"#,
	)
	.unwrap();
	let output = crabx(temp.path())
		.args(["--build", "FiXtUrE@1.2.3", "built argument"])
		.env("CARGO", temp.path().join("helper"))
		.env("CRABX_TEST_CRATE", "FiXtUrE")
		.env("CRABX_TEST_TARGET", binstalk::TARGET)
		.env("PATH", "")
		.output()
		.unwrap();
	assert_eq!(
		output.status.code(),
		Some(37),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	let built = cache(temp.path())
		.join("crabx/v2")
		.join(binstalk::TARGET)
		.join("build/fixture/1.2.3");
	assert!(built.join(".crates2.json").is_file());
	assert!(built.join("record.json").is_file());
	let again = crabx(temp.path())
		.args(["--build", "fixture@1.2.3"])
		.env("CARGO", "missing-cargo")
		.env("PATH", "")
		.output()
		.unwrap();
	assert_eq!(again.status.code(), Some(37));
}

#[test]
fn pinned_cache_matches_direct_execution_without_cargo_or_network() {
	let temp = tempfile::tempdir().unwrap();
	let root = setup(temp.path(), "fixture");
	// A pin never consults unrelated mutable update metadata.
	fs::write(root.join("latest"), "corrupt mutable metadata").unwrap();
	let args = [
		"hello world".into(),
		"--build".into(),
		"--".into(),
		std::ffi::OsString::from_vec(vec![0xff, b'x']),
	];
	let direct = Command::new(temp.path().join("helper")).args(&args).output().unwrap();
	let child = crabx(temp.path())
		.arg("FiXtUrE@1.2.3")
		.args(&args)
		.env("PATH", "")
		.env("HTTPS_PROXY", "http://127.0.0.1:1")
		.stdout(Stdio::piped())
		.stderr(Stdio::piped())
		.spawn()
		.unwrap();
	let pid = child.id();
	let output = child.wait_with_output().unwrap();
	assert_eq!(output.status.code(), Some(37));
	assert_eq!(output.stderr, direct.stderr);
	let stdout = String::from_utf8(output.stdout).unwrap();
	assert_eq!(stdout.lines().next().unwrap(), format!("pid={pid}"));
	assert_eq!(
		stdout.lines().skip(1).collect::<Vec<_>>(),
		String::from_utf8(direct.stdout)
			.unwrap()
			.lines()
			.skip(1)
			.collect::<Vec<_>>()
	);
}

#[test]
fn exec_preserves_signals_and_incomplete_cache_is_an_error() {
	let temp = tempfile::tempdir().unwrap();
	let root = setup(temp.path(), "fixture");
	let mut child = crabx(temp.path())
		.args(["fixture@1.2.3", "signal"])
		.stdin(Stdio::piped())
		.stdout(Stdio::piped())
		.spawn()
		.unwrap();
	let mut ready = String::new();
	BufReader::new(child.stdout.take().unwrap())
		.read_line(&mut ready)
		.unwrap();
	assert_eq!(ready, "ready\n");
	// Readiness is synchronized over a pipe, never a timing assumption.
	assert_eq!(unsafe { libc::kill(child.id() as i32, libc::SIGTERM) }, 0);
	assert_eq!(child.wait().unwrap().signal(), Some(libc::SIGTERM));
	fs::remove_file(root.join("1.2.3/bin/tool")).unwrap();
	let output = crabx(temp.path())
		.arg("fixture@1.2.3")
		.env("PATH", "")
		.output()
		.unwrap();
	assert!(!output.status.success());
	assert!(
		String::from_utf8(output.stderr)
			.unwrap()
			.contains("missing or invalid cached binary")
	);
}

/// Cache an index, checked now, of releases published the given hours ago (none for no `pubtime`).
// These tests run real processes, so they read the real clock, with margins of an hour or more.
fn index(root: &Path, releases: &[(&str, Option<u64>)]) {
	let now = std::time::SystemTime::now();
	let releases: Vec<_> = releases
		.iter()
		.map(|(version, hours)| {
			serde_json::json!({
				"version": version,
				"published": hours.map(|hours| now - std::time::Duration::from_secs(hours * 3600)),
			})
		})
		.collect();
	fs::create_dir_all(root).unwrap();
	fs::write(
		root.join("index.json"),
		serde_json::to_vec(&serde_json::json!({ "etag": null, "checked": now, "releases": releases })).unwrap(),
	)
	.unwrap();
}

#[test]
fn cold_unpinned_build_installs_the_chosen_release_before_binary_selection() {
	let temp = tempfile::tempdir().unwrap();
	setup(temp.path(), "fixture");
	let root = cache(temp.path())
		.join("crabx/v2")
		.join(binstalk::TARGET)
		.join("build/fixture");
	// Cargo would pick 1.3.0, which is under a day old; the helper requires `--version =1.2.3`.
	index(&root, &[("1.3.0", Some(1)), ("1.2.3", Some(48))]);
	// An absent --bin reports after installation; with no check due, no updater starts.
	let output = crabx(temp.path())
		.args(["--build", "--bin", "missing", "fixture"])
		.env("CARGO", temp.path().join("helper"))
		.env("CRABX_TEST_TARGET", binstalk::TARGET)
		.output()
		.unwrap();
	assert_eq!(output.status.code(), Some(1));
	assert!(String::from_utf8_lossy(&output.stderr).contains("no binary \"missing\""));
	assert!(root.join("1.2.3/record.json").is_file());
	let output = crabx(temp.path())
		.args(["--build", "fixture@1.2.3"])
		.env("CARGO", "must-not-run")
		.output()
		.unwrap();
	assert_eq!(output.status.code(), Some(37));
}

#[test]
fn cached_runs_report_diagnostics_once_and_pins_ignore_corrupt_refresh_state() {
	let temp = tempfile::tempdir().unwrap();
	let root = setup(temp.path(), "fixture");
	index(&root, &[("1.2.3", Some(48))]);
	// 0.1's state, which this version neither reads nor rewrites.
	fs::write(root.join("latest"), "1.2.3").unwrap();
	let legacy = r#"{"etag":"0.1-validator","version":"1.2.3","failure":null}"#;
	fs::write(root.join("refresh.json"), legacy).unwrap();
	// A retry far in the future, so no updater is due whatever the clock reads.
	let retry_at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1 << 40);
	fs::write(
		root.join("failure.json"),
		serde_json::to_vec(&serde_json::json!({
			"message": "fixture update failure",
			"version": null,
			"reported": false,
			"delay": std::time::Duration::from_secs(1800),
			"retry_at": retry_at,
		}))
		.unwrap(),
	)
	.unwrap();
	let output = crabx(temp.path()).arg("fixture").env("PATH", "").output().unwrap();
	assert_eq!(output.status.code(), Some(37));
	assert!(String::from_utf8_lossy(&output.stderr).contains("fixture update failure"));
	let state: serde_json::Value = serde_json::from_slice(&fs::read(root.join("failure.json")).unwrap()).unwrap();
	assert_eq!(state["reported"], true);
	let output = crabx(temp.path()).arg("fixture").output().unwrap();
	assert_eq!(output.status.code(), Some(37));
	assert!(!String::from_utf8_lossy(&output.stderr).contains("fixture update failure"));
	assert_eq!(fs::read_to_string(root.join("refresh.json")).unwrap(), legacy);

	for file in ["failure.json", "index.json"] {
		let corrupt = tempfile::tempdir().unwrap();
		let root = setup(corrupt.path(), "fixture");
		index(&root, &[("1.2.3", Some(48))]);
		fs::write(root.join(file), "malformed").unwrap();
		let output = crabx(corrupt.path()).arg("fixture").output().unwrap();
		assert_eq!(output.status.code(), Some(1));
		let diagnostic = String::from_utf8_lossy(&output.stderr);
		assert!(diagnostic.contains("corrupt"), "{diagnostic}");
		assert!(diagnostic.contains(&root.join(file).display().to_string()));
		// A pin reads neither.
		let output = crabx(corrupt.path()).arg("fixture@1.2.3").output().unwrap();
		assert_eq!(output.status.code(), Some(37));
	}
}

#[test]
fn unpinned_runs_apply_their_own_delay_to_the_cached_index() {
	let temp = tempfile::tempdir().unwrap();
	let root = setup(temp.path(), "fixture");
	// 1.3.0 is installed, as a `0s` run would leave it, and exits 38 instead of the fixture's 37.
	let newer = root.join("1.3.0");
	fs::create_dir_all(newer.join("bin")).unwrap();
	fs::write(newer.join("bin/tool"), "#!/bin/sh\nexit 38\n").unwrap();
	fs::set_permissions(
		newer.join("bin/tool"),
		std::os::unix::fs::PermissionsExt::from_mode(0o755),
	)
	.unwrap();
	fs::write(
		newer.join("record.json"),
		r#"{"version":"1.3.0","bins":["tool"],"source":"fixture"}"#,
	)
	.unwrap();
	fs::write(root.join("latest"), "1.3.0").unwrap();
	index(&root, &[("1.3.0", Some(1)), ("1.2.3", Some(48))]);
	// Each run needs nothing but the cache: no Cargo, no network.
	let run = |args: &[&str], age: Option<&str>| {
		let mut command = crabx(temp.path());
		command
			.args(args)
			.env("PATH", "")
			.env("HTTPS_PROXY", "http://127.0.0.1:1");
		if let Some(age) = age {
			command.env("CRABX_MINIMUM_RELEASE_AGE", age);
		}
		command.output().unwrap()
	};
	assert_eq!(run(&["fixture"], None).status.code(), Some(37));
	assert_eq!(run(&["fixture"], Some("0s")).status.code(), Some(38));
	assert_eq!(run(&["fixture"], Some("2h")).status.code(), Some(37));
	// The flag wins over the environment.
	assert_eq!(
		run(&["--minimum-release-age", "30m", "fixture"], Some("7d"))
			.status
			.code(),
		Some(38)
	);
	// A malformed delay fails even a pinned run, which otherwise ignores it.
	let output = run(&["fixture@1.3.0"], Some("1day"));
	assert_eq!(output.status.code(), Some(1));
	assert!(String::from_utf8_lossy(&output.stderr).contains("invalid CRABX_MINIMUM_RELEASE_AGE"));
	assert_eq!(run(&["fixture@1.3.0"], None).status.code(), Some(38));
	// A yanked release never runs unpinned, whatever the delay.
	index(&root, &[("1.2.3", Some(48))]);
	assert_eq!(run(&["fixture"], Some("0s")).status.code(), Some(37));
	// Nothing old enough: the error names the newest release and the ways around the delay.
	index(&root, &[("1.3.0", Some(1))]);
	let output = run(&["fixture"], None);
	assert_eq!(output.status.code(), Some(1));
	let error = String::from_utf8_lossy(&output.stderr);
	assert!(
		error.contains("no release of fixture is 1d old yet: fixture@1.3.0 was published at"),
		"{error}"
	);
	assert!(
		error.contains("run fixture@1.3.0 to pin it, or pass --minimum-release-age 0s"),
		"{error}"
	);
	// No publish time is an error, not old enough.
	index(&root, &[("1.3.0", None), ("1.2.3", Some(48))]);
	let output = run(&["fixture"], None);
	assert_eq!(output.status.code(), Some(1));
	assert!(String::from_utf8_lossy(&output.stderr).contains("no publish time for version 1.3.0"));
	assert_eq!(run(&["fixture"], Some("0s")).status.code(), Some(38));
}

/// An untrusted checkout holding the caller's directory, cache, and temporary directory, and a
/// toolchain and CARGO_HOME beside it that the caller names relative to its directory.
struct Checkout {
	temp: tempfile::TempDir,
	/// The build cache for the fixture crate.
	root: std::path::PathBuf,
	log: std::path::PathBuf,
}

impl Checkout {
	fn new() -> Self {
		let temp = tempfile::tempdir().unwrap();
		setup(temp.path(), "fixture");
		let checkout = temp.path().join("checkout");
		for dir in [".git", ".cargo", "tools", "cache", "tmp"] {
			fs::create_dir_all(checkout.join(dir)).unwrap();
		}
		fs::write(checkout.join(".cargo/config.toml"), "untrusted = [").unwrap();
		let bin = temp.path().join("toolchain/bin");
		fs::create_dir_all(&bin).unwrap();
		fs::copy(temp.path().join("helper"), bin.join("cargo")).unwrap();
		fs::write(bin.join("rustc"), "#!/bin/sh\n").unwrap();
		fs::set_permissions(bin.join("rustc"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
		fs::create_dir(temp.path().join("trusted-home")).unwrap();
		// macOS has no XDG_CACHE_HOME, so its cache stays in HOME.
		let caches = if cfg!(target_os = "macos") {
			cache(temp.path())
		} else {
			checkout.join("cache")
		};
		let root = caches.join("crabx/v2").join(binstalk::TARGET).join("build/fixture");
		let log = temp.path().join("cargo.log");
		Self { temp, root, log }
	}

	/// crabx run from `checkout/tools`, where nothing but PATH finds Cargo.
	fn crabx(&self) -> Command {
		let checkout = self.temp.path().join("checkout");
		let mut command = crabx(self.temp.path());
		command
			.current_dir(checkout.join("tools"))
			.env("XDG_CACHE_HOME", checkout.join("cache"))
			.env("TMPDIR", checkout.join("tmp"))
			.env("CARGO_HOME", "../../trusted-home")
			.env("PATH", "../../toolchain/bin")
			.env("CRABX_TEST_TARGET", binstalk::TARGET)
			.env("CRABX_TEST_LOG", &self.log)
			.env("HTTPS_PROXY", "http://127.0.0.1:1");
		command
	}

	/// Each Cargo run's version, directory, parent's directory (Linux only), and CARGO_HOME.
	fn runs(&self) -> Vec<Vec<String>> {
		fs::read_to_string(&self.log)
			.unwrap_or_default()
			.lines()
			.map(|line| line.split(' ').map(str::to_owned).collect())
			.collect()
	}

	/// Assert Cargo ran from the root with the caller's home, started by a process in `parent`.
	fn assert_isolated(&self, run: &[String], version: &str, parent: &Path) {
		let parent = if cfg!(target_os = "linux") {
			parent
		} else {
			Path::new("")
		};
		let home = fs::canonicalize(self.temp.path()).unwrap().join("trusted-home");
		assert_eq!(
			run,
			[version, "/", &parent.display().to_string(), &home.display().to_string()]
		);
	}
}

/// Run `command` with the write end of a pipe as its fd 3, which a detached refresh and its Cargo
/// inherit, and return its output once every process holding that end exits.
fn output_after_refresh(command: &mut Command) -> std::process::Output {
	let (mut reader, writer) = std::io::pipe().unwrap();
	let fd = writer.as_raw_fd();
	// SAFETY: dup2 and fcntl are async-signal-safe.
	unsafe {
		command.pre_exec(move || {
			// dup2 onto itself would leave close-on-exec set.
			let result = if fd == 3 {
				libc::fcntl(3, libc::F_SETFD, 0)
			} else {
				libc::dup2(fd, 3)
			};
			if result < 0 {
				return Err(std::io::Error::last_os_error());
			}
			Ok(())
		});
	}
	let output = command.output().unwrap();
	drop(writer);
	reader.read_to_end(&mut Vec::new()).unwrap();
	output
}

#[test]
fn builds_run_cargo_outside_the_checkout_with_the_callers_toolchain() {
	let checkout = Checkout::new();
	let output = checkout.crabx().args(["--build", "fixture@1.2.3"]).output().unwrap();
	assert_eq!(
		output.status.code(),
		Some(37),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(checkout.root.join("1.2.3/record.json").is_file());
	let runs = checkout.runs();
	assert_eq!(runs.len(), 1);
	// The foreground crabx itself stays where the caller ran it.
	let tools = fs::canonicalize(checkout.temp.path().join("checkout/tools")).unwrap();
	checkout.assert_isolated(&runs[0], "1.2.3", &tools);
	// 1.3.0 qualifies but is not installed: the run takes 1.2.3 and a detached refresh builds 1.3.0.
	index(&checkout.root, &[("1.3.0", Some(48)), ("1.2.3", Some(72))]);
	let output = output_after_refresh(checkout.crabx().args(["--build", "fixture"]));
	assert_eq!(
		output.status.code(),
		Some(37),
		"{}",
		String::from_utf8_lossy(&output.stderr)
	);
	assert!(!String::from_utf8_lossy(&output.stderr).contains("background update"));
	assert!(checkout.root.join("1.3.0/record.json").is_file());
	let runs = checkout.runs();
	assert_eq!(runs.len(), 2);
	checkout.assert_isolated(&runs[1], "1.3.0", Path::new("/"));
	// A cached build still runs, and its refresh still checks the index, without Cargo.
	index(&checkout.root, &[("1.4.0", Some(48)), ("1.3.0", Some(72))]);
	let output = output_after_refresh(checkout.crabx().args(["--build", "fixture"]).env("PATH", ""));
	assert_eq!(output.status.code(), Some(37));
	assert!(!String::from_utf8_lossy(&output.stderr).contains("background update"));
	let failure: serde_json::Value =
		serde_json::from_slice(&fs::read(checkout.root.join("failure.json")).unwrap()).unwrap();
	assert_eq!(failure["version"], "1.4.0", "{failure}");
	assert!(
		failure["message"]
			.as_str()
			.unwrap()
			.contains("found no \"cargo\" on PATH"),
		"{failure}"
	);
}

#[test]
fn builds_reject_a_cargo_home_the_checkout_controls_or_configures() {
	let checkout = Checkout::new();
	let work = checkout.temp.path();
	fs::create_dir(work.join(".cargo")).unwrap();
	fs::write(work.join(".cargo/config.toml"), "untrusted = [").unwrap();
	let homes = [
		(work.join("checkout/.cargo").into_os_string(), "is inside the checkout"),
		("../.cargo".into(), "is inside the checkout"),
		(work.join("trusted-home").into_os_string(), "cargo install would read"),
	];
	for (home, error) in homes {
		let output = checkout
			.crabx()
			.args(["--build", "fixture@1.2.3"])
			.env("CARGO_HOME", &home)
			.output()
			.unwrap();
		assert_eq!(output.status.code(), Some(1));
		assert!(String::from_utf8_lossy(&output.stderr).contains(error), "{home:?}");
	}
	fs::remove_file(work.join(".cargo/config.toml")).unwrap();
	let output = checkout.crabx().args(["--build", "fixture@1.2.3"]).output().unwrap();
	assert_eq!(output.status.code(), Some(37));
	assert!(checkout.root.join("1.2.3/record.json").is_file());
	// An installed release still runs, and its refresh still checks the index, but refuses to build
	// with this home.
	index(&checkout.root, &[("1.3.0", Some(48)), ("1.2.3", Some(72))]);
	let output = output_after_refresh(
		checkout
			.crabx()
			.args(["--build", "fixture"])
			.env("CARGO_HOME", "../.cargo"),
	);
	assert_eq!(output.status.code(), Some(37));
	assert!(!String::from_utf8_lossy(&output.stderr).contains("background update"));
	let failure = || -> serde_json::Value {
		serde_json::from_slice(&fs::read(checkout.root.join("failure.json")).unwrap()).unwrap()
	};
	let refused = failure();
	assert_eq!(refused["version"], "1.3.0", "{refused}");
	let message = refused["message"].as_str().unwrap();
	assert!(
		message.starts_with("CARGO_HOME") && message.contains("is inside the checkout"),
		"{refused}"
	);
	assert!(!checkout.root.join("1.3.0").exists());
	assert_eq!(checkout.runs().len(), 1);
	// The next run reports the refusal, and its refresh waits out the backoff.
	let output = output_after_refresh(checkout.crabx().args(["--build", "fixture"]));
	assert_eq!(output.status.code(), Some(37));
	let stderr = String::from_utf8_lossy(&output.stderr);
	assert!(
		stderr.contains(&format!("the last background update failed: {message}")),
		"{stderr}"
	);
	// A due index is still fetched under a rejected setting, so a yank of the cached release is seen.
	fs::remove_file(checkout.root.join("failure.json")).unwrap();
	let path = checkout.root.join("index.json");
	let mut stale: serde_json::Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
	stale["checked"] =
		serde_json::to_value(std::time::SystemTime::now() - std::time::Duration::from_secs(48 * 3600)).unwrap();
	fs::write(&path, serde_json::to_vec(&stale).unwrap()).unwrap();
	let output = output_after_refresh(
		checkout
			.crabx()
			.args(["--build", "fixture"])
			.env("CARGO_TARGET_DIR", "target"),
	);
	assert_eq!(output.status.code(), Some(37));
	assert!(!String::from_utf8_lossy(&output.stderr).contains("background update"));
	// The fetch ran first, and failed only for want of the network.
	let fetched = failure();
	assert_eq!(fetched["version"], serde_json::Value::Null, "{fetched}");
	assert!(
		fetched["message"].as_str().unwrap().contains("index.crates.io"),
		"{fetched}"
	);
	assert_eq!(checkout.runs().len(), 1);
}

#[test]
fn prebuilt_refresh_needs_no_cargo() {
	let temp = tempfile::tempdir().unwrap();
	let root = setup(temp.path(), "fixture");
	index(&root, &[("1.3.0", Some(48)), ("1.2.3", Some(72))]);
	let output = output_after_refresh(
		crabx(temp.path())
			.arg("fixture")
			.env("PATH", "")
			.env("HTTPS_PROXY", "http://127.0.0.1:1"),
	);
	assert_eq!(output.status.code(), Some(37));
	assert!(!String::from_utf8_lossy(&output.stderr).contains("background update"));
	// The refresh ran, and failed only for want of the network.
	let failure: serde_json::Value = serde_json::from_slice(&fs::read(root.join("failure.json")).unwrap()).unwrap();
	assert_eq!(failure["version"], "1.3.0", "{failure}");
}
