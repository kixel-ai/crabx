//! Which GitHub repository an archive's download origin binds it to.
//!
//! Attestations are queried for one repository, so an archive that GitHub serves from another
//! (through `pkg-url`, a repository redirect, or an archive redirect) would read as unattested
//! there. Any disagreement between those origins is an error, never a guess.

use crate::fetchers::Receipt;
use anyhow::{Context, Result, bail, ensure};
use reqwest::{Url, redirect::Policy};
use std::{
	collections::{BTreeMap, BTreeSet, HashMap},
	sync::{Arc, Mutex},
};

/// The redirect hops each request followed, by the URL it started from.
#[derive(Clone, Default)]
pub struct Redirects(Arc<Mutex<HashMap<Url, BTreeSet<Vec<Url>>>>>);

impl Redirects {
	/// A redirect policy that follows up to 10 hops, like reqwest's default, and records them.
	// Each request adds at most 10 URLs, and requests are bounded by the candidate URLs tried.
	pub fn policy(&self) -> Policy {
		let log = self.clone();
		Policy::custom(move |attempt| {
			if attempt.previous().len() > 10 {
				return attempt.error("too many redirects");
			}
			let mut hops = attempt.previous().to_vec();
			let start = unfragmented(hops.remove(0));
			hops.push(attempt.url().clone());
			log.0.lock().unwrap().entry(start).or_default().insert(hops);
			attempt.follow()
		})
	}

	/// Every chain of hops requests for `start` followed, each beginning after `start`.
	fn chains(&self, start: &Url) -> Vec<Vec<Url>> {
		self.0
			.lock()
			.unwrap()
			.get(&unfragmented(start.clone()))
			.map(|chains| chains.iter().cloned().collect())
			.unwrap_or_default()
	}
}

// A request's redirect chain starts at its URL without the fragment, which is never sent.
fn unfragmented(mut url: Url) -> Url {
	url.set_fragment(None);
	url
}

/// Where an archive came from, as far as attestations can tell.
pub struct Origin {
	/// The GitHub repository the archive must be attested in, or none when GitHub is not involved.
	pub repo: Option<String>,
	/// The GitHub release asset the archive was finally served from, or its initial URL. A
	/// `releases/latest/download` URL only reveals its release tag by redirecting.
	pub asset: Url,
}

/// Find `receipt`'s origin. A conflicting or ambiguous GitHub origin is an error.
pub fn resolve(receipt: &Receipt, redirects: &Redirects) -> Result<Origin> {
	let chains = redirects.chains(&receipt.url);
	Ok(Origin {
		repo: origin(
			receipt.declared.as_deref(),
			receipt.repository.as_deref(),
			receipt.explicit,
			&receipt.url,
			&chains,
		)?,
		asset: release_asset(&receipt.url, &chains)?,
	})
}

fn release_asset(url: &Url, chains: &[Vec<Url>]) -> Result<Url> {
	// A chain that another extends is one request's earlier hops, not another path.
	let full: Vec<_> = chains
		.iter()
		.filter(|chain| {
			!chains
				.iter()
				.any(|other| other.len() > chain.len() && other.starts_with(chain))
		})
		.collect();
	let direct = Vec::new();
	let full = if full.is_empty() { vec![&direct] } else { full };
	// A release is its repository and decoded tag; a redirect's query string is not part of it.
	let mut found = BTreeMap::new();
	let initial = release(url)?;
	for chain in full {
		let mut hops = Vec::new();
		for hop in chain {
			if let Some(id) = release(hop)? {
				hops.push((id, hop));
			}
		}
		// An initial URL that names a tag keeps it; only one that names none, like
		// `releases/latest/download`, takes its release from the redirects.
		let chosen = match &initial {
			Some(id) => {
				ensure!(
					hops.iter().all(|(other, _)| other == id),
					"the archive's redirects lead to another GitHub release than {url}"
				);
				Some((id.clone(), url))
			}
			None => hops.pop(),
		};
		if let Some((id, asset)) = chosen {
			found.entry(id).or_insert(asset);
		}
	}
	ensure!(
		found.len() <= 1,
		"the archive's redirects lead to several GitHub releases"
	);
	Ok(found.into_values().next().unwrap_or(url).clone())
}

