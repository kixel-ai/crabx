#![cfg(unix)]

use std::{
	fs,
	io::{BufRead, BufReader},
	os::unix::{ffi::OsStringExt, process::ExitStatusExt},
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
		.env_remove("CRABX_MINIMUM_RELEASE_AGE");
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
                // crabx always chooses the version, even for an unpinned run.
                let version = args.iter().position(|arg| arg == "--version").unwrap();
                assert_eq!(args[version + 1], "=1.2.3");
                let root = args.iter().position(|arg| arg == "--root").unwrap();
                let root = std::path::Path::new(&args[root + 1]);
                std::fs::copy(std::env::current_exe().unwrap(), root.join("bin/tool")).unwrap();
                std::fs::write(root.join(".crates2.json"),
                    r#"{"installs":{"fixture 1.2.3 (registry+https://github.com/rust-lang/crates.io-index)":{}}}"#).unwrap();
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
		command
			.arg("fixture@1.2.3")
			.env("PATH", "")
			.env("HTTPS_PROXY", "http://127.0.0.1:1");
		if mode == "quickinstall" {
			command.arg("--allow-quickinstall");
		}
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
		.join("crabx/v1")
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
		.join("crabx/v1")
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
