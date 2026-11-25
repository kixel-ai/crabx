use crate::fetchers::Receipt;
use crate::network;
use anyhow::{Context, Result, bail, ensure};
use semver::Version;
use serde::Deserialize;
use sigstore_verify::{
	VerificationPolicy, Verifier,
	trust_root::{TrustedRoot, TufConfig},
	types::{Bundle, Sha256Hash, SignatureContent},
};
use std::{fs::File, io::BufReader, path::Path};

#[derive(Deserialize)]
struct Page {
	#[serde(deserialize_with = "one_attestation")]
	attestations: Option<Attestation>,
}

#[derive(Deserialize)]
struct Attestation {
	bundle: Option<Bundle>,
	bundle_url: Option<reqwest::Url>,
}

fn one_attestation<'de, D: serde::Deserializer<'de>>(
	deserializer: D,
) -> std::result::Result<Option<Attestation>, D::Error> {
	struct One;
	impl<'de> serde::de::Visitor<'de> for One {
		type Value = Option<Attestation>;
		fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
			f.write_str("a page containing at most one attestation")
		}
		fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut seq: A) -> std::result::Result<Self::Value, A::Error> {
			let first = seq.next_element()?;
			if seq.next_element::<serde::de::IgnoredAny>()?.is_some() {
				return Err(serde::de::Error::custom("GitHub ignored the attestation page size"));
			}
			Ok(first)
		}
	}
	deserializer.deserialize_seq(One)
}

/// The crate and release `receipt`'s archive must be attested as, and the GitHub repository
/// `origin` found that attests it, if any.
pub struct Subject<'a> {
	pub name: &'a str,
	pub version: &'a Version,
	pub repo: Option<String>,
}

pub async fn verify(receipt: &Receipt, subject: Subject<'_>, stage: &Path) -> Result<String> {
	let Subject { name, version, repo } = subject;
	let Some(repo) = repo else {
		return Ok("author release (repository does not support GitHub attestations)".into());
	};
	let digest = Sha256Hash::new(receipt.sha256).to_hex();
	let client = network::client()?;
	// Request one bundle per page. An attestation for a different digest is never queried.
	let endpoint = format!("https://api.github.com/repos/{repo}/attestations/sha256:{digest}");
	let mut url = reqwest::Url::parse(&format!("{endpoint}?per_page=1"))?;
	let token = network::github_token()?;
	let mut first_page = true;
	let mut verified = 0;
	let mut roots: [Option<TrustedRoot>; 2] = [None, None];
	loop {
		let mut request = client.get(url.clone()).header("Accept", "application/vnd.github+json");
		if let Some(token) = &token {
			request = request.bearer_auth(token);
		}
		let response = request.send().await?;
		ensure!(
			response.url().host_str() == Some("api.github.com"),
			"GitHub API redirected outside GitHub"
		);
		// GitHub returns 404 when there are no attestations for this digest.
		if response.status() == reqwest::StatusCode::NOT_FOUND && first_page {
			return Ok("author release (no published attestation)".into());
		}
		ensure!(
			response.status() != reqwest::StatusCode::UNAUTHORIZED || token.is_none(),
			network::REJECTED_TOKEN
		);
		let response = response.error_for_status().context("query GitHub attestations")?;
		let next = response.headers().get("link").map(|v| v.to_str()).transpose()?;
		let next = next_page(next, &url, &repo, &digest)?;
		let file = network::spool(response, stage).await?;
		let page: Page = serde_json::from_reader(BufReader::new(File::open(file.path())?))?;
		let Some(attestation) = page.attestations else {
			ensure!(next.is_none(), "empty attestation page has a next page");
			break;
		};
		let bundle = match attestation.bundle {
			Some(bundle) => bundle,
			None => {
				let url = attestation
					.bundle_url
					.context("attestation has neither bundle nor bundle_url")?;
				ensure!(
					matches!(url.scheme(), "https" | "http") && url.username().is_empty() && url.password().is_none(),
					"invalid attestation bundle URL"
				);
				// External bundle storage receives no GitHub credential.
				let response = client.get(url).send().await?.error_for_status()?;
				let file = network::spool(response, stage).await?;
				external_bundle(file.path())?
			}
		};
		// GitHub's timestamp-only trust domain has its own CA and TSA. A
		// public-good bundle cannot lose its log and verify under that CA.
		let github = bundle.verification_material.tlog_entries.is_empty();
		let root = match &mut roots[usize::from(github)] {
			Some(root) => root,
			slot => slot.insert(if github {
				TrustedRoot::from_tuf(TufConfig::github()).await?
			} else {
				TrustedRoot::production().await?
			}),
		};
		let verifier = Verifier::new(root)?;
		verify_bundle(
			&verifier,
			&bundle,
			Artifact {
				digest: &digest,
				repo: &repo,
				name,
				version,
				asset: &receipt.url,
			},
		)?;
		verified += 1;
		let Some(next) = next else { break };
		url = next;
		first_page = false;
	}
	if verified > 0 {
		Ok(format!("GitHub attestations ({verified}) sha256:{digest}"))
	} else {
		Ok("author release (no published attestation)".into())
	}
}