/// The `(repository, tag)` of a GitHub release asset URL, and none for any other URL.
fn release(url: &Url) -> Result<Option<(String, String)>> {
	let Some(repo) = repository_url(url)? else {
		return Ok(None);
	};
	let Some(tag) = url
		.path()
		.split("/releases/download/")
		.nth(1)
		.and_then(|rest| rest.split('/').next())
	else {
		return Ok(None);
	};
	let tag = percent_encoding::percent_decode_str(tag)
		.decode_utf8()
		.context("invalid release tag")?;
	Ok(Some((repo, tag.into_owned())))
}

fn origin(
	declared: Option<&str>,
	resolved: Option<&str>,
	explicit: bool,
	url: &Url,
	chains: &[Vec<Url>],
) -> Result<Option<String>> {
	let parse = |value: &str| repository_url(&Url::parse(value).context("invalid crate repository URL")?);
	let declared = declared.map(parse).transpose()?.flatten();
	let resolved_github = resolved.map(parse).transpose()?.flatten();
	// Binstalk derives its default URLs from the resolved repository, so it is the one to query.
	ensure!(
		declared.is_none() || resolved_github.is_some(),
		"the crate's GitHub repository redirects outside GitHub"
	);
	// A release repository is named by a GitHub URL in the archive's initial URL or any redirect.
	// The CDN names none, so it must directly follow a hop that does.
	let mut releases = BTreeSet::new();
	let direct: &[Url] = &[];
	for chain in chains.iter().map(Vec::as_slice).chain([direct]) {
		let mut named = false;
		for hop in std::iter::once(url).chain(chain) {
			if let Some(repo) = repository_url(hop)? {
				releases.insert(repo);
				named = true;
			} else if cdn(hop) {
				ensure!(
					named,
					"{hop} is a GitHub download that no GitHub release URL led to, so its repository is unknown"
				);
			} else {
				// A detour through another host leaves the CDN asset's owner unproven.
				named = false;
			}
		}
	}
	ensure!(
		releases.len() <= 1,
		"the archive comes from several GitHub repositories: {}",
		releases.iter().cloned().collect::<Vec<_>>().join(", ")
	);
	let release = releases.pop_first();
	let declares = |repo: &str| explicit && repository_url(url).ok().flatten().as_deref() == Some(repo);
	match (resolved_github, release) {
		(Some(repo), Some(release)) => {
			ensure!(
				repo == release,
				"the crate's repository is {repo}, but the archive comes from GitHub repository {release}"
			);
			ensure!(
				declared.is_some() || declares(&repo),
				"the crate's repository redirects to GitHub repository {repo}, which only its own pkg-url may claim"
			);
			Ok(Some(repo))
		}
		(Some(repo), None) => {
			ensure!(
				declared.is_some(),
				"the crate's repository redirects to GitHub repository {repo}, which only its own pkg-url may claim"
			);
			Ok(Some(repo))
		}
		(None, Some(release)) => {
			ensure!(
				declares(&release),
				"the archive comes from GitHub repository {release}, which the crate's own pkg-url must name"
			);
			Ok(Some(release))
		}
		(None, None) => Ok(None),
	}
}

fn cdn(url: &Url) -> bool {
	url.host_str()
		.is_some_and(|host| host == "githubusercontent.com" || host.ends_with(".githubusercontent.com"))
}

/// The lowercase `owner/name` of a github.com URL, and none for any other host.
pub fn repository_url(url: &Url) -> Result<Option<String>> {
	if !matches!(url.host_str(), Some("github.com" | "www.github.com")) {
		return Ok(None);
	}
	if url.scheme() != "https" && url.scheme() != "http" || !url.username().is_empty() || url.password().is_some() {
		bail!("invalid GitHub URL {url}");
	}
	let mut segments = url.path_segments().context("repository has no path")?;
	let owner = segments.next().context("repository has no owner")?;
	let repo = segments
		.next()
		.context("repository has no name")?
		.trim_end_matches(".git");
	ensure!(!owner.is_empty() && !repo.is_empty(), "repository has no owner or name");
	Ok(Some(format!("{owner}/{repo}").to_ascii_lowercase()))
}

