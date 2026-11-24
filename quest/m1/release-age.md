# [M] Wait 24 hours before running a new release, and check daily

## Goal

An unpinned `crabx <crate>`, and an unpinned launcher from `crabx install
<crate>`, runs the newest stable, non-yanked version published at least 24
hours ago, by the `pubtime` the crates.io index records. A pinned
`crate@version` ignores the delay. `--minimum-release-age <duration>` or
`CRABX_MINIMUM_RELEASE_AGE` changes the delay, and `0s` turns it off.
An unpinned run still runs the cached version at once and refreshes in the
background, but checks the index for a crate at most once a day.

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

Recommendations:

- Check the version an unpinned run is about to execute, not only the one
  the refresh picks: the cached `latest` pointer can hold a version too new
  for the current policy, from 0.1.0's refresh or a `0s` run. Record each
  entry's `pubtime`, and before executing, fall back to the newest cached
  version the policy allows, or resolve one.
- Pick the version before branching on `--build`, so a cold `--build` passes
  Cargo an exact eligible version instead of letting it choose the newest.
- Pass the effective policy to the detached refresh explicitly; it is
  rebuilt from a command line that today drops options such as this one.
- Recompute eligibility on every refresh, a `304` included, from the index
  data already cached, since time passing alone makes a version eligible.
- Keep the schedule out of the existing refresh state file: 0.1.0 rejects
  unknown fields there, so a rollback to it on the same cache would break
  every unpinned run. Use a separate file.
- Store the last check's time in the refresh state and skip the refresh
  until a day has passed. Also refresh as soon as a version the last check
  saw, but which was too new, reaches the delay, so that version doesn't wait
  another full day. That check is a deliberate exception to the daily limit,
  and it fetches the index again so it sees a yank made in the meantime.
- Keep the failure backoff as it is: a failed refresh retries after 30
  seconds, doubling to 30 minutes, and doesn't wait a day.

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
