"""Build crates.io releases and publish binstall-compatible archives."""

import datetime
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import stat
import subprocess
import sys
import tarfile
import tomllib
import urllib.error
import urllib.parse
import urllib.request
import zipfile

TARGETS = [
    ("x86_64-unknown-linux-gnu", "ubuntu-24.04"),
    ("aarch64-unknown-linux-gnu", "ubuntu-24.04-arm"),
    ("x86_64-apple-darwin", "macos-15"),
    ("aarch64-apple-darwin", "macos-15"),
    ("x86_64-pc-windows-msvc", "windows-latest"),
    ("aarch64-pc-windows-msvc", "windows-11-arm"),
]

# What the runner gives a step to reach its artifact, cache, OIDC, and step-output
# APIs. A crate's build script or proc-macro runs without them.
RUNNER_ENV = {
    "ACTIONS_RUNTIME_TOKEN", "ACTIONS_RUNTIME_URL", "ACTIONS_RESULTS_URL", "ACTIONS_CACHE_URL", "ACTIONS_CACHE_SERVICE_V2",
    "ACTIONS_ID_TOKEN_REQUEST_TOKEN", "ACTIONS_ID_TOKEN_REQUEST_URL",
    "GITHUB_OUTPUT", "GITHUB_ENV", "GITHUB_PATH", "GITHUB_STATE", "GITHUB_STEP_SUMMARY",
}
# GitHub's largest release asset; no verified archive is staged past it.
MAX_ASSET = 2 * 1024**3


def run(*args, cwd=None, env=None):
    return subprocess.check_output(args, cwd=cwd, env=env, text=True).strip()


def request(url, data=None, missing=False):
    headers = {"User-Agent": "crabx-release-pipeline (github.com/kixel-ai/crabx)"}
    if url.startswith("https://api.github.com/"):
        headers["Authorization"] = "Bearer " + os.environ["GH_TOKEN"]
        headers["Accept"] = "application/vnd.github+json"
    body = None if data is None else json.dumps(data).encode()
    if body is not None:
        headers["Content-Type"] = "application/json"
    try:
        with urllib.request.urlopen(urllib.request.Request(url, body, headers), timeout=60) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        if missing and error.code == 404:
            return None
        raise


def api(path, **kwargs):
    return request("https://api.github.com/repos/" + os.environ["GITHUB_REPOSITORY"] + "/" + path, **kwargs)


def safe(value):
    if not re.fullmatch(r"[A-Za-z0-9_.+-]+", value) or value in (".", ".."):
        raise ValueError(f"unsafe name: {value!r}")
    return value


def asset(item, target):
    return f"{safe(item['name'])}-{target}-v{safe(item['version'])}.tgz"


def artifact(item, target):
    """Name both a target's build job and the artifact it uploads."""
    return f"crabx-{safe(item['name'])}@{safe(item['version'])}@{target}"


def targets(item):
    return item.get("targets", [t for t, _ in TARGETS])


def tag(item):
    # A crate's own tag must name its published commit. A bare version tag on another
    # commit belongs to another crate of the workspace, or to none, so the crate's own
    # tag is used instead.
    found = []
    for name in (f"{item['name']}-v{item['version']}", f"v{item['version']}"):
        ref = api("git/ref/tags/" + urllib.parse.quote(name, safe=""), missing=True)
        if ref is None:
            continue
        obj = ref["object"]
        seen = set()
        while obj["type"] == "tag":
            if obj["sha"] in seen:
                raise ValueError("cyclic annotated tag")
            seen.add(obj["sha"])
            obj = api("git/tags/" + obj["sha"])["object"]
        if obj["type"] != "commit" or obj["sha"] != item["sha"]:
            if name == f"v{item['version']}":
                continue
            raise ValueError(f"{name} does not point to published commit {item['sha']}")
        found.append(name)
    return found[0] if found else f"{item['name']}-v{item['version']}"


def same_repo(url, repo):
    return (url or "").removesuffix("/").removesuffix(".git").lower() == repo.lower()


