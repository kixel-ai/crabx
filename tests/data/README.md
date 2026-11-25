The attestation and trust root are Sigstore's real conda provenance fixture and its
pinned public-good trust root, from sigstore/sigstore-rust 0.16.0:

- `crates/sigstore-verify/test_data/bundles/conda-attestation.sigstore.json`
- `crates/sigstore-verify/test_data/trusted_roots/public-good.json`
- `crates/sigstore-verify/test_data/bundles/github-private-attestation.sigstore.json`
  (GitHub's timestamp-only release attestation, with the library's embedded GitHub root)

Source: https://github.com/sigstore/sigstore-rust/tree/55227ff848a69d2d81ac36607d6fcfa25b97553a,
Apache-2.0.
The test changes its artifact digest, repository, signature, and transparency-log time
independently and requires each change to fail verification.
The GitHub fixture also checks the RFC 3161 signature, signed release tag, and
rejection under the public-good trust domain.

`pipeline-attestation.json` is the real SLSA provenance the crabx pipeline attested for
crabx 0.1.0's archives from `main`, fetched from GitHub's attestations API for
`sha256:ad37ee971cde7a8ada1a29ee78660e4adeaab0e9cd768aa1052dc9e60174d23e`
(`crabx-x86_64-pc-windows-msvc-v0.1.0.tgz`), under the public-good trust root above.
It checks the signed crate, version, and runner binding.