// Raw Snappy copy tags expand at most 64 bytes per tag. Bound allocation by
// the compressed input, rather than trusting its advertised output length.
fn external_bundle(path: &Path) -> Result<Bundle> {
	let compressed = std::fs::read(path)?;
	let length = snap::raw::decompress_len(&compressed)?;
	ensure!(
		length <= compressed.len().saturating_mul(64),
		"invalid Snappy expansion"
	);
	let json = snap::raw::Decoder::new().decompress_vec(&compressed)?;
	Ok(serde_json::from_slice(&json)?)
}

fn next_page(link: Option<&str>, current: &reqwest::Url, repo: &str, digest: &str) -> Result<Option<reqwest::Url>> {
	let mut next = None;
	for part in link.into_iter().flat_map(|link| link.split(',')) {
		let mut fields = part.split(';');
		let target = fields.next().context("missing pagination URL")?.trim();
		if !fields.any(|field| field.trim() == "rel=\"next\"") {
			continue;
		}
		ensure!(next.is_none(), "duplicate next attestation page");
		let target = target
			.strip_prefix('<')
			.and_then(|s| s.strip_suffix('>'))
			.context("invalid pagination URL")?;
		let url = reqwest::Url::parse(target)?;
		let suffix = format!("/attestations/sha256:{digest}");
		let path = url
			.path()
			.strip_suffix(&suffix)
			.context("pagination changed the artifact digest")?;
		// GitHub canonicalizes /repos/owner/name to /repositories/<numeric ID>
		// in Link headers. Signed identities remain bound to the original repo.
		let canonical = path
			.strip_prefix("/repositories/")
			.is_some_and(|id| !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()));
		ensure!(
			path == format!("/repos/{repo}") || canonical,
			"invalid attestation pagination path"
		);
		ensure!(
			url.scheme() == "https"
				&& url.host_str() == Some("api.github.com")
				&& url.port().is_none()
				&& url.username().is_empty()
				&& url.password().is_none()
				&& url.fragment().is_none(),
			"invalid attestation pagination origin"
		);
		ensure!(
			url.query_pairs()
				.filter(|(key, _)| key == "per_page")
				.map(|(_, value)| value)
				.collect::<Vec<_>>()
				== ["1"],
			"pagination changed the attestation page size"
		);
		ensure!(url != *current, "GitHub repeated the attestation page");
		next = Some(url);
	}
	Ok(next)
}

#[derive(Clone, Copy)]
struct Artifact<'a> {
	digest: &'a str,
	repo: &'a str,
	/// The crate the archive must have been built from.
	name: &'a str,
	version: &'a Version,
	asset: &'a reqwest::Url,
}

fn verify_bundle(verifier: &Verifier, bundle: &Bundle, artifact: Artifact<'_>) -> Result<()> {
	let github = bundle.verification_material.tlog_entries.is_empty();
	let release = release(bundle)?;
	let mut policy = if release.is_some() {
		ensure!(github, "GitHub release attestation uses the wrong trust domain");
		VerificationPolicy::any_identity().require_identity("https://dotcom.releases.github.com")
	} else {
		VerificationPolicy::any_identity().require_issuer("https://token.actions.githubusercontent.com")
	};
	if github {
		// This CA issues timestamp-only bundles without public CT or Rekor.
		// The verifier still requires a verified RFC 3161 time, CA chain,
		// signature and artifact digest. Never apply this policy to the
		// public-good root in the download path.
		policy = policy.skip_tlog_unsafe().skip_sct();
	}
	let result = verifier
		.verify(Sha256Hash::from_hex(artifact.digest)?, bundle, &policy)
		.context("invalid published attestation")?;
	ensure!(
		result.certificate_verified()
			&& !result.verified_timestamps().is_empty()
			&& (github || (result.sct_verified() && result.tlog_verified())),
		"attestation verification is incomplete"
	);
	if let Some(release) = release {
		ensure!(
			release.repository.eq_ignore_ascii_case(artifact.repo),
			"release attestation names another repository"
		);
		return artifact.tag(&release.tag);
	}
	let cert = result.certificate().context("attestation has no certificate")?;
	let signer = artifact.authorize(
		&cert.ci_claims,
		cert.identity.as_ref().map(|identity| identity.as_str()),
	)?;
	if signer == Signer::Pipeline {
		artifact.check_build(statement(bundle)?)?;
	}
	Ok(())
}