def published(name, version, repo):
    # Stream the archive, never extract paths supplied by the registry.
    url = f"https://static.crates.io/crates/{name}/{name}-{version}.crate"
    req = urllib.request.Request(url, headers={"User-Agent": "crabx-release-pipeline"})
    wanted = {f"{name}-{version}/Cargo.toml", f"{name}-{version}/.cargo_vcs_info.json"}
    files = {}
    implicit_bin = False
    with urllib.request.urlopen(req, timeout=60) as response, tarfile.open(fileobj=response, mode="r|gz") as archive:
        for member in archive:
            relative = member.name.removeprefix(f"{name}-{version}/")
            if member.isfile() and (relative == "src/main.rs" or (relative.startswith("src/bin/") and (relative.count("/") == 2 and relative.endswith(".rs") or relative.count("/") == 3 and relative.endswith("/main.rs")))):
                implicit_bin = True
            if member.name in wanted:
                if not member.isfile() or member.name in files:
                    raise ValueError("invalid or duplicate crate metadata")
                files[member.name] = archive.extractfile(member).read()
    manifest = tomllib.loads(files[f"{name}-{version}/Cargo.toml"].decode())
    if not manifest.get("bin") and implicit_bin and manifest["package"].get("autobins", True):
        manifest["bin"] = [{"name": name}]
    # Only a release of this repository's binaries needs its published source.
    if not manifest.get("bin"):
        return None
    if not same_repo(manifest["package"].get("repository"), repo):
        print(f"{name}@{version}: its published manifest names another repository", file=sys.stderr)
        return None
    if f"{name}-{version}/.cargo_vcs_info.json" not in files:
        raise ValueError(f"{name}@{version}: published without its git commit; yank it to skip it")
    vcs = json.loads(files[f"{name}-{version}/.cargo_vcs_info.json"])
    if vcs["git"].get("dirty", False) or not re.fullmatch(r"[0-9a-f]{40}", vcs["git"]["sha1"]):
        raise ValueError(f"{name}@{version}: dirty or invalid published commit; yank it to skip it")
    path = PurePosixPath(vcs.get("path_in_vcs", ""))
    if path.is_absolute() or ".." in path.parts or "\\" in str(path):
        raise ValueError("invalid published crate path")
    if manifest["package"]["name"] != name or manifest["package"]["version"] != version:
        raise ValueError("registry archive identity mismatch")
    return {"name": safe(name), "version": safe(version), "sha": vcs["git"]["sha1"], "path": str(path)}


def discover():
    workflow = os.environ["CALLER_WORKFLOW"].split("/.github/", 1)[1].split("@", 1)[0]
    added = run("git", "log", "--follow", "--diff-filter=A", "--format=%cI", "--", ".github/" + workflow).splitlines()
    if not added:
        raise ValueError("cannot find when caller workflow was added")
    since = datetime.datetime.fromisoformat(added[-1])
    names = set()
    for path in run("git", "ls-files", "**/Cargo.toml", "Cargo.toml").splitlines():
        package = tomllib.loads(Path(path).read_text()).get("package", {})
        # A name crates.io cannot hold, such as a template's, is no release to look up.
        if package and package.get("publish") != False and re.fullmatch(r"[A-Za-z][A-Za-z0-9_-]*", package.get("name", "")):
            names.add(package["name"])
    items = []
    repo = "https://github.com/" + os.environ["GITHUB_REPOSITORY"]
    for name in sorted(names):
        info = request(f"https://crates.io/api/v1/crates/{name}", missing=True)
        if info is None or not same_repo(info["crate"].get("repository"), repo):
            print(f"{name}: no crates.io crate names {repo} as its repository", file=sys.stderr)
            continue
        for version in info["versions"]:
            if version["yanked"] or datetime.datetime.fromisoformat(version["created_at"]) < since:
                continue
            item = published(name, safe(version["num"]), repo)
            if item is None:
                continue
            item["tag"] = tag(item)
            release = api("releases/tags/" + urllib.parse.quote(item["tag"], safe=""), missing=True)
            item["targets"] = [target for target, _ in TARGETS]
            if release is not None:
                assets = run("gh", "api", "--paginate", f"repos/{os.environ['GITHUB_REPOSITORY']}/releases/{release['id']}/assets", "--jq", ".[].name").splitlines()
                item["targets"] = [target for target, _ in TARGETS if asset(item, target) not in assets]
                if not item["targets"]:
                    continue
            items.append(item)
    return items


def build(item, target):
    safe(item["name"])
    safe(item["version"])
    manifest = str((Path("source") / item["path"] / "Cargo.toml").resolve())
    env = {key: value for key, value in os.environ.items() if key not in RUNNER_ENV}
    metadata = json.loads(run("cargo", "metadata", "--locked", "--no-deps", "--format-version=1", "--manifest-path", manifest, cwd=str(Path(manifest).parent), env=env))
    packages = [p for p in metadata["packages"] if p["name"] == item["name"] and p["version"] == item["version"]]
    if len(packages) != 1:
        raise ValueError("published crate does not match checked-out commit")
    bins = [safe(t["name"]) for t in packages[0]["targets"] if "bin" in t["kind"]]
    if not bins:
        raise ValueError("crate has no binaries")
    command = ["cargo-zigbuild", "zigbuild"] if "linux" in target else ["cargo", "build"]
    build_target = target + ".2.28" if "linux" in target else target
    # A workspace root's manifest would otherwise build its default members.
    run(*command, "--locked", "--release", "--package", f"{item['name']}@{item['version']}", "--bins", "--all-features", "--manifest-path", manifest, "--target", build_target, cwd=str(Path(manifest).parent), env=env)
    out = Path("archives")
    out.mkdir(exist_ok=True)
    root = asset(item, target).removesuffix(".tgz")
    with tarfile.open(out / asset(item, target), "w:gz") as archive:
        for binary in bins:
            filename = binary + (".exe" if "windows" in target else "")
            archive.add(Path(metadata["target_directory"]) / target / "release" / filename, arcname=root + "/" + filename)


