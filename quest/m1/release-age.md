# [M] Wait 24 hours before running a new release

## Goal

An unpinned `crabx <crate>`, and an unpinned launcher from `crabx install
<crate>`, runs the newest stable, non-yanked version published at least 24
hours ago, by the `pubtime` the crates.io index records. A pinned
`crate@version` ignores the delay. `--minimum-release-age <duration>` or
`CRABX_MINIMUM_RELEASE_AGE` changes the delay, and `0s` turns it off.

## Plan

Decided 2026-11-24 (maintainer, /quest-plan in kixel-ai/token-max): ✅ a 24h
default with a flag and an environment variable to override it, like mise's
`minimum_release_age`, which has defaulted to 24h since v2026.6.2 (rejected:
7 days, which makes every fix wait a week; 24h with no override). A hijacked
release is usually yanked within hours, so a day's delay keeps most of them
from ever running.

Recommendations:

- Apply it on both paths that pick a version: the background refresh in
  `network.rs`, which already filters yanked and prerelease versions, and the
  cold first resolve in `install.rs`, which today asks binstalk for any
  version (`VersionReq::STAR`).
- crates.io records `pubtime` for every version, back to 2014's (checked
  2026-11-24 on `serde` and `token-max`). A version without one is an error
  naming the crate, not "old enough", and it applies only to unpinned runs.
- If a crate has no version old enough (one published less than a day ago),
  fail with a message naming the version and when it becomes eligible, and
  suggest pinning it or `--minimum-release-age 0s`.
- Accept mise's duration forms (`24h`, `7d`) and reject anything else. The
  flag goes before the crate, like crabx's other options.
- The README says which versions an unpinned run picks, and why.
- Unit tests set the clock rather than reading it.