#[derive(Deserialize)]
struct Release {
	repository: String,
	tag: String,
}

/// The in-toto statement a DSSE bundle signs, if it is one.
fn payload(bundle: &Bundle) -> Result<Option<&[u8]>> {
	let SignatureContent::DsseEnvelope(envelope) = &bundle.content else {
		return Ok(None);
	};
	ensure!(
		envelope.payload_type == "application/vnd.in-toto+json",
		"unsupported attestation payload type"
	);
	Ok(Some(envelope.payload.as_bytes()))
}

fn statement(bundle: &Bundle) -> Result<&[u8]> {
	payload(bundle)?.context("pipeline attestation is not an in-toto statement")
}

fn release(bundle: &Bundle) -> Result<Option<Release>> {
	let Some(payload) = payload(bundle)? else {
		return Ok(None);
	};
	#[derive(Deserialize)]
	struct Header {
		#[serde(rename = "predicateType")]
		kind: String,
	}
	let header: Header = serde_json::from_slice(payload)?;
	if header.kind != "https://in-toto.io/attestation/release/v0.2" {
		return Ok(None);
	}
	#[derive(Deserialize)]
	struct Statement {
		predicate: Release,
	}
	let statement: Statement = serde_json::from_slice(payload)?;
	Ok(Some(statement.predicate))
}

/// The reusable workflow whose builds crabx trusts for any repository that calls it.
const PIPELINE_SIGNER: &str = "https://github.com/kixel-ai/crabx/.github/workflows/crabx.yml@";

/// Who signed an attestation: the crate's own workflow, or crabx's pipeline.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Signer {
	Author,
	Pipeline,
}

