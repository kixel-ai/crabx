//! What an unpinned run knows of a crate's releases, and the release-age delay that picks among them.
use crate::{
	cache::{self, Cache},
	network,
};
use anyhow::{Context, Result, anyhow};
use reqwest::{Response, StatusCode, header};
use semver::Version;
use serde::{Deserialize, Serialize};
use std::{
	collections::BTreeSet,
	fmt,
	fs::File,
	io::{BufReader, Read},
	path::PathBuf,
	str::FromStr,
	time::{Duration, SystemTime},
};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);

/// The most bytes of one crate's sparse-index file, which runs to about 1.5 MB for crates with
/// thousands of releases.
pub const INDEX_BYTES: u64 = 16 << 20;

/// How long ago a release must have been published for an unpinned run to pick it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MinimumAge(Duration);

impl MinimumAge {
	/// A hijacked release is usually yanked within hours, so a day's delay keeps most from ever running.
	pub const DEFAULT: Self = Self(DAY);
}

const UNITS: [(&str, u64); 5] = [("w", 7 * 86400), ("d", 86400), ("h", 3600), ("m", 60), ("s", 1)];

impl FromStr for MinimumAge {
	type Err = anyhow::Error;

	/// mise's forms for `minimum_release_age`: a whole number and one unit, such as `24h` or `7d`.
	fn from_str(value: &str) -> Result<Self> {
		let (count, unit) = value.split_at(value.find(|c: char| !c.is_ascii_digit()).unwrap_or(value.len()));
		UNITS
			.iter()
			.find(|(name, _)| *name == unit)
			.and_then(|(_, seconds)| count.parse::<u64>().ok()?.checked_mul(*seconds))
			.map(|seconds| Self(Duration::from_secs(seconds)))
			.with_context(|| {
				format!("invalid duration {value:?}; use a whole number of s, m, h, d or w, such as 24h or 7d")
			})
	}
}

impl fmt::Display for MinimumAge {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		let seconds = self.0.as_secs();
		match UNITS
			.iter()
			.find(|(_, size)| seconds != 0 && seconds.is_multiple_of(*size))
		{
			Some((unit, size)) => write!(f, "{}{unit}", seconds / size),
			None => write!(f, "{seconds}s"),
		}
	}
}