#[cfg(test)]
mod tests {
	use super::*;

	fn url(value: &str) -> Url {
		Url::parse(value).unwrap()
	}

	const ASSET: &str = "https://github.com/a/b/releases/download/v1.0.0/tool-x86_64.tgz";
	const CDN: &str = "https://release-assets.githubusercontent.com/github-production-release-asset/1?sig=x";

	fn check(
		declared: Option<&str>,
		resolved: Option<&str>,
		explicit: bool,
		asset: &str,
		chains: &[&[&str]],
	) -> Result<Option<String>> {
		let chains: Vec<Vec<Url>> = chains.iter().map(|c| c.iter().map(|u| url(u)).collect()).collect();
		origin(declared, resolved, explicit, &url(asset), &chains)
	}

	fn repo(value: &str) -> Result<Option<String>> {
		repository_url(&url(value))
	}

	#[test]
	fn repositories_are_canonical() {
		assert_eq!(
			repo("https://github.com/moq-dev/moq.git").unwrap().as_deref(),
			Some("moq-dev/moq")
		);
		assert_eq!(
			repo("https://github.com/moq-dev/moq/tree/main/rs/moq-cli")
				.unwrap()
				.as_deref(),
			Some("moq-dev/moq")
		);
		assert!(repo("https://gitlab.com/a/b").unwrap().is_none());
		assert!(repo("https://github.com/a").is_err());
		assert_eq!(repo("http://github.com/a/b").unwrap().as_deref(), Some("a/b"));
		assert_eq!(repo("https://www.github.com/A/b").unwrap().as_deref(), Some("a/b"));
		assert!(repo("https://user@github.com/a/b").is_err());
	}

	#[test]
	fn the_release_tag_comes_from_the_final_github_release_url() {
		let latest = url("https://github.com/a/b/releases/latest/download/tool.tgz");
		let tagged = url("https://github.com/a/b/releases/download/tool-v1.2.3/tool.tgz");
		let cdn = url(CDN);
		let asset = |start: &Url, chains: &[Vec<Url>]| release_asset(start, chains).unwrap();
		assert_eq!(asset(&latest, &[]), latest);
		assert_eq!(asset(&url(ASSET), &[]), url(ASSET));
		// Each prefix of a request's chain is also recorded, and none disagrees with the whole.
		let chains = [vec![tagged.clone()], vec![tagged.clone(), cdn.clone()]];
		assert_eq!(asset(&latest, &chains), tagged);
		assert_eq!(asset(&url(ASSET), &[vec![cdn.clone()]]), url(ASSET));
		let other = url("https://github.com/a/b/releases/download/v9/tool.tgz");
		assert!(release_asset(&latest, &[vec![tagged.clone()], vec![other]]).is_err());
		// A requested tag survives a redirect, and a redirect to another tag is an error.
		let pinned = url("https://github.com/a/b/releases/download/tool-v1.2.3/tool.tgz");
		assert_eq!(asset(&pinned, &[vec![tagged.clone()]]), pinned);
		assert!(release_asset(&url(ASSET), &[vec![tagged.clone()]]).is_err());
		let bumped = url("https://github.com/a/b/releases/download/tool-v1.2.4/tool.tgz");
		assert!(release_asset(&pinned, &[vec![bumped]]).is_err());
		// A query string or an encoded tag is the same release, not a second one.
		let query = url("https://github.com/a/b/releases/download/tool-v1.2.3/tool.tgz?X=1");
		let encoded = url("https://github.com/a/b/releases/download/tool%2Dv1.2.3/tool.tgz");
		assert_eq!(asset(&pinned, &[vec![query.clone()]]), pinned);
		assert_eq!(asset(&latest, &[vec![tagged.clone()], vec![query], vec![encoded]]), tagged);
	}