impl Artifact<'_> {
	fn authorize(&self, claims: &sigstore_verify::crypto::FulcioCiClaims, identity: Option<&str>) -> Result<Signer> {
		let repo = self.repo;
		let expected = format!("https://github.com/{repo}");
		let matches = match claims.source_repository_uri.as_deref() {
			Some(uri) => uri.eq_ignore_ascii_case(&expected),
			None => claims
				.deprecated_github
				.workflow_repository
				.as_deref()
				.is_some_and(|value| value.eq_ignore_ascii_case(repo)),
		};
		ensure!(matches, "attestation was issued for a different repository");
		let signer = claims
			.build_signer_uri
			.as_deref()
			.or(identity)
			.context("attestation lacks a workflow identity")?;
		let signer = signer.to_ascii_lowercase();
		// The pipeline's signed statement is stricter, so it wins where both match.
		let signer = if signer.starts_with(PIPELINE_SIGNER) {
			Signer::Pipeline
		} else if signer.starts_with(&format!("{expected}/.github/workflows/")) {
			Signer::Author
		} else {
			bail!("attestation signer is neither the author's workflow nor the crabx pipeline");
		};
		let reference = claims
			.source_repository_ref
			.as_deref()
			.or(claims.deprecated_github.workflow_ref.as_deref())
			.context("attestation lacks a source ref")?;
		if let Some(tag) = reference.strip_prefix("refs/tags/") {
			self.tag(tag)?;
		} else if reference.starts_with("refs/heads/") {
			// A branch names no release, and binstalk's templates may reuse one asset name across
			// versions, so only the pipeline's signed crate and version can bind a branch build.
			ensure!(
				signer == Signer::Pipeline,
				"attestation from branch {reference:?} is not signed by the crabx pipeline"
			);
		} else {
			bail!("attestation was issued from an untrusted ref {reference:?}");
		}
		Ok(signer)
	}

	/// Require the pipeline's signed SLSA statement to name this crate and version, built on a
	/// GitHub-hosted runner, since its one signature covers every archive of the run.
	fn check_build(&self, statement: &[u8]) -> Result<()> {
		#[derive(Deserialize)]
		struct Statement {
			#[serde(rename = "predicateType")]
			kind: String,
			predicate: Predicate,
		}
		#[derive(Deserialize)]
		struct Predicate {
			#[serde(rename = "buildDefinition")]
			build: Build,
		}
		#[derive(Deserialize)]
		struct Build {
			#[serde(rename = "externalParameters")]
			external: External,
			#[serde(rename = "internalParameters")]
			internal: Internal,
		}
		#[derive(Deserialize)]
		struct External {
			#[serde(rename = "crate")]
			name: String,
			version: String,
		}
		#[derive(Deserialize)]
		struct Internal {
			github: Github,
		}
		#[derive(Deserialize)]
		struct Github {
			runner_environment: String,
		}
		let statement: Statement = serde_json::from_slice(statement).context("malformed pipeline attestation")?;
		ensure!(
			statement.kind == "https://slsa.dev/provenance/v1",
			"pipeline attestation is not SLSA provenance"
		);
		let Build { external, internal } = statement.predicate.build;
		ensure!(
			external.name.eq_ignore_ascii_case(self.name),
			"pipeline attestation is for crate {:?}, not {:?}",
			external.name,
			self.name
		);
		ensure!(
			Version::parse(&external.version).is_ok_and(|version| &version == self.version),
			"pipeline attestation is for version {:?}, not {}",
			external.version,
			self.version
		);
		ensure!(
			internal.github.runner_environment == "github-hosted",
			"pipeline attestation was built on a {:?} runner, not a GitHub-hosted one",
			internal.github.runner_environment
		);
		Ok(())
	}

	fn tag(&self, tag: &str) -> Result<()> {
		let asset_tag = self
			.asset
			.path()
			.split("/releases/download/")
			.nth(1)
			.and_then(|v| v.split('/').next())
			.map(|v| percent_encoding::percent_decode_str(v).decode_utf8())
			.transpose()?;
		ensure!(
			asset_tag.as_deref() == Some(tag) || tag == self.version.to_string() || tag == format!("v{}", self.version),
			"attestation is for another release tag"
		);
		Ok(())
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn external_bundles_decode_raw_snappy_and_reject_corruption() {
		let temp = tempfile::NamedTempFile::new().unwrap();
		let json = include_bytes!("../tests/data/github-attestation.json");
		let compressed = snap::raw::Encoder::new().compress_vec(json).unwrap();
		std::fs::write(temp.path(), compressed).unwrap();
		external_bundle(temp.path()).unwrap();
		std::fs::write(temp.path(), b"invalid snappy").unwrap();
		assert!(external_bundle(temp.path()).is_err());
	}

	#[test]
	fn an_attestation_page_holds_at_most_one_attestation() {
		let page = |json: &str| serde_json::from_str::<Page>(json);
		assert!(page(r#"{"attestations":[]}"#).unwrap().attestations.is_none());
		let bundle = include_str!("../tests/data/attestation.json");
		let one = page(&format!(r#"{{"attestations":[{{"bundle":{bundle}}}]}}"#)).unwrap();
		assert!(one.attestations.unwrap().bundle.is_some());
		let external = page(r#"{"attestations":[{"bundle":null,"bundle_url":"https://example.com/b"}]}"#).unwrap();
		assert!(external.attestations.unwrap().bundle_url.is_some());
		// A server that ignores per_page=1, or omits the list, is refused rather than truncated.
		let error = page(r#"{"attestations":[{},{}]}"#).err().unwrap();
		assert!(error.to_string().contains("ignored the attestation page size"));
		assert!(page("{}").is_err());
		assert!(page(r#"{"attestations":null}"#).is_err());
	}

	#[test]
	fn release_attestations_need_an_in_toto_payload() {
		let json = include_str!("../tests/data/github-attestation.json");
		assert!(release(&Bundle::from_json(json).unwrap()).unwrap().is_some());
		let mut changed: serde_json::Value = serde_json::from_str(json).unwrap();
		changed["dsseEnvelope"]["payloadType"] = "text/plain".into();
		let changed = Bundle::from_json(&serde_json::to_string(&changed).unwrap()).unwrap();
		assert!(release(&changed).is_err());
		// Another predicate, such as a build provenance, is no release attestation.
		assert!(
			release(&Bundle::from_json(include_str!("../tests/data/attestation.json")).unwrap())
				.unwrap()
				.is_none()
		);
	}

	#[test]
	fn github_cursor_pagination_preserves_the_cursor_and_api_boundary() {
		let current =
			reqwest::Url::parse("https://api.github.com/repos/a/b/attestations/sha256:abc?per_page=1").unwrap();
		let target = "https://api.github.com/repositories/192136193/attestations/sha256:abc?per_page=1&after=58893410";
		let link = format!("<{target}>; rel=\"next\", <https://docs.github.com/api>; rel=\"deprecation\"");
		assert_eq!(
			next_page(Some(&link), &current, "a/b", "abc")
				.unwrap()
				.unwrap()
				.as_str(),
			target
		);
		assert!(next_page(None, &current, "a/b", "abc").unwrap().is_none());
		for bad in [
			target.replace("api.github.com", "evil.example"),
			target.replace("https:", "http:"),
			target.replace("api.github.com", "user@api.github.com"),
			target.replace("sha256:abc", "sha256:def"),
			target.replace("per_page=1", "per_page=100"),
			target.replace("192136193", "a/b"),
			current.to_string(),
		] {
			assert!(
				next_page(Some(&format!("<{bad}>; rel=\"next\"")), &current, "a/b", "abc").is_err(),
				"{bad}"
			);
		}
	}

	#[test]
	fn github_release_checks_tsa_digest_repository_tag_and_trust_domain() {
		let root = TrustedRoot::from_embedded(sigstore_verify::trust_root::SigstoreInstance::GitHub).unwrap();
		let verifier = Verifier::new(&root).unwrap();
		let json = include_str!("../tests/data/github-attestation.json");
		let bundle = Bundle::from_json(json).unwrap();
		let version = Version::new(0, 1, 9);
		let asset =
			reqwest::Url::parse("https://github.com/jdx/communique/releases/download/v0.1.9/asset.tgz").unwrap();
		let artifact = Artifact {
			digest: "76f1fe8593bf227cca2c089e3c16dc95014a8d3e89c5dd220530469ca043c428",
			repo: "jdx/communique",
			name: "tool",
			version: &version,
			asset: &asset,
		};
		verify_bundle(&verifier, &bundle, artifact).unwrap();
		assert!(
			verify_bundle(
				&verifier,
				&bundle,
				Artifact {
					repo: "other/repo",
					..artifact
				}
			)
			.is_err()
		);
		assert!(
			verify_bundle(
				&verifier,
				&bundle,
				Artifact {
					digest: &"00".repeat(32),
					..artifact
				}
			)
			.is_err()
		);
		let other_version = Version::new(9, 9, 9);
		let other_asset =
			reqwest::Url::parse("https://github.com/jdx/communique/releases/download/v9.9.9/asset.tgz").unwrap();
		assert!(
			verify_bundle(
				&verifier,
				&bundle,
				Artifact {
					version: &other_version,
					asset: &other_asset,
					..artifact
				}
			)
			.is_err()
		);
		let public = TrustedRoot::from_json(include_str!("../tests/data/trusted-root.json")).unwrap();
		assert!(verify_bundle(&Verifier::new(&public).unwrap(), &bundle, artifact).is_err());
		let mut changed: serde_json::Value = serde_json::from_str(json).unwrap();
		let token = &mut changed["verificationMaterial"]["timestampVerificationData"]["rfc3161Timestamps"][0]["signedTimestamp"];
		let mut signature = token.as_str().unwrap().to_owned();
		let index = signature.len() - 8;
		let replacement = if signature.as_bytes()[index] == b'A' { "B" } else { "A" };
		signature.replace_range(index..index + 1, replacement);
		*token = signature.into();
		let changed = Bundle::from_json(&serde_json::to_string(&changed).unwrap()).unwrap();
		assert!(verify_bundle(&verifier, &changed, artifact).is_err());
	}

	#[test]
	fn identity_is_bound_to_the_repository() {
		let mut claims = sigstore_verify::crypto::FulcioCiClaims::default();
		claims.source_repository_uri = Some("https://github.com/a/b".into());
		claims.build_signer_uri = Some("https://github.com/a/b/.github/workflows/release.yml@refs/tags/v1.2.3".into());
		claims.source_repository_ref = Some("refs/tags/v1.2.3".into());
		let asset = reqwest::Url::parse("https://github.com/a/b/releases/download/v1.2.3/asset.tgz").unwrap();
		let version = Version::new(1, 2, 3);
		assert!(
			Artifact {
				digest: "",
				repo: "a/b",
				name: "tool",
				version: &version,
				asset: &asset
			}
			.authorize(&claims, None)
			.is_ok()
		);
		assert!(
			Artifact {
				digest: "",
				repo: "a/evil",
				name: "tool",
				version: &version,
				asset: &asset
			}
			.authorize(&claims, None)
			.is_err()
		);
		claims.source_repository_ref = Some("refs/pull/1/merge".into());
		assert!(
			Artifact {
				digest: "",
				repo: "a/b",
				name: "tool",
				version: &version,
				asset: &asset
			}
			.authorize(&claims, None)
			.is_err()
		);
	}

	#[test]
	fn only_the_crabx_pipeline_signs_for_other_repositories() {
		let mut claims = sigstore_verify::crypto::FulcioCiClaims::default();
		claims.source_repository_uri = Some("https://github.com/a/b".into());
		claims.source_repository_ref = Some("refs/tags/v1.2.3".into());
		let asset = reqwest::Url::parse("https://github.com/a/b/releases/download/v1.2.3/asset.tgz").unwrap();
		let version = Version::new(1, 2, 3);
		let artifact = Artifact {
			digest: "",
			repo: "a/b",
			name: "tool",
			version: &version,
			asset: &asset,
		};
		let mut signs = |signer: &str| {
			claims.build_signer_uri = Some(signer.into());
			artifact.authorize(&claims, None).is_ok()
		};
		assert!(signs(
			"https://github.com/kixel-ai/crabx/.github/workflows/crabx.yml@0123456789abcdef0123456789abcdef01234567"
		));
		assert!(signs(
			"https://github.com/Kixel-AI/Crabx/.github/workflows/crabx.yml@refs/tags/pipeline-v1.0.0"
		));
		// The pipeline's old home, the repository's other workflows, and a lookalike name.
		assert!(!signs(
			"https://github.com/kixel-ai/rust2/.github/workflows/crabx.yml@refs/tags/pipeline-v1.0.0"
		));
		assert!(!signs(
			"https://github.com/kixel-ai/crabx/.github/workflows/release.yml@refs/heads/main"
		));
		assert!(!signs(
			"https://github.com/kixel-ai/crabx/.github/workflows/crabx.yml.evil@refs/heads/main"
		));
		assert!(!signs(
			"https://github.com/kixel-ai/crabx-evil/.github/workflows/crabx.yml@refs/heads/main"
		));
	}

	#[test]
	fn legacy_fulcio_claims_still_bind_repository_workflow_and_ref() {
		let mut claims = sigstore_verify::crypto::FulcioCiClaims::default();
		claims.deprecated_github.workflow_repository = Some("a/b".into());
		claims.deprecated_github.workflow_ref = Some("refs/tags/v1.2.3".into());
		let version = Version::new(1, 2, 3);
		let asset = reqwest::Url::parse("https://github.com/a/b/releases/download/v1.2.3/asset.tgz").unwrap();
		let artifact = Artifact {
			digest: "",
			repo: "a/b",
			name: "tool",
			version: &version,
			asset: &asset,
		};
		let identity = Some("https://github.com/a/b/.github/workflows/release.yml@refs/tags/v1.2.3");
		artifact.authorize(&claims, identity).unwrap();
		assert!(artifact.authorize(&claims, None).is_err());
		assert!(
			artifact
				.authorize(
					&claims,
					Some("https://github.com/evil/fork/.github/workflows/release.yml")
				)
				.is_err()
		);
		claims.deprecated_github.workflow_repository = Some("evil/fork".into());
		assert!(artifact.authorize(&claims, identity).is_err());
		claims.deprecated_github.workflow_repository = Some("a/b".into());
		claims.deprecated_github.workflow_ref = Some("refs/pull/1/merge".into());
		assert!(artifact.authorize(&claims, identity).is_err());
	}

	#[test]
	fn real_attestation_rejects_changed_digest_identity_signature_and_log() {
		let root = TrustedRoot::from_json(include_str!("../tests/data/trusted-root.json")).unwrap();
		let verifier = Verifier::new(&root).unwrap();
		let json = include_str!("../tests/data/pipeline-attestation.json");
		let bundle = Bundle::from_json(json).unwrap();
		// crabx 0.1.0's Windows archive, signed from a branch by the pipeline.
		let digest = "ad37ee971cde7a8ada1a29ee78660e4adeaab0e9cd768aa1052dc9e60174d23e";
		let version = Version::new(0, 1, 0);
		let url = reqwest::Url::parse(
			"https://github.com/kixel-ai/crabx/releases/download/crabx-v0.1.0/crabx-x86_64-pc-windows-msvc-v0.1.0.tgz",
		)
		.unwrap();
		let artifact = Artifact {
			digest,
			repo: "kixel-ai/crabx",
			name: "crabx",
			version: &version,
			asset: &url,
		};
		verify_bundle(&verifier, &bundle, artifact).unwrap();
		let zero = "00".repeat(32);
		for wrong in [
			Artifact {
				digest: &zero,
				..artifact
			},
			Artifact {
				repo: "evil/fork",
				..artifact
			},
		] {
			assert!(verify_bundle(&verifier, &bundle, wrong).is_err());
		}
		let mut tampered: serde_json::Value = serde_json::from_str(json).unwrap();
		tampered["dsseEnvelope"]["signatures"][0]["sig"] = "AAAA".into();
		let tampered = Bundle::from_json(&serde_json::to_string(&tampered).unwrap()).unwrap();
		assert!(verify_bundle(&verifier, &tampered, artifact).is_err());
		let mut tampered: serde_json::Value = serde_json::from_str(json).unwrap();
		tampered["verificationMaterial"]["tlogEntries"][0]["integratedTime"] = "1".into();
		let tampered = Bundle::from_json(&serde_json::to_string(&tampered).unwrap()).unwrap();
		assert!(verify_bundle(&verifier, &tampered, artifact).is_err());
	}

	#[test]
	fn a_valid_author_attestation_from_a_branch_is_refused() {
		let root = TrustedRoot::from_json(include_str!("../tests/data/trusted-root.json")).unwrap();
		let verifier = Verifier::new(&root).unwrap();
		let bundle = Bundle::from_json(include_str!("../tests/data/attestation.json")).unwrap();
		let version = Version::new(1, 2, 3);
		let url =
			reqwest::Url::parse("https://github.com/prefix-dev/sigstore-example/releases/download/v1.2.3/asset.tgz")
				.unwrap();
		// The signature, log, and repository are valid; only a pipeline's signed version binds a branch build.
		let error = verify_bundle(
			&verifier,
			&bundle,
			Artifact {
				digest: "54303491a8418fbed24344b513546182c29b43bf282ceb433af65e2299f9271f",
				repo: "prefix-dev/sigstore-example",
				name: "sigstore-example",
				version: &version,
				asset: &url,
			},
		)
		.unwrap_err();
		assert!(
			error.to_string().contains("not signed by the crabx pipeline"),
			"{error}"
		);
	}

	#[test]
	fn branch_builds_are_bound_only_by_the_pipelines_signed_crate_and_version() {
		let mut claims = sigstore_verify::crypto::FulcioCiClaims::default();
		claims.source_repository_uri = Some("https://github.com/a/b".into());
		claims.source_repository_ref = Some("refs/heads/main".into());
		// binstalk's templates may reuse one asset name for every release tag.
		let asset = reqwest::Url::parse("https://github.com/a/b/releases/download/v9.9.9/tool-x86_64.tgz").unwrap();
		let version = Version::new(1, 2, 3);
		let artifact = Artifact {
			digest: "",
			repo: "a/b",
			name: "tool",
			version: &version,
			asset: &asset,
		};
		claims.build_signer_uri = Some("https://github.com/a/b/.github/workflows/release.yml@refs/heads/main".into());
		let error = artifact.authorize(&claims, None).unwrap_err().to_string();
		assert!(error.contains("not signed by the crabx pipeline"), "{error}");
		claims.build_signer_uri = Some(format!("{PIPELINE_SIGNER}refs/tags/pipeline-v1.0.0"));
		assert_eq!(artifact.authorize(&claims, None).unwrap(), Signer::Pipeline);
		// A tag still has to match the release, whoever signed it.
		claims.source_repository_ref = Some("refs/tags/v9.9.9".into());
		assert_eq!(artifact.authorize(&claims, None).unwrap(), Signer::Pipeline);
		claims.source_repository_ref = Some("refs/tags/v1.0.0".into());
		assert!(artifact.authorize(&claims, None).is_err());
		claims.build_signer_uri = Some("https://github.com/a/b/.github/workflows/release.yml@refs/tags/v9.9.9".into());
		claims.source_repository_ref = Some("refs/tags/v9.9.9".into());
		assert_eq!(artifact.authorize(&claims, None).unwrap(), Signer::Author);
	}

	#[test]
	fn the_pipelines_signed_statement_names_the_crate_version_and_runner() {
		let bundle = Bundle::from_json(include_str!("../tests/data/pipeline-attestation.json")).unwrap();
		let real = statement(&bundle).unwrap();
		let version = Version::new(0, 1, 0);
		let asset =
			reqwest::Url::parse("https://github.com/kixel-ai/crabx/releases/download/crabx-v0.1.0/a.tgz").unwrap();
		let artifact = Artifact {
			digest: "",
			repo: "kixel-ai/crabx",
			name: "crabx",
			version: &version,
			asset: &asset,
		};
		artifact.check_build(real).unwrap();
		let with = |edit: &dyn Fn(&mut serde_json::Value)| {
			let mut value: serde_json::Value = serde_json::from_slice(real).unwrap();
			edit(&mut value);
			serde_json::to_vec(&value).unwrap()
		};
		let external = |key: &'static str, to: &'static str| {
			with(&move |v| v["predicate"]["buildDefinition"]["externalParameters"][key] = to.into())
		};
		artifact.check_build(&external("crate", "CrabX")).unwrap();
		for (changed, expect) in [
			(external("crate", "other"), "for crate"),
			(external("crate", "crabx-evil"), "for crate"),
			(external("version", "0.1.1"), "for version"),
			(external("version", "0.1.0+build"), "for version"),
			(external("version", "not a version"), "for version"),
			(
				with(&|v| {
					v["predicate"]["buildDefinition"]["internalParameters"]["github"]["runner_environment"] =
						"self-hosted".into()
				}),
				"not a GitHub-hosted one",
			),
			(
				with(&|v| {
					v["predicate"]["buildDefinition"]["internalParameters"]["github"]["runner_environment"] =
						serde_json::Value::Null
				}),
				"malformed",
			),
			(
				with(&|v| v["predicate"]["buildDefinition"]["internalParameters"] = serde_json::json!({})),
				"malformed",
			),
			(
				with(&|v| v["predicateType"] = "https://example.com/other".into()),
				"not SLSA provenance",
			),
			(with(&|v| v["predicate"] = serde_json::json!({})), "malformed"),
			(b"[]".to_vec(), "malformed"),
		] {
			let error = format!("{:#}", artifact.check_build(&changed).unwrap_err());
			assert!(error.contains(expect), "{error}");
		}
	}

	#[test]
	fn a_real_pipeline_attestation_binds_the_crate_and_version() {
		let root = TrustedRoot::from_json(include_str!("../tests/data/trusted-root.json")).unwrap();
		let verifier = Verifier::new(&root).unwrap();
		let bundle = Bundle::from_json(include_str!("../tests/data/pipeline-attestation.json")).unwrap();
		// crabx 0.1.0's Windows archive, signed from a branch by the pipeline.
		let digest = "ad37ee971cde7a8ada1a29ee78660e4adeaab0e9cd768aa1052dc9e60174d23e";
		let version = Version::new(0, 1, 0);
		let asset = reqwest::Url::parse(
			"https://github.com/kixel-ai/crabx/releases/download/crabx-v0.1.0/crabx-x86_64-pc-windows-msvc-v0.1.0.tgz",
		)
		.unwrap();
		let artifact = Artifact {
			digest,
			repo: "kixel-ai/crabx",
			name: "crabx",
			version: &version,
			asset: &asset,
		};
		verify_bundle(&verifier, &bundle, artifact).unwrap();
		let other = Version::new(0, 1, 1);
		for wrong in [
			Artifact {
				name: "other",
				..artifact
			},
			Artifact {
				version: &other,
				..artifact
			},
			Artifact {
				repo: "kixel-ai/other",
				..artifact
			},
		] {
			assert!(verify_bundle(&verifier, &bundle, wrong).is_err());
		}
	}
}
