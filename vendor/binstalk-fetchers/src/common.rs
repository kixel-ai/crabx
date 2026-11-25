#![allow(unused)]

use std::{
	future::Future,
	sync::{
		Once,
		atomic::{AtomicBool, Ordering::Relaxed},
	},
};

pub(super) use binstalk_downloader::{
	download::{DataVerifier, DownloadError},
	remote::{
		Client, StatusCode, Url,
		header::{self, HeaderMap},
		rate_limit_retry_after,
	},
};
pub(super) use binstalk_git_repo_api::gh_api_client::GhApiClient;
use binstalk_git_repo_api::gh_api_client::{GhApiError, GhReleaseArtifact, GhReleaseArtifactUrl};
pub(super) use binstalk_types::cargo_toml_binstall::{PkgFmt, PkgMeta};
pub(super) use compact_str::CompactString;
pub(super) use tokio::task::JoinHandle;
pub(super) use tracing::{debug, instrument, warn};

use crate::fetchers::FetchError;

static WARN_RATE_LIMIT_ONCE: Once = Once::new();
static TOKEN_REJECTED: AtomicBool = AtomicBool::new(false);

/// Whether GitHub's API rejected the token during discovery. The caller checks this before the
/// resolver's error to fail by name, only when discovery actually used GitHub.
pub fn token_rejected() -> bool {
	TOKEN_REJECTED.load(Relaxed)
}

/// Return Ok(Some(api_artifact_url)) if exists, or Ok(None) if it doesn't.
///
/// Caches info on all artifacts matching (repo, tag).
pub(super) async fn get_gh_release_artifact_url(
	gh_api_client: GhApiClient,
	artifact: GhReleaseArtifact,
) -> Result<Option<GhReleaseArtifactUrl>, GhApiError> {
	debug!("Using GitHub API to check for existence of artifact, which will also cache the API response");

	// Ask once, before the first query, so the hundreds of candidate URLs don't each resend a
	// rejected token and queue behind each other's 401s.
	if gh_api_client.has_gh_token() && !crate::network::github_token_accepted().await {
		TOKEN_REJECTED.store(true, Relaxed);
	}
	if token_rejected() {
		return Err(GhApiError::Unauthorized);
	}

	// The future returned has the same size as a pointer
	match gh_api_client.has_release_artifact(artifact).await {
		Ok(ret) => Ok(ret),
		Err(GhApiError::NotFound) => Ok(None),

		Err(GhApiError::RateLimit { retry_after }) => {
			WARN_RATE_LIMIT_ONCE.call_once(|| {
                warn!("Your GitHub API token (if any) has reached its rate limit and cannot be used again until {retry_after:?}, so we will fallback to HEAD/GET on the url.");
                warn!("If you did not supply a github token, consider doing so: GitHub limits unauthorized users to 60 requests per hour per origin IP address.");
            });
			Err(GhApiError::RateLimit { retry_after })
		}
		// Without a token, a 401 says nothing about the caller, who can only fall back.
		Err(GhApiError::Unauthorized) => {
			if gh_api_client.has_gh_token() {
				TOKEN_REJECTED.store(true, Relaxed);
			}
			Err(GhApiError::Unauthorized)
		}

		Err(err) => Err(err),
	}
}

/// Check if the URL exists by querying the GitHub API.
///
/// Caches info on all artifacts matching (repo, tag).
///
/// This function returns a future where its size should be at most size of
/// 2-4 pointers.
pub(super) async fn does_url_exist(client: Client, gh_api_client: GhApiClient, url: &Url) -> Result<bool, FetchError> {
	static GH_API_CLIENT_FAILED: AtomicBool = AtomicBool::new(false);

	debug!("Checking for package at: '{url}'");

	if !GH_API_CLIENT_FAILED.load(Relaxed)
		&& let Some(artifact) = GhReleaseArtifact::try_extract_from_url(url)
	{
		match get_gh_release_artifact_url(gh_api_client, artifact).await {
			Ok(ret) => return Ok(ret.is_some()),

			Err(GhApiError::RateLimit { .. }) => {}
			Err(GhApiError::Unauthorized) if !token_rejected() => {}

			Err(err) => return Err(err.into()),
		}

		GH_API_CLIENT_FAILED.store(true, Relaxed);
	}

	remote_exists(&client, url.clone()).await
}

/// Whether `url` serves a file. A host that never or no longer has it (404, 410) or refuses
/// the tokenless client (a 403 without rate-limit headers) is a miss; any other answer but
/// success is an error rather than a miss, so a transient failure never falls through to a less
/// preferred candidate.
pub(super) async fn remote_exists(client: &Client, url: Url) -> Result<bool, FetchError> {
	let response = Box::pin(client.get(url.clone()).send(false)).await?;
	let status = response.status();
	if matches!(status, StatusCode::NOT_FOUND | StatusCode::GONE)
		|| (status == StatusCode::FORBIDDEN && !rate_limited(response.headers()))
	{
		return Ok(false);
	}
	let status = response.error_for_status()?.status();
	if !status.is_success() {
		// A redirect left unfollowed, such as one without a Location, is a broken host, not a miss.
		let err = std::io::Error::other(format!("{url} answered {status}, not a file"));
		return Err(DownloadError::from(err).into());
	}
	Ok(true)
}