	#[test]
	fn a_github_repository_binds_its_own_releases() {
		let github = Some("https://github.com/a/b");
		// Default URLs, explicit ones, and a release the CDN serves after naming its repository.
		assert_eq!(
			check(github, github, false, ASSET, &[]).unwrap().as_deref(),
			Some("a/b")
		);
		assert_eq!(
			check(github, github, true, ASSET, &[&[CDN]]).unwrap().as_deref(),
			Some("a/b")
		);
		assert_eq!(
			check(github, github, false, "https://example.com/tool.tgz", &[])
				.unwrap()
				.as_deref(),
			Some("a/b")
		);
		// A renamed repository is queried by its new name, which its default URLs already use.
		let renamed = Some("https://github.com/a/new");
		assert_eq!(
			check(
				github,
				renamed,
				false,
				"https://github.com/a/new/releases/download/v1/t.tgz",
				&[]
			)
			.unwrap()
			.as_deref(),
			Some("a/new")
		);
	}

	#[test]
	fn a_release_in_another_repository_conflicts() {
		let github = Some("https://github.com/a/b");
		let other = "https://github.com/c/d/releases/download/v1.0.0/tool.tgz";
		// pkg-url, a redirect from a pkg-url elsewhere, and a redirect from the repository's own release.
		for (asset, chains) in [
			(other, &[][..]),
			("https://example.com/tool.tgz", &[&[other][..]][..]),
			(ASSET, &[&[other, CDN][..]][..]),
			// One request's chain does not excuse another's.
			(ASSET, &[&[CDN][..], &[other][..]][..]),
		] {
			let error = check(github, github, true, asset, chains).unwrap_err().to_string();
			assert!(error.contains("c/d"), "{error}");
		}
	}

	#[test]
	fn a_repository_redirecting_outside_github_or_into_it_is_ambiguous() {
		let github = Some("https://github.com/a/b");
		let gitlab = Some("https://gitlab.com/a/b");
		assert!(check(github, gitlab, false, "https://gitlab.com/a/b/-/releases/t.tgz", &[]).is_err());
		// A project page binstalk then builds GitHub release URLs from.
		let error = check(gitlab, github, false, ASSET, &[]).unwrap_err().to_string();
		assert!(error.contains("its own pkg-url"), "{error}");
		// Even when nothing downloads from GitHub, and when only pkg-url, not the template's default, may claim it.
		assert!(check(gitlab, github, false, "https://example.com/t.tgz", &[]).is_err());
		assert!(check(gitlab, github, true, "https://example.com/t.tgz", &[]).is_err());
		assert_eq!(check(gitlab, github, true, ASSET, &[]).unwrap().as_deref(), Some("a/b"));
		assert!(check(gitlab, github, true, ASSET, &[&[CDN]]).is_ok());
		// A GitHub release reached from a non-GitHub archive URL or with no repository at all.
		assert!(check(gitlab, gitlab, false, "https://example.com/t.tgz", &[&[ASSET]]).is_err());
		assert!(check(gitlab, gitlab, true, ASSET, &[]).unwrap().is_some());
		assert!(check(gitlab, gitlab, false, ASSET, &[]).is_err());
		assert!(check(None, None, false, "https://example.com/t.tgz", &[&[ASSET]]).is_err());
		assert!(check(None, None, false, ASSET, &[]).is_err());
	}

	#[test]
	fn a_pkg_url_alone_names_its_github_repository() {
		assert_eq!(check(None, None, true, ASSET, &[]).unwrap().as_deref(), Some("a/b"));
		assert_eq!(
			check(None, None, true, ASSET, &[&[CDN]]).unwrap().as_deref(),
			Some("a/b")
		);
		assert!(
			check(None, None, true, "https://example.com/t.tgz", &[])
				.unwrap()
				.is_none()
		);
		assert!(
			check(
				Some("https://gitlab.com/a/b"),
				Some("https://gitlab.com/a/b"),
				false,
				"https://example.com/t.tgz",
				&[]
			)
			.unwrap()
			.is_none()
		);
	}

