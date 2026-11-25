#![cfg(unix)]

use std::{
	fs,
	io::{BufRead, BufReader},
	net::TcpListener,
	path::{Path, PathBuf},
	process::{Command, Output},
	time::Duration,
};

fn cache(temp: &Path) -> PathBuf {
	if cfg!(target_os = "macos") {
		temp.join("Library/Caches")
	} else {
		temp.join("cache")
	}
}

fn records(temp: &Path) -> PathBuf {
	if cfg!(target_os = "macos") {
		temp.join("Library/Application Support/crabx/launchers")
	} else {
		temp.join("state/crabx/launchers")
	}
}

/// Run `program`, crabx or a launcher, offline, with every directory under `temp`.
fn run(temp: &Path, program: &Path, args: &[&str]) -> Output {
	Command::new(program)
		.args(args)
		.env("HOME", temp)
		.env("XDG_CACHE_HOME", cache(temp))
		.env("XDG_STATE_HOME", temp.join("state"))
		.env("CRABX_INSTALL_DIR", temp.join("bin"))
		.env("PATH", "")
		.env("HTTPS_PROXY", "http://127.0.0.1:1")
		.env_remove("CRABX_REFRESH")
		.output()
		.unwrap()
}

fn stderr(output: &Output) -> String {
	String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Cache `fixture@1.2.3`, a program that prints its arguments and exits 37.
fn setup(temp: &Path) -> PathBuf {
	let source = temp.join("fixture.rs");
	fs::write(
		&source,
		r#"fn main() { println!("{:?}", std::env::args().skip(1).collect::<Vec<_>>()); std::process::exit(37); }"#,
	)
	.unwrap();
	let root = cache(temp)
		.join("crabx/v1")
		.join(binstalk::TARGET)
		.join("author/fixture");
	fs::create_dir_all(root.join("1.2.3/bin")).unwrap();
	let status = Command::new("rustc")
		.arg(&source)
		.arg("-o")
		.arg(root.join("1.2.3/bin/fixture"))
		.status()
		.unwrap();
	assert!(status.success());
	fs::write(
		root.join("1.2.3/record.json"),
		r#"{"version":"1.2.3","bins":["fixture"],"source":"fixture"}"#,
	)
	.unwrap();
	fs::write(root.join("latest"), "1.2.3").unwrap();
	root
}

#[test]
fn launchers_pass_every_argument_and_refuse_other_files() {
	let temp = tempfile::tempdir().unwrap();
	let temp = temp.path();
	setup(temp);
	let crabx = Path::new(env!("CARGO_BIN_EXE_crabx"));
	let bin = temp.join("bin");
	let output = run(temp, crabx, &["install", "fixture@1.2.3"]);
	assert!(output.status.success(), "{}", stderr(&output));
	let installed = bin.join("crabx");
	assert!(stderr(&output).contains(&format!("installed {}\n", installed.display())));
	assert!(stderr(&output).contains(&format!("add {} to PATH", bin.display())));
	assert_eq!(fs::read(&installed).unwrap(), fs::read(crabx).unwrap());
	assert_eq!(fs::read_link(bin.join("fixture")).unwrap(), Path::new("crabx"));

	// `--refresh` and crabx's options reach the crate, pinned without a network.
	let output = run(temp, &bin.join("fixture"), &["--refresh", "--bin", "x"]);
	assert_eq!(output.status.code(), Some(37), "{}", stderr(&output));
	assert_eq!(String::from_utf8_lossy(&output.stdout), "[\"--refresh\", \"--bin\", \"x\"]\n");

	// The installed crabx doesn't copy itself over itself.
	let output = run(temp, &installed, &["install", "fixture@1.2.3"]);
	assert!(output.status.success(), "{}", stderr(&output));
	assert!(!stderr(&output).contains(&format!("installed {}\n", installed.display())));

	for name in ["crabx", "CrabX@1.0.0"] {
		let output = run(temp, crabx, &["install", name]);
		assert!(stderr(&output).contains("`crabx crabx@VERSION`"), "{}", stderr(&output));
	}
	let output = run(temp, crabx, &["--bin", "x", "install", "fixture"]);
	assert!(stderr(&output).contains("options after install"), "{}", stderr(&output));
	let output = run(temp, crabx, &["install", "fixture", "argument"]);
	assert!(stderr(&output).contains("one crate"), "{}", stderr(&output));

	// A name with no record is an error, not some crate's run.
	std::os::unix::fs::symlink("crabx", bin.join("other")).unwrap();
	let output = run(temp, &bin.join("other"), &["--version"]);
	assert_eq!(output.status.code(), Some(1));
	assert!(stderr(&output).contains("\"other\", which is no launcher"), "{}", stderr(&output));

	// One launcher per name, and never over another program's file.
	let elsewhere = temp.join("elsewhere");
	fs::create_dir(&elsewhere).unwrap();
	fs::write(elsewhere.join("fixture"), "another installer's program").unwrap();
	let install_elsewhere = || {
		Command::new(crabx)
			.args(["install", "fixture@1.2.3"])
			.env("HOME", temp)
			.env("XDG_CACHE_HOME", cache(temp))
			.env("XDG_STATE_HOME", temp.join("state"))
			.env("CRABX_INSTALL_DIR", &elsewhere)
			.output()
			.unwrap()
	};
	let output = install_elsewhere();
	assert!(
		stderr(&output).contains(&format!("already has a launcher at {}", bin.join("fixture").display())),
		"{}",
		stderr(&output)
	);
	// Deleting a launcher uninstalls it.
	fs::remove_file(bin.join("fixture")).unwrap();
	let output = install_elsewhere();
	assert!(
		stderr(&output).contains(&format!(
			"{} exists and is not crabx's launcher",
			elsewhere.join("fixture").display()
		)),
		"{}",
		stderr(&output)
	);
	assert!(records(temp).join("fixture.json").is_file());
	let output = run(temp, &installed, &["install"]);
	assert!(output.status.success(), "{}", stderr(&output));
	assert!(!records(temp).join("fixture.json").exists());
}

#[test]
fn a_launchers_refresh_runs_no_crate() {
	let temp = tempfile::tempdir().unwrap();
	let temp = temp.path();
	let root = setup(temp);
	let output = run(temp, Path::new(env!("CARGO_BIN_EXE_crabx")), &["install", "fixture"]);
	assert!(output.status.success(), "{}", stderr(&output));
	let launcher = temp.join("bin/fixture");

	// The marker wins over the launcher's name: this refreshes, failing offline, and runs nothing.
	let output = Command::new(&launcher)
		.arg("fixture")
		.env("HOME", temp)
		.env("XDG_CACHE_HOME", cache(temp))
		.env("XDG_STATE_HOME", temp.join("state"))
		.env("HTTPS_PROXY", "http://127.0.0.1:1")
		.env("CRABX_REFRESH", "1")
		.output()
		.unwrap();
	assert_eq!(output.status.code(), Some(1), "{}", stderr(&output));
	assert!(output.stdout.is_empty());
	assert!(root.join("refresh.json").is_file());
	fs::remove_file(root.join("refresh.json")).unwrap();

	// The refresh an unpinned run starts asks the index for a newer version rather than
	// running the crate again.
	let proxy = TcpListener::bind("127.0.0.1:0").unwrap();
	let address = proxy.local_addr().unwrap();
	let (sender, receiver) = std::sync::mpsc::channel();
	std::thread::spawn(move || {
		let (stream, _) = proxy.accept().unwrap();
		let mut line = String::new();
		BufReader::new(stream).read_line(&mut line).unwrap();
		sender.send(line).unwrap();
	});
	let output = Command::new(&launcher)
		.arg("argument")
		.env("HOME", temp)
		.env("XDG_CACHE_HOME", cache(temp))
		.env("XDG_STATE_HOME", temp.join("state"))
		.env("HTTPS_PROXY", format!("http://{address}"))
		.env_remove("CRABX_REFRESH")
		.output()
		.unwrap();
	assert_eq!(output.status.code(), Some(37), "{}", stderr(&output));
	assert_eq!(String::from_utf8_lossy(&output.stdout), "[\"argument\"]\n");
	// Only a hang reaches this deadline; the refresh connects as soon as it starts.
	let request = receiver.recv_timeout(Duration::from_secs(120)).unwrap();
	assert!(request.starts_with("CONNECT index.crates.io:443 "), "{request}");
}
