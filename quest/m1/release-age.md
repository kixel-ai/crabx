# [M] Wait 24 hours before running a new release, and check daily

## Goal

An unpinned `crabx <crate>`, and an unpinned launcher from `crabx install
<crate>`, runs the newest stable, non-yanked version published at least 24
hours ago, by the `pubtime` the crates.io index records. A pinned
`crate@version` ignores the delay. `--minimum-release-age <duration>` or
`CRABX_MINIMUM_RELEASE_AGE` changes the delay, and `0s` turns it off.
An unpinned run still runs the cached version at once and refreshes in the
background, but runs its freshness check, the index fetch that looks for new
versions, at most once a day per crate.

## Plan

Decided 2026-11-24 (maintainer, /quest-plan in kixel-ai/token-max): ✅ a 24h
default with a flag and an environment variable to override it, like mise's
`minimum_release_age`, which has defaulted to 24h since v2026.6.2 (rejected:
7 days, which makes every fix wait a week; 24h with no override). A hijacked
release is usually yanked within hours, so a day's delay keeps most of them
from ever running.

Decided 2026-11-24 (maintainer, /quest-plan in kixel-ai/token-max): ✅ the
background refresh checks at most once a day per crate and trust policy,
still stale-while-revalidate (rejected: keeping a check on every unpinned
run). This supersedes the root quest's 2026-11-12 rejection of a 24-hour
TTL: with the release-age delay a release already arrives a day late, and a
daily check spares crates.io a request on most runs. Today that request goes
to index.crates.io, a CDN; GitHub is contacted only to install a new version.

Decided 2026-11-24 (maintainer, at the review checkpoint of
[kixel-ai/crabx#27](https://github.com/kixel-ai/crabx/pull/27)): ✅ every
unpinned run picks its version from the cached index, the current delay and
the current time, and the daily freshness check is the only index fetch
before a version is chosen (rejected:
extra fetches when a release turns eligible or when a longer delay applies,
whose timing kept raising review findings). A release is eligible once it
is a day old. On a warm cache, the cached index may not list it until the
next freshness check, up to a day later; a version already installed then
runs at once, and one that isn't installs in the background for a later
run, so a new release typically first runs 24 to 48 hours after
publishing. A cold run selects, installs and runs an eligible release in
one go.

Invariants the implementation keeps, each with a test that sets the clock
rather than reading it:

- A warm unpinned run executes the newest installed version that the
  current policy allows, at the moment it runs, by the newest index data
  crabx holds; the background refresh installs the newest eligible version
  for the next run. Each run evaluates the cached index with its own delay,
  and if that names a newer eligible version than any installed, starts that
  background install even when no index fetch is due. The cached `latest` pointer is a hint, not the answer,
  since 0.1.0's refresh, a `0s` run or a `7d` run may have set it.
- A cold run picks an exact eligible version before installing, on both
  paths: the first resolve (today binstalk's `VersionReq::STAR` in
  `install.rs`) and `--build`, which passes Cargo that version instead of
  letting it choose.
- A version runs only if the newest index data crabx holds says it isn't
  yanked. Nothing fetches early to catch a yank or a release turning
  eligible: the daily freshness check sees both within a day, and a run waits on the
  network only when nothing cached is eligible.
- The freshness check, the index fetch that looks for new versions, runs at
  most once a day per crate, in the background, and
  a failed fetch backs off as today (30 seconds, doubling to 30 minutes). A
  conditional request is sent only when the index data its validator stands
  for is cached, which 0.1.0's refresh state doesn't hold. Installing a
  chosen version makes whatever requests binstalk or Cargo need, such as
  `fetch_crate_matched`'s own index read; the README says so.
- The detached refresh gets the effective policy explicitly.
- 0.1.0 still runs on a cache 0.2.0 has used: new state goes in new files,
  since 0.1.0 rejects unknown fields in its refresh state.

Recommendations:

- crates.io records `pubtime` for every version, back to 2014's (checked
  2026-11-24 on `serde` and `token-max`). A version without one is an error
  naming the crate, not "old enough", and it applies only to unpinned runs.
- If a crate has no version old enough, fail with a message naming the
  newest version and when it becomes eligible, and suggest pinning it or
  `--minimum-release-age 0s`.
- Accept mise's duration forms (`24h`, `7d`) and reject anything else. The
  flag goes before the crate, like crabx's other options.
- The README says which versions an unpinned run picks, and why.