	#[test]
	fn a_github_cdn_download_needs_an_earlier_release_url() {
		let github = Some("https://github.com/a/b");
		// Straight to the CDN, or through a redirect that never named a repository: the repository
		// attestations are checked against may not own the asset.
		for (asset, chains) in [
			(CDN, &[][..]),
			("https://example.com/t.tgz", &[&[CDN][..]][..]),
			(ASSET, &[&[CDN][..], &["https://example.com/x", CDN][..]][..]),
			("https://objects.githubusercontent.com/x", &[][..]),
		] {
			for (declared, explicit) in [(github, false), (github, true), (None, true)] {
				let error = check(declared, declared, explicit, asset, chains)
					.unwrap_err()
					.to_string();
				assert!(error.contains("no GitHub release URL led to"), "{error}");
			}
		}
		assert!(
			check(
				github,
				github,
				false,
				ASSET,
				&[&[CDN, "https://objects.githubusercontent.com/y"]]
			)
			.is_ok()
		);
	}

	#[test]
	fn a_malformed_origin_is_an_error() {
		assert!(check(Some("not a url"), None, false, ASSET, &[]).is_err());
		assert!(
			check(
				Some("https://github.com/a"),
				Some("https://github.com/a"),
				false,
				ASSET,
				&[]
			)
			.is_err()
		);
		assert!(check(None, None, true, "https://github.com/a/b/", &[&["https://github.com/"]]).is_err());
	}

	#[test]
	fn redirects_are_recorded_per_starting_url_and_bounded() {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};
		crate::network::runtime().unwrap().block_on(async {
			let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
			let address = listener.local_addr().unwrap();
			tokio::spawn(async move {
				loop {
					let (mut stream, _) = listener.accept().await.unwrap();
					tokio::spawn(async move {
						let mut request = Vec::new();
						while !request.ends_with(b"\r\n\r\n") {
							request.push(stream.read_u8().await.unwrap());
						}
						let path = String::from_utf8_lossy(&request);
						let path = path.split(' ').nth(1).unwrap();
						let response = match path.strip_prefix("/hop") {
							// /hopN redirects to /hopN+1 forever, /end serves a file.
							Some(n) => format!(
								"HTTP/1.1 302 Found\r\nLocation: /hop{}\r\nContent-Length: 0\r\n\r\n",
								n.parse::<u32>().unwrap() + 1
							),
							None if path == "/start" => {
								"HTTP/1.1 302 Found\r\nLocation: /middle\r\nContent-Length: 0\r\n\r\n".into()
							}
							None if path == "/middle" => {
								"HTTP/1.1 302 Found\r\nLocation: /end\r\nContent-Length: 0\r\n\r\n".into()
							}
							None => "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".into(),
						};
						let _ = stream.write_all(response.as_bytes()).await;
					});
				}
			});
			let redirects = Redirects::default();
			let client = reqwest::Client::builder()
				.no_proxy()
				.redirect(redirects.policy())
				.build()
				.unwrap();
			let start = url(&format!("http://{address}/start"));
			assert!(client.get(start.clone()).send().await.unwrap().status().is_success());
			let middle = url(&format!("http://{address}/middle"));
			let end = url(&format!("http://{address}/end"));
			assert_eq!(redirects.chains(&start), [vec![middle.clone()], vec![middle, end]]);
			// A fragment is never sent, so it names the same request.
			let fragment = url(&format!("http://{address}/start#asset"));
			assert_eq!(redirects.chains(&fragment), redirects.chains(&start));
			assert!(client.get(fragment.clone()).send().await.unwrap().status().is_success());
			assert_eq!(redirects.chains(&fragment).len(), 2);
			assert!(redirects.chains(&url(&format!("http://{address}/end"))).is_empty());
			// Like reqwest's default, ten redirects are followed and an eleventh is an error.
			let endless = client.get(format!("http://{address}/hop0")).send().await.unwrap_err();
			assert!(endless.is_redirect(), "{endless}");
		});
	}
}
