#![cfg(windows)]

use std::{
	fs,
	io::{BufRead, BufReader},
	os::windows::process::CommandExt,
	process::{Command, Stdio},
};

// The fixture crate: `sleep MARKER` waits for Ctrl-C, keeps working a while, then writes MARKER
// and exits 42; `ctrl-c PID` presses Ctrl-C in PID's console.
const FIXTURE: &str = r#"
type Handler = unsafe extern "system" fn(u32) -> i32;
#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetConsoleCtrlHandler(handler: Option<Handler>, add: i32) -> i32;
    fn FreeConsole() -> i32;
    fn AttachConsole(pid: u32) -> i32;
    fn GenerateConsoleCtrlEvent(event: u32, group: u32) -> i32;
}
static PRESSED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
unsafe extern "system" fn pressed(_: u32) -> i32 {
    PRESSED.store(true, std::sync::atomic::Ordering::SeqCst);
    1
}
fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args[0].as_str() {
        "sleep" => unsafe {
            assert_ne!(SetConsoleCtrlHandler(Some(pressed), 1), 0);
            println!("ready");
            while !PRESSED.load(std::sync::atomic::Ordering::SeqCst) {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            // Shutdown work crabx must outlast; the test needs no timing, only the order.
            std::thread::sleep(std::time::Duration::from_secs(2));
            std::fs::write(&args[1], "done").unwrap();
            std::process::exit(42);
        },
        "ctrl-c" => unsafe {
            FreeConsole();
            assert_ne!(AttachConsole(args[1].parse().unwrap()), 0);
            // Ctrl-C reaches every process on the console, this one included.
            assert_ne!(SetConsoleCtrlHandler(None, 1), 0);
            assert_ne!(GenerateConsoleCtrlEvent(0, 0), 0);
        },
        mode => panic!("unknown mode {mode}"),
    }
}
"#;

/// Removes the fixture's entries from the user's real cache, which Windows locates without the environment.
struct Cleanup(Vec<std::path::PathBuf>);

impl Drop for Cleanup {
	fn drop(&mut self) {
		for path in &self.0 {
			let _ = fs::remove_dir_all(path);
			let _ = fs::remove_file(path);
		}
	}
}

#[test]
fn ctrl_c_waits_for_the_crate_and_exits_with_its_code() {
	let temp = tempfile::tempdir().unwrap();
	let source = temp.path().join("fixture.rs");
	let helper = temp.path().join("fixture.exe");
	fs::write(&source, FIXTURE).unwrap();
	assert!(
		Command::new("rustc")
			.args(["--edition", "2024"])
			.arg(&source)
			.arg("-o")
			.arg(&helper)
			.status()
			.unwrap()
			.success()
	);

	let name = format!("crabx-ctrl-c-fixture-{}", std::process::id());
	let base = directories::BaseDirs::new().unwrap();
	let host = base.cache_dir().join("crabx/v2").join(binstalk::TARGET);
	let _cleanup = Cleanup(vec![
		host.join("author").join(&name),
		host.join("attested").join(format!("{name}.lock")),
	]);
	let entry = host.join("author").join(&name).join("1.2.3");
	fs::create_dir_all(entry.join("bin")).unwrap();
	fs::copy(&helper, entry.join("bin/tool.exe")).unwrap();
	fs::write(
		entry.join("record.json"),
		r#"{"version":"1.2.3","bins":["tool.exe"],"source":"fixture"}"#,
	)
	.unwrap();

	let marker = temp.path().join("marker");
	// CREATE_NEW_CONSOLE: a console of its own, so the Ctrl-C reaches crabx and the crate, not this test.
	let mut crabx = Command::new(env!("CARGO_BIN_EXE_crabx"))
		.arg(format!("{name}@1.2.3"))
		.arg("sleep")
		.arg(&marker)
		.env_remove("CRABX_MINIMUM_RELEASE_AGE")
		.creation_flags(0x00000010)
		.stdout(Stdio::piped())
		.spawn()
		.unwrap();
	let mut ready = String::new();
	BufReader::new(crabx.stdout.take().unwrap())
		.read_line(&mut ready)
		.unwrap();
	assert_eq!(ready.trim_end(), "ready");

	assert!(
		Command::new(&helper)
			.arg("ctrl-c")
			.arg(crabx.id().to_string())
			.status()
			.unwrap()
			.success()
	);
	let status = crabx.wait().unwrap();
	assert!(marker.exists(), "crabx exited with {status} before the crate finished");
	assert_eq!(status.code(), Some(42));
}