def built(item):
    for target in targets(item):
        if not (Path("archives") / asset(item, target)).is_file():
            raise ValueError(f"missing archive for {target}")


def inventory(item):
    """Pick each target's artifact from this run, refusing any but exactly one built by a job whose latest attempt succeeded."""
    # download-artifact keeps only the newest of same-named artifacts, so duplicates
    # show only in the unfiltered list. Artifacts carry no run attempt, so one rule
    # covers reruns: re-running a build that already uploaded either fails its upload
    # or leaves a duplicate, and both hold back this crate.
    run_path = f"repos/{os.environ['GITHUB_REPOSITORY']}/actions/runs/{os.environ['GITHUB_RUN_ID']}"
    artifacts = [a for page in json.loads(run("gh", "api", "--paginate", "--slurp", run_path + "/artifacts?per_page=100")) for a in page["artifacts"]]
    jobs = [j for page in json.loads(run("gh", "api", "--paginate", "--slurp", run_path + "/jobs?filter=all&per_page=100")) for j in page["jobs"]]
    found = {}
    for target in targets(item):
        name = artifact(item, target)
        matches = [a for a in artifacts if a["name"] == name and not a["expired"]]
        if len(matches) != 1:
            raise ValueError(f"{name}: expected one artifact in this run, found {len(matches)}")
        # A called workflow's job is named after its caller's job, as in "build / NAME".
        builds = [j for j in jobs if j["name"] == name or j["name"].endswith(" / " + name)]
        attempt = max((j["run_attempt"] for j in builds), default=None)
        latest = [j for j in builds if j["run_attempt"] == attempt]
        if len(latest) != 1 or latest[0]["status"] != "completed" or latest[0]["conclusion"] != "success":
            raise ValueError(f"{name}: its build job's latest attempt did not succeed")
        digest = matches[0].get("digest") or ""
        if not re.fullmatch(r"sha256:[0-9a-f]{64}", digest):
            raise ValueError(f"{name}: artifact has no SHA-256 digest")
        found[target] = {"id": matches[0]["id"], "digest": digest}
    return found


def collect(item, found, downloads, out=Path("archives")):
    """Stage each target's verified archive from its downloaded artifact ZIP, never through a general extractor."""
    # Only archives verified here are attested and published.
    out.mkdir()
    for target in targets(item):
        name = artifact(item, target)
        files = list((downloads / target).iterdir()) if (downloads / target).is_dir() else []
        if len(files) != 1 or files[0].is_symlink() or not files[0].is_file():
            raise ValueError(f"{name}: expected one downloaded artifact file")
        with files[0].open("rb") as file:
            if "sha256:" + hashlib.file_digest(file, "sha256").hexdigest() != found[target]["digest"]:
                raise ValueError(f"{name}: downloaded artifact does not match its digest")
        expected = asset(item, target)
        with zipfile.ZipFile(files[0]) as archive:
            members = archive.infolist()
            if [(m.filename, m.orig_filename) for m in members] != [(expected, expected)]:
                raise ValueError(f"{name}: artifact must hold only {expected}")
            member = members[0]
            # A ZIP may record no file type, but never a link or device.
            if member.is_dir() or stat.S_IFMT(member.external_attr >> 16) not in (0, stat.S_IFREG) or member.file_size > MAX_ASSET:
                raise ValueError(f"{name}: {expected} is not a regular file within GitHub's asset limit")
            with archive.open(member) as source, (out / expected).open("xb") as dest:
                shutil.copyfileobj(source, dest)


def publish(item):
    # This job executes only pipeline code, never the crate's code or Cargo.
    built(item)
    name = tag(item)
    if name != item["tag"]:
        raise ValueError("release tag changed since discovery")
    release = api("releases/tags/" + urllib.parse.quote(name, safe=""), missing=True)
    if release is None:
        release = api("releases", data={"tag_name": name, "target_commitish": item["sha"], "name": name, "prerelease": "-" in item["version"].split("+", 1)[0], "make_latest": "false"})
    existing = json.loads(run("gh", "api", "--paginate", "--slurp", f"repos/{os.environ['GITHUB_REPOSITORY']}/releases/{release['id']}/assets"))
    existing = {a["name"]: a for page in existing for a in page}
    for target, _ in TARGETS:
        path = Path("archives") / asset(item, target)
        if path.name not in existing:
            if not path.is_file():
                raise ValueError(f"missing asset: {path.name}")
            run("gh", "release", "upload", name, str(path), "--repo", os.environ["GITHUB_REPOSITORY"])