/// The stable, non-yanked releases crabx last read from a crate's sparse-index file.
// It lives in its own file because 0.1's refresh state rejects unknown fields, and its
// validator stands only for the releases stored beside it.
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Index {
	etag: Option<String>,
	/// When crates.io last answered, which schedules the next daily check.
	checked: SystemTime,
	/// Newest first.
	releases: Vec<Release>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Release {
	version: Version,
	/// crates.io's `pubtime`.
	published: Option<SystemTime>,
}

/// The releases an unpinned run's delay allows at the moment it runs.
#[derive(Debug, PartialEq)]
pub struct Choice {
	/// The newest eligible release already installed, which this run executes.
	pub installed: Option<Version>,
	/// The newest eligible release, which the next run should find installed.
	pub newest: Option<Version>,
}

impl Index {
	fn path(cache: &Cache) -> PathBuf {
		cache.root.join("index.json")
	}

	/// The cached index, if any run has fetched one.
	pub fn load(cache: &Cache) -> Result<Option<Self>> {
		let path = Self::path(cache);
		let file = match File::open(&path) {
			Ok(file) => file,
			Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
			Err(error) => return Err(error).with_context(|| format!("read {}", path.display())),
		};
		let mut index: Self = serde_json::from_reader(BufReader::new(file))
			.with_context(|| format!("corrupt index cache {}", path.display()))?;
		index.releases.sort_by(|a, b| b.version.cmp(&a.version));
		Ok(Some(index))
	}

	/// Fetch and cache the crate's sparse-index file, conditionally when `previous` holds the releases
	/// its validator stands for.
	pub async fn fetch(cache: &Cache, name: &str, previous: Option<Self>, now: SystemTime) -> Result<Self> {
		let previous = previous.filter(|index| index.etag.is_some());
		let mut request = network::client()?.get(url(name));
		if let Some(etag) = previous.as_ref().and_then(|index| index.etag.as_deref()) {
			request = request.header(header::IF_NONE_MATCH, etag);
		}
		let response = request.send().await?.error_for_status()?;
		Self::receive(cache, previous, response, now).await
	}

	async fn receive(cache: &Cache, previous: Option<Self>, response: Response, now: SystemTime) -> Result<Self> {
		let index = if response.status() == StatusCode::NOT_MODIFIED {
			Self {
				checked: now,
				..previous.context("the index answered 304 to an unconditional request")?
			}
		} else {
			let etag = response
				.headers()
				.get(header::ETAG)
				.map(|value| value.to_str().map(str::to_owned))
				.transpose()?;
			std::fs::create_dir_all(&cache.root)?;
			let file = network::spool(response, &cache.root, INDEX_BYTES).await?;
			Self::parse(BufReader::new(File::open(file.path())?), etag, now)?
		};
		cache::atomic(&Self::path(cache), &serde_json::to_vec(&index)?)?;
		Ok(index)
	}

	fn parse(reader: impl Read, etag: Option<String>, now: SystemTime) -> Result<Self> {
		#[derive(Deserialize)]
		struct Entry {
			vers: Version,
			yanked: bool,
			pubtime: Option<String>,
		}
		let mut releases = Vec::new();
		// Dependency lists stream past unread.
		for entry in serde_json::Deserializer::from_reader(reader).into_iter::<Entry>() {
			let entry = entry?;
			if entry.yanked || !entry.vers.pre.is_empty() {
				continue;
			}
			let published = entry
				.pubtime
				.map(|time| -> Result<SystemTime> { Ok(time.parse::<jiff::Timestamp>()?.try_into()?) })
				.transpose()
				.with_context(|| format!("invalid pubtime for version {} in the index", entry.vers))?;
			releases.push(Release {
				version: entry.vers,
				published,
			});
		}
		releases.sort_by(|a, b| b.version.cmp(&a.version));
		Ok(Self {
			etag,
			checked: now,
			releases,
		})
	}

	/// Whether the daily check for new releases is due; a clock set back makes it due at once.
	pub fn due(&self, now: SystemTime) -> bool {
		now.duration_since(self.checked).map_or(true, |elapsed| elapsed >= DAY)
	}

	/// The releases `age` allows at `now`, newest first. One without a publish time is an error
	/// once reached, since nothing says it is old enough.
	fn eligible(&self, age: MinimumAge, now: SystemTime) -> impl Iterator<Item = Result<&Version>> {
		self.releases.iter().filter_map(move |release| {
			if age.0.is_zero() {
				return Some(Ok(&release.version));
			}
			let Some(published) = release.published else {
				return Some(Err(anyhow!(
					"crates.io records no publish time for version {}",
					release.version
				)));
			};
			published
				.checked_add(age.0)
				.is_some_and(|eligible| eligible <= now)
				.then_some(Ok(&release.version))
		})
	}

	/// The newest release `age` allows at `now`.
	pub fn newest(&self, age: MinimumAge, now: SystemTime) -> Result<Option<&Version>> {
		self.eligible(age, now).next().transpose()
	}

	/// The newest eligible release, and the newest of those in `installed`.
	pub fn choose(&self, installed: &BTreeSet<Version>, age: MinimumAge, now: SystemTime) -> Result<Choice> {
		let mut choice = Choice {
			installed: None,
			newest: None,
		};
		for version in self.eligible(age, now) {
			let version = version?;
			choice.newest.get_or_insert_with(|| version.clone());
			if installed.contains(version) {
				choice.installed = Some(version.clone());
				break;
			}
		}
		Ok(choice)
	}

	/// The error for a crate none of whose releases `age` allows.
	pub fn too_young(&self, name: &str, age: MinimumAge) -> anyhow::Error {
		let Some(newest) = self.releases.first() else {
			return anyhow!("{name} has no non-yanked stable release");
		};
		let version = &newest.version;
		let Some(published) = newest.published else {
			return anyhow!("crates.io records no publish time for {name}@{version}");
		};
		let eligible = published
			.checked_add(age.0)
			.map_or_else(|| "never".to_owned(), timestamp);
		anyhow!(
			"no release of {name} is {age} old yet: {name}@{version} was published at {}, and is eligible at {eligible}; \
			 run {name}@{version} to pin it, or pass --minimum-release-age 0s",
			timestamp(published)
		)
	}
}

fn timestamp(time: SystemTime) -> String {
	jiff::Timestamp::try_from(time).map_or_else(|_| format!("{time:?}"), |time| time.to_string())
}

/// The crate's sparse-index file.
pub fn url(name: &str) -> String {
	format!("https://index.crates.io/{}", index_path(name))
}

fn index_path(name: &str) -> String {
	let name = name.to_ascii_lowercase();
	match name.len() {
		1 => format!("1/{name}"),
		2 => format!("2/{name}"),
		3 => format!("3/{}/{name}", &name[..1]),
		_ => format!("{}/{}/{name}", &name[..2], &name[2..4]),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::io::Write;

	const HOUR: Duration = Duration::from_secs(3600);

	fn at(hours: u64) -> SystemTime {
		SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000) + HOUR * hours as u32
	}

	fn line(version: &str, yanked: bool, published: Option<SystemTime>) -> String {
		let pubtime = published.map_or(String::new(), |time| format!(r#","pubtime":"{}""#, timestamp(time)));
		format!(r#"{{"vers":"{version}","yanked":{yanked},"deps":[{{"ignored":1}}]{pubtime}}}"#)
	}

	fn index(lines: &[String]) -> Index {
		Index::parse(lines.join("\n").as_bytes(), None, at(0)).unwrap()
	}

	fn set(versions: &[&str]) -> BTreeSet<Version> {
		versions.iter().map(|v| Version::parse(v).unwrap()).collect()
	}

	fn version(value: &str) -> Option<Version> {
		Some(Version::parse(value).unwrap())
	}

	#[test]
	fn durations_take_one_whole_unit() {
		for (value, seconds, shown) in [
			("24h", 86400, "1d"),
			("7d", 604800, "1w"),
			("90m", 5400, "90m"),
			("0s", 0, "0s"),
			("2w", 1209600, "2w"),
			("61s", 61, "61s"),
		] {
			let age: MinimumAge = value.parse().unwrap();
			assert_eq!(age, MinimumAge(Duration::from_secs(seconds)), "{value}");
			assert_eq!(age.to_string(), shown);
			assert_eq!(shown.parse::<MinimumAge>().unwrap(), age);
		}
		assert_eq!(MinimumAge::DEFAULT, "24h".parse().unwrap());
		for invalid in [
			"",
			"24",
			"h",
			"1.5h",
			"-1h",
			"+1h",
			"1d12h",
			"1 d",
			"1H",
			"1day",
			"99999999999999999999w",
		] {
			assert!(invalid.parse::<MinimumAge>().is_err(), "{invalid}");
		}
	}

	#[test]
	fn the_index_keeps_stable_unyanked_releases_by_semver() {
		let index = index(&[
			line("2.0.0", false, Some(at(0))),
			line("9.0.0-beta.1", false, Some(at(0))),
			line("10.0.0", true, Some(at(0))),
			line("1.0.0", false, None),
		]);
		let versions: Vec<_> = index.releases.iter().map(|r| r.version.to_string()).collect();
		assert_eq!(versions, ["2.0.0", "1.0.0"]);
		assert_eq!(index.releases[0].published, Some(at(0)));
		assert_eq!(index.releases[1].published, None);
		assert_eq!(
			Index::parse(
				&br#"{"vers":"1.0.0","yanked":false,"pubtime":"2026-11-18T22:08:12Z"}"#[..],
				None,
				at(0)
			)
			.unwrap()
			.releases[0]
				.published,
			Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_795_039_692))
		);
		for malformed in [
			&br#"{"vers":"oops","yanked":false}"#[..],
			br#"{"vers":"1.0.0","yanked":false,"pubtime":"yesterday"}"#,
		] {
			assert!(Index::parse(malformed, None, at(0)).is_err());
		}
		for (name, path) in [
			("A", "1/a"),
			("AB", "2/ab"),
			("AbC", "3/a/abc"),
			("crabx", "cr/ab/crabx"),
		] {
			assert_eq!(index_path(name), path);
		}
	}

	#[test]
	fn runs_pick_the_newest_installed_release_the_delay_allows_now() {
		let day = MinimumAge::DEFAULT;
		let index = index(&[
			line("3.0.0", false, Some(at(100))),
			// A backport published after the newer major.
			line("2.1.0", false, Some(at(90))),
			line("2.0.0", false, Some(at(10))),
			line("1.0.0", false, Some(at(0))),
		]);
		// Eligible exactly a day after publishing, and not a second before.
		let choice = |installed, now| index.choose(&set(installed), day, now).unwrap();
		assert_eq!(
			choice(&["1.0.0", "3.0.0"], at(114) - Duration::from_secs(1)),
			Choice {
				installed: version("1.0.0"),
				newest: version("2.0.0"),
			}
		);
		assert_eq!(
			choice(&["1.0.0", "3.0.0"], at(114)),
			Choice {
				installed: version("1.0.0"),
				newest: version("2.1.0"),
			}
		);
		assert_eq!(
			choice(&["2.0.0", "2.1.0", "3.0.0"], at(124)),
			Choice {
				installed: version("3.0.0"),
				newest: version("3.0.0"),
			}
		);
		// Nothing installed is eligible: the run must install the newest eligible release.
		assert_eq!(
			choice(&["3.0.0"], at(50)),
			Choice {
				installed: None,
				newest: version("2.0.0"),
			}
		);
		assert_eq!(
			choice(&["1.0.0"], at(5)),
			Choice {
				installed: None,
				newest: None,
			}
		);
		// Each run applies its own delay: 0s takes everything, 7d holds back the newer releases.
		let none = "0s".parse().unwrap();
		assert_eq!(
			index.choose(&set(&["3.0.0"]), none, at(0)).unwrap().installed,
			version("3.0.0")
		);
		let week = "7d".parse().unwrap();
		assert_eq!(
			index.choose(&set(&["3.0.0", "1.0.0"]), week, at(200)).unwrap(),
			Choice {
				installed: version("1.0.0"),
				newest: version("2.0.0"),
			}
		);
		// An installed release the index no longer lists, such as a yanked one, never runs.
		let yanked = Index::parse(
			[line("3.0.0", true, Some(at(0))), line("2.0.0", false, Some(at(0)))]
				.join("\n")
				.as_bytes(),
			None,
			at(0),
		)
		.unwrap();
		assert_eq!(
			yanked.choose(&set(&["3.0.0"]), day, at(48)).unwrap(),
			Choice {
				installed: None,
				newest: version("2.0.0"),
			}
		);
	}

	#[test]
	fn a_missing_publish_time_fails_only_unpinned_delayed_choices_that_reach_it() {
		let index = index(&[
			line("3.0.0", false, Some(at(0))),
			line("2.0.0", false, None),
			line("1.0.0", false, Some(at(0))),
		]);
		let day = MinimumAge::DEFAULT;
		assert_eq!(
			index.choose(&set(&["3.0.0"]), day, at(24)).unwrap().installed,
			version("3.0.0")
		);
		let error = index.choose(&set(&["1.0.0"]), day, at(24)).unwrap_err();
		assert!(
			error.to_string().contains("no publish time for version 2.0.0"),
			"{error}"
		);
		let none = "0s".parse().unwrap();
		assert_eq!(
			index.choose(&set(&["1.0.0"]), none, at(24)).unwrap().installed,
			version("1.0.0")
		);
	}

	#[test]
	fn too_young_names_the_newest_release_and_when_it_qualifies() {
		let index = index(&[line("1.0.0", false, Some(at(0))), line("2.0.0", false, Some(at(10)))]);
		let message = index.too_young("tool", MinimumAge::DEFAULT).to_string();
		assert_eq!(
			message,
			format!(
				"no release of tool is 1d old yet: tool@2.0.0 was published at {}, and is eligible at {}; \
				 run tool@2.0.0 to pin it, or pass --minimum-release-age 0s",
				timestamp(at(10)),
				timestamp(at(34)),
			)
		);
		assert!(timestamp(at(10)).ends_with('Z'));
		assert_eq!(
			Index::parse(&b""[..], None, at(0))
				.unwrap()
				.too_young("tool", MinimumAge::DEFAULT)
				.to_string(),
			"tool has no non-yanked stable release"
		);
	}

	#[test]
	fn the_freshness_check_is_due_daily_or_after_a_clock_change() {
		let index = Index::parse(&b""[..], None, at(10)).unwrap();
		assert!(!index.due(at(10)));
		assert!(!index.due(at(34) - Duration::from_secs(1)));
		assert!(index.due(at(34)));
		assert!(index.due(at(9)));
	}

	#[test]
	fn validators_stand_only_for_the_releases_cached_beside_them() {
		fn response(status: &str, body: &str) -> Response {
			let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
			let address = listener.local_addr().unwrap();
			let wire = format!(
				"HTTP/1.1 {status}\r\nContent-Length: {}\r\nETag: new-validator\r\nConnection: close\r\n\r\n{body}",
				body.len()
			);
			let server = std::thread::spawn(move || {
				let (mut stream, _) = listener.accept().unwrap();
				let mut reader = BufReader::new(&mut stream);
				let mut line = String::new();
				loop {
					line.clear();
					assert!(std::io::BufRead::read_line(&mut reader, &mut line).unwrap() > 0);
					if line == "\r\n" {
						break;
					}
				}
				stream.write_all(wire.as_bytes()).unwrap();
			});
			let result = network::runtime()
				.unwrap()
				.block_on(reqwest::Client::new().get(format!("http://{address}")).send())
				.unwrap();
			server.join().unwrap();
			result
		}
		let receive = |cache, previous, response| {
			network::runtime()
				.unwrap()
				.block_on(Index::receive(cache, previous, response, at(5)))
		};
		let temp = tempfile::tempdir().unwrap();
		let cache = Cache::at(temp.path().join("fixture"));
		assert!(Index::load(&cache).unwrap().is_none());
		// A 304 needs the releases it confirms.
		assert!(receive(&cache, None, response("304 Not Modified", "")).is_err());
		let body = line("1.0.0", false, Some(at(0)));
		let fetched = receive(&cache, None, response("200 OK", &body)).unwrap();
		assert_eq!(fetched.etag.as_deref(), Some("new-validator"));
		let loaded = Index::load(&cache).unwrap().unwrap();
		assert_eq!(loaded.checked, at(5));
		assert_eq!(loaded.releases[0].version, Version::new(1, 0, 0));
		let previous = Index {
			checked: at(1),
			..loaded
		};
		let confirmed = network::runtime()
			.unwrap()
			.block_on(Index::receive(
				&cache,
				Some(previous),
				response("304 Not Modified", ""),
				at(30),
			))
			.unwrap();
		assert_eq!(confirmed.releases.len(), 1);
		assert_eq!(Index::load(&cache).unwrap().unwrap().checked, at(30));
		std::fs::write(Index::path(&cache), "malformed").unwrap();
		let error = format!("{:#}", Index::load(&cache).unwrap_err());
		assert!(error.contains(&Index::path(&cache).display().to_string()), "{error}");
	}
}