/// The most bytes of a signature file, or of quickinstall's target list, a host may send.
pub(super) const SMALL_FILE: u64 = 64 << 10;

/// `url`'s body, failing once it passes `limit` bytes.
pub(super) async fn download_bytes(client: &Client, url: Url, limit: u64) -> Result<Vec<u8>, FetchError> {
	let mut bytes = Vec::new();
	read_body(client, url, limit, |chunk| {
		bytes.extend_from_slice(chunk);
		Ok(())
	})
	.await?;
	Ok(bytes)
}

/// Stream `url`'s body into `file` through `verifier`, failing once it passes `limit` bytes.
pub(super) async fn download_to(
	client: &Client,
	url: Url,
	file: &mut std::fs::File,
	verifier: &mut dyn DataVerifier,
	limit: u64,
) -> Result<(), FetchError> {
	use std::io::Write;
	read_body(client, url, limit, |chunk| {
		verifier.update(chunk);
		file.write_all(chunk)
	})
	.await
}

async fn read_body(
	client: &Client,
	url: Url,
	limit: u64,
	mut sink: impl FnMut(&bytes::Bytes) -> std::io::Result<()>,
) -> Result<(), FetchError> {
	let mut response = client.get(url.clone()).send(true).await?;
	let too_large = || FetchError::TooLarge(Box::new(url.clone()), limit);
	let declared = response.headers().get(header::CONTENT_LENGTH);
	if declared
		.and_then(|length| length.to_str().ok()?.parse::<u64>().ok())
		.is_some_and(|length| length > limit)
	{
		return Err(too_large());
	}
	let mut total = 0u64;
	while let Some(chunk) = response.chunk().await? {
		total += chunk.len() as u64;
		if total > limit {
			return Err(too_large());
		}
		sink(&chunk).map_err(DownloadError::from)?;
	}
	Ok(())
}

/// Whether a 403 asks the client to wait, as GitHub's rate limits do, rather than refusing it.
fn rate_limited(headers: &HeaderMap) -> bool {
	headers.contains_key(header::RETRY_AFTER) || rate_limit_retry_after(headers).is_some()
}

#[derive(Debug)]
pub(super) struct AutoAbortJoinHandle<T>(JoinHandle<T>);

impl<T> AutoAbortJoinHandle<T>
where
	T: Send + 'static,
{
	pub(super) fn spawn<F>(future: F) -> Self
	where
		F: Future<Output = T> + Send + 'static,
	{
		Self(tokio::spawn(future))
	}
}

impl<T> Drop for AutoAbortJoinHandle<T> {
	fn drop(&mut self) {
		self.0.abort();
	}
}

impl<T, E> AutoAbortJoinHandle<Result<T, E>>
where
	E: Into<FetchError>,
{
	pub(super) async fn flattened_join(mut self) -> Result<T, FetchError> {
		(&mut self.0).await?.map_err(Into::into)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn signatures_and_archives_stop_at_their_limit() {
		struct Counted(u64);
		impl DataVerifier for Counted {
			fn update(&mut self, data: &bytes::Bytes) {
				self.0 += data.len() as u64;
			}
			fn validate(&mut self) -> bool {
				true
			}
		}
		let builder = Client::default_builder("crabx-test", None, true, Default::default(), &mut std::iter::empty());
		let client = Client::from_builder(builder.no_proxy(), 1.try_into().unwrap(), 100.try_into().unwrap()).unwrap();
		let serve =
			|declared| async move { Url::parse(&crate::network::tests::serve(vec![7; 1000], declared).await).unwrap() };
		crate::network::runtime().unwrap().block_on(async {
			for declared in [true, false] {
				let error = download_bytes(&client, serve(declared).await, 999).await.unwrap_err();
				assert!(matches!(error, FetchError::TooLarge(_, 999)), "{error}");
				assert_eq!(
					download_bytes(&client, serve(declared).await, 1000).await.unwrap(),
					[7; 1000]
				);
				let mut file = tempfile::tempfile().unwrap();
				let mut counted = Counted(0);
				let error = download_to(&client, serve(declared).await, &mut file, &mut counted, 999).await;
				assert!(matches!(error, Err(FetchError::TooLarge(_, 999))));
				// Nothing past the limit is written or verified.
				assert!(file.metadata().unwrap().len() <= 999 && counted.0 <= 999);
				let mut file = tempfile::tempfile().unwrap();
				let mut counted = Counted(0);
				download_to(&client, serve(declared).await, &mut file, &mut counted, 1000)
					.await
					.unwrap();
				assert_eq!((file.metadata().unwrap().len(), counted.0), (1000, 1000));
			}
		});
	}

	#[test]
	fn only_a_bare_403_is_a_refusal() {
		let headers = |pairs: &[(&'static str, &'static str)]| {
			let mut map = HeaderMap::new();
			for (name, value) in pairs {
				map.insert(*name, header::HeaderValue::from_static(value));
			}
			map
		};
		assert!(!rate_limited(&headers(&[])));
		assert!(!rate_limited(&headers(&[("x-ratelimit-remaining", "12")])));
		assert!(rate_limited(&headers(&[("retry-after", "60")])));
		assert!(rate_limited(&headers(&[("retry-after", "soon")])));
		assert!(rate_limited(&headers(&[("x-ratelimit-remaining", "0")])));
	}
}