def provenance(item):
    repo = os.environ["GITHUB_REPOSITORY"]
    workflow = os.environ["PIPELINE_WORKFLOW"]
    # GitHub's attestation API accepts only its own build types, so this follows workflow/v1's shape.
    caller, ref = os.environ["GITHUB_WORKFLOW_REF"].split("@", 1)
    owner, name, path = caller.split("/", 2)
    if f"{owner}/{name}" != repo:
        raise ValueError(f"workflow {caller} is not in {repo}")
    return {
        "buildDefinition": {
            "buildType": "https://actions.github.io/buildtypes/workflow/v1",
            "externalParameters": {
                "workflow": {"ref": ref, "repository": f"https://github.com/{repo}", "path": path},
                "crate": item["name"],
                "version": item["version"],
                "path": item["path"],
            },
            "internalParameters": {
                "github": {
                    "event_name": os.environ["GITHUB_EVENT_NAME"],
                    "repository_id": os.environ["GITHUB_REPOSITORY_ID"],
                    "repository_owner_id": os.environ["GITHUB_REPOSITORY_OWNER_ID"],
                    "runner_environment": os.environ["RUNNER_ENVIRONMENT"],
                },
            },
            "resolvedDependencies": [
                # GitHub's attestation API checks the first dependency against the signing certificate's
                # source ref and commit, so the caller workflow's own commit leads; a backfilled crate's differs.
                {"uri": f"git+https://github.com/{repo}@{ref}", "digest": {"gitCommit": os.environ["GITHUB_SHA"]}},
                {"uri": f"git+https://github.com/{repo}", "digest": {"gitCommit": item["sha"]}},
                {"uri": "https://github.com/" + workflow.split("@", 1)[0], "digest": {"gitCommit": os.environ["PIPELINE_SHA"]}},
            ],
        },
        "runDetails": {
            "builder": {"id": "https://github.com/" + workflow},
            "metadata": {"invocationId": f"https://github.com/{repo}/actions/runs/{os.environ['GITHUB_RUN_ID']}/attempts/{os.environ['GITHUB_RUN_ATTEMPT']}"},
        },
    }


def main():
    mode = sys.argv[1]
    if mode in ("discover", "fixture"):
        items = discover() if mode == "discover" else [{"name": "crabx-pipeline-fixture", "version": "0.0.0", "sha": os.environ["GITHUB_SHA"], "path": "pipeline/fixture"}]
        matrix = [{**item, "target": target, "os": runner} for item in items for target, runner in TARGETS if target in targets(item)]
        # GitHub's matrix has a hard limit. Do not silently omit old releases.
        if len(matrix) > 256:
            raise ValueError("backfill exceeds GitHub's 256-job matrix; publish older versions in batches")
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            for key, value in {"builds": {"include": matrix}, "releases": {"include": items}, "any": bool(items)}.items():
                print(f"{key}={json.dumps(value, separators=(',', ':'))}", file=output)
    elif mode == "build":
        build(json.loads(os.environ["ITEM"]), os.environ["TARGET"])
    elif mode == "inventory":
        found = inventory(json.loads(os.environ["ITEM"]))
        # A fresh directory, so nothing left from an earlier step is mistaken for a download.
        downloads = Path(os.environ["RUNNER_TEMP"], "crabx")
        downloads.mkdir()
        (downloads / "inventory.json").write_text(json.dumps(found))
        with open(os.environ["GITHUB_OUTPUT"], "a") as output:
            for target, entry in found.items():
                print(f"{target}={entry['id']}", file=output)
    elif mode == "collect":
        downloads = Path(os.environ["RUNNER_TEMP"], "crabx")
        collect(json.loads(os.environ["ITEM"]), json.loads((downloads / "inventory.json").read_text()), downloads)
    elif mode == "provenance":
        # A crate missing any archive, as when one of its builds failed, is neither attested nor published.
        built(json.loads(os.environ["ITEM"]))
        Path("predicate.json").write_text(json.dumps(provenance(json.loads(os.environ["ITEM"]))))
    elif mode == "publish":
        publish(json.loads(os.environ["ITEM"]))
    else:
        raise ValueError(f"unknown mode: {mode}")


if __name__ == "__main__":
    main()
