"""Exercise release discovery, tag validation, and recovery without write credentials."""

import hashlib
import importlib.util
import io
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import tarfile
import tempfile
import unittest
from unittest.mock import patch
import zipfile

spec = importlib.util.spec_from_file_location("pipeline", Path(__file__).with_name("pipeline.py"))
p = importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)
ITEM = {"name": "demo", "version": "1.2.3", "sha": "a" * 40, "path": "cli", "tag": "demo-v1.2.3"}
REPO = "https://github.com/example/demo"


def crate(files):
    archive = io.BytesIO()
    with tarfile.open(fileobj=archive, mode="w:gz") as tar:
        for name, value in files.items():
            info = tarfile.TarInfo("demo-1.2.3/" + name)
            info.size = len(value)
            tar.addfile(info, io.BytesIO(value))
    archive.seek(0)
    return archive


class Pipeline(unittest.TestCase):
    PROVENANCE_ENV = {
        "GITHUB_REPOSITORY": "example/demo",
        "GITHUB_WORKFLOW_REF": "example/demo/.github/workflows/binaries.yml@refs/heads/main",
        "GITHUB_EVENT_NAME": "push",
        "GITHUB_REPOSITORY_ID": "1",
        "GITHUB_REPOSITORY_OWNER_ID": "2",
        "RUNNER_ENVIRONMENT": "github-hosted",
        "GITHUB_RUN_ID": "3",
        "GITHUB_RUN_ATTEMPT": "1",
        "PIPELINE_WORKFLOW": "kixel-ai/crabx/.github/workflows/crabx.yml@refs/tags/pipeline-v1.0.0",
        "PIPELINE_SHA": "b" * 40,
        "GITHUB_SHA": "c" * 40,
    }

    def test_provenance_uses_githubs_workflow_build_type(self):
        with patch.dict(os.environ, self.PROVENANCE_ENV):
            definition = p.provenance(ITEM)["buildDefinition"]
        self.assertEqual(definition["buildType"], "https://actions.github.io/buildtypes/workflow/v1")
        self.assertEqual(definition["externalParameters"], {
            "workflow": {"ref": "refs/heads/main", "repository": REPO, "path": ".github/workflows/binaries.yml"},
            "crate": "demo", "version": "1.2.3", "path": "cli",
        })
        self.assertEqual(definition["internalParameters"]["github"]["runner_environment"], "github-hosted")
        self.assertEqual(definition["resolvedDependencies"], [
            {"uri": "git+https://github.com/example/demo@refs/heads/main", "digest": {"gitCommit": "c" * 40}},
            {"uri": "git+https://github.com/example/demo", "digest": {"gitCommit": ITEM["sha"]}},
            {"uri": "https://github.com/kixel-ai/crabx/.github/workflows/crabx.yml", "digest": {"gitCommit": "b" * 40}},
        ])

    def test_provenance_refuses_a_workflow_from_another_repository(self):
        env = {**self.PROVENANCE_ENV, "GITHUB_WORKFLOW_REF": "other/demo/.github/workflows/binaries.yml@refs/heads/main"}
        with patch.dict(os.environ, env), self.assertRaisesRegex(ValueError, "not in example/demo"):
            p.provenance(ITEM)

    def test_tag_identity(self):
        with patch.object(p, "api", return_value=None):
            self.assertEqual(p.tag(ITEM), "demo-v1.2.3")
        for kind in ("commit", "tag"):
            def api(path, **kwargs):
                if path == "git/ref/tags/demo-v1.2.3":
                    return {"object": {"type": kind, "sha": ITEM["sha"]}}
                if path.startswith("git/tags/"):
                    return {"object": {"type": "commit", "sha": ITEM["sha"]}}
            with patch.object(p, "api", side_effect=api):
                self.assertEqual(p.tag(ITEM), ITEM["tag"])
        with patch.object(p, "api", return_value={"object": {"type": "commit", "sha": "b" * 40}}):
            with self.assertRaisesRegex(ValueError, "does not point"):
                p.tag(ITEM)
        # A bare version tag on another commit is another crate's, never this one's.
        for sha, expected in (("b" * 40, "demo-v1.2.3"), (ITEM["sha"], "v1.2.3")):
            def api(path, **kwargs):
                if path == "git/ref/tags/v1.2.3":
                    return {"object": {"type": "commit", "sha": sha}}
            with patch.object(p, "api", side_effect=api):
                self.assertEqual(p.tag(ITEM), expected)
        # A crate named like a version tag still needs its own tag to match.
        vtool = {**ITEM, "name": "vtool"}
        with patch.object(p, "api", side_effect=lambda path, **kwargs: {"object": {"type": "commit", "sha": "b" * 40}} if path == "git/ref/tags/vtool-v1.2.3" else None):
            with self.assertRaisesRegex(ValueError, "does not point"):
                p.tag(vtool)

    def test_registry_metadata(self):
        manifest = f'[package]\nname="demo"\nversion="1.2.3"\nrepository="{REPO}"\n[[bin]]\nname="demo"\n'.encode()
        for dirty, path in ((False, "cli"), (True, "cli"), (False, "../escape"), (False, "/escape")):
            archive = crate({"Cargo.toml": manifest, ".cargo_vcs_info.json": json.dumps({"git": {"sha1": ITEM["sha"], "dirty": dirty}, "path_in_vcs": path}).encode()})
            with patch.object(p.urllib.request, "urlopen", return_value=archive):
                if dirty or path != "cli":
                    with self.assertRaises(ValueError):
                        p.published("demo", "1.2.3", REPO)
                else:
                    item = p.published("demo", "1.2.3", REPO)
                    self.assertEqual(item["sha"], ITEM["sha"])
        with patch.object(p.urllib.request, "urlopen", return_value=crate({"Cargo.toml": manifest})):
            with self.assertRaisesRegex(ValueError, "without its git commit"):
                p.published("demo", "1.2.3", REPO)
        # Another repository's release is skipped before its source is checked.
        dirty = json.dumps({"git": {"sha1": ITEM["sha"], "dirty": True}}).encode()
        with patch.object(p.urllib.request, "urlopen", return_value=crate({"Cargo.toml": manifest, ".cargo_vcs_info.json": dirty})):
            self.assertIsNone(p.published("demo", "1.2.3", "https://github.com/example/other"))

    def test_null_repository(self):
        with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/demo", "CALLER_WORKFLOW": "example/demo/.github/workflows/binaries.yml@main"}), patch.object(p, "run", side_effect=["2026-01-01T00:00:00+00:00", "Cargo.toml"]), patch.object(Path, "read_text", return_value='[package]\nname="demo"'), patch.object(p, "request", return_value={"crate": {"repository": None}}):
            self.assertEqual(p.discover(), [])

    def test_implicit_binary(self):
        for source, autobins, expected in (("src/main.rs", True, True), ("src/bin/tool.rs", True, True), ("src/bin/tool/main.rs", True, True), ("src/lib.rs", True, False), ("src/main.rs", False, False)):
            archive = crate({
                "Cargo.toml": f'[package]\nname="demo"\nversion="1.2.3"\nrepository="{REPO}"\nautobins={str(autobins).lower()}\n'.encode(),
                ".cargo_vcs_info.json": json.dumps({"git": {"sha1": ITEM["sha"]}}).encode(),
                source: b"fn main() {}",
            })
            with patch.object(p.urllib.request, "urlopen", return_value=archive):
                self.assertEqual(p.published("demo", "1.2.3", REPO) is not None, expected)

    def test_template_name(self):
        with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/demo", "CALLER_WORKFLOW": "example/demo/.github/workflows/binaries.yml@main"}), patch.object(p, "run", side_effect=["2026-01-01T00:00:00+00:00", "Cargo.toml"]), patch.object(Path, "read_text", return_value='[package]\nname="{{project-name}}"'), patch.object(p, "request") as request:
            self.assertEqual(p.discover(), [])
        request.assert_not_called()

    def test_archive_layout(self):
        metadata = {"target_directory": "target", "packages": [{"name": "demo", "version": "1.2.3", "targets": [{"name": "demo", "kind": ["bin"]}, {"name": "tool", "kind": ["bin"]}, {"name": "demo", "kind": ["lib"]}]}]}
        cwd = os.getcwd()
        with tempfile.TemporaryDirectory() as tmp:
            os.chdir(tmp)
            try:
                for target in ("x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"):
                    exe = ".exe" if "windows" in target else ""
                    release = Path("target", target, "release")
                    release.mkdir(parents=True, exist_ok=True)
                    for binary in ("demo", "tool"):
                        (release / (binary + exe)).write_text(binary)
                    with patch.object(p, "run", side_effect=[json.dumps(metadata), ""]) as run:
                        p.build(ITEM, target)
                    self.assertEqual(run.call_args_list[1].args[-1], target + (".2.28" if "linux" in target else ""))
                    args = run.call_args_list[1].args
                    self.assertEqual(args[args.index("--package") + 1], "demo@1.2.3")
                    root = f"demo-{target}-v1.2.3"
                    with tarfile.open(Path("archives", root + ".tgz")) as archive:
                        self.assertEqual(sorted(archive.getnames()), [f"{root}/demo{exe}", f"{root}/tool{exe}"])
                        self.assertEqual(archive.extractfile(f"{root}/tool{exe}").read(), b"tool")
                # A crate missing any archive is not attested.
                p.built({**ITEM, "targets": ["x86_64-unknown-linux-gnu"]})
                with self.assertRaisesRegex(ValueError, "missing archive"):
                    p.built(ITEM)
            finally:
                os.chdir(cwd)

    def test_release_kind(self):
        for version, prerelease in (("1.0.0+build-5", False), ("1.0.0-rc.1+build-5", True)):
            item = {**ITEM, "version": version}
            with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/demo"}), patch.object(p, "tag", return_value=ITEM["tag"]), patch.object(p, "api", side_effect=[None, {"id": 7}]) as api, patch.object(p, "run", return_value="[[]]"), patch.object(Path, "is_file", return_value=True):
                p.publish(item)
            self.assertEqual(api.call_args.kwargs["data"]["prerelease"], prerelease)

    def test_partial_release_recovery(self):
        first = p.asset(ITEM, p.TARGETS[0][0])
        existing = [[{"name": first}, {"name": "unrelated.txt"}]]
        with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/demo"}), patch.object(p, "tag", return_value=ITEM["tag"]), patch.object(p, "api", return_value={"id": 7}), patch.object(p, "run", return_value=json.dumps(existing)) as run, patch.object(Path, "is_file", return_value=True):
            p.publish(ITEM)
        uploads = [call.args for call in run.call_args_list if call.args[:3] == ("gh", "release", "upload")]
        self.assertEqual(len(uploads), 5)
        self.assertNotIn("archives/" + first, [args[4] for args in uploads])

    def discovered(self):
        """Run the real discover() and published() against a crate whose first target is already released."""
        manifest = f'[package]\nname="demo"\nversion="1.2.3"\nrepository="{REPO}"\n[[bin]]\nname="demo"\n'.encode()
        archive = crate({"Cargo.toml": manifest, ".cargo_vcs_info.json": json.dumps({"git": {"sha1": ITEM["sha"]}, "path_in_vcs": ITEM["path"]}).encode()})
        def run(*args):
            if args[:2] == ("git", "log"):
                return "2026-01-01T00:00:00+00:00"
            if args[:2] == ("git", "ls-files"):
                return "Cargo.toml"
            return p.asset(ITEM, p.TARGETS[0][0])
        with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/demo", "CALLER_WORKFLOW": "example/demo/.github/workflows/binaries.yml@main"}), patch.object(p, "run", side_effect=run), patch.object(Path, "read_text", return_value='[package]\nname="demo"'), patch.object(p, "request", return_value={"crate": {"repository": REPO}, "versions": [{"num": "1.2.3", "created_at": "2026-02-01T00:00:00Z", "yanked": False}, {"num": "0.1.0", "created_at": "2025-01-01T00:00:00Z", "yanked": False}]}), patch.object(p.urllib.request, "urlopen", return_value=archive), patch.object(p, "published", wraps=p.published) as published, patch.object(p, "tag", return_value=ITEM["tag"]), patch.object(p, "api", return_value={"id": 7}):
            items = p.discover()
        self.assertEqual(published.call_count, 1)
        return items

    def test_discovery_missing_targets(self):
        items = self.discovered()
        self.assertEqual(items, [{**ITEM, "targets": [t for t, _ in p.TARGETS[1:]]}])

    def test_build_refuses_a_checkout_that_is_not_the_published_crate(self):
        for packages, error in (
            ([], "does not match"),
            ([{"name": "demo", "version": "9.9.9", "targets": []}], "does not match"),
            ([{"name": "demo", "version": "1.2.3", "targets": [{"name": "demo", "kind": ["lib"]}]}], "no binaries"),
        ):
            metadata = json.dumps({"target_directory": "target", "packages": packages})
            with patch.object(p, "run", return_value=metadata) as run, self.assertRaisesRegex(ValueError, error):
                p.build(ITEM, "x86_64-unknown-linux-gnu")
            # Nothing of the crate's is compiled.
            self.assertEqual(run.call_count, 1)

    def test_publish_refuses_a_moved_tag_or_a_missing_asset(self):
        with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/demo"}), patch.object(Path, "is_file", return_value=True), patch.object(p, "tag", return_value="v1.2.3"), patch.object(p, "api") as api:
            with self.assertRaisesRegex(ValueError, "tag changed"):
                p.publish(ITEM)
        api.assert_not_called()
        # An asset deleted after the archive check is never replaced by an upload of nothing.
        with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/demo"}), patch.object(p, "built"), patch.object(Path, "is_file", return_value=False), patch.object(p, "tag", return_value=ITEM["tag"]), patch.object(p, "api", return_value={"id": 7}), patch.object(p, "run", return_value="[[]]") as run:
            with self.assertRaisesRegex(ValueError, "missing asset"):
                p.publish(ITEM)
        self.assertFalse([call for call in run.call_args_list if call.args[:3] == ("gh", "release", "upload")])

    def outputs(self, mode, items=()):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "out"
            env = {"GITHUB_OUTPUT": str(out), "GITHUB_SHA": "c" * 40}
            with patch.dict(os.environ, env), patch.object(p.sys, "argv", ["pipeline.py", mode]), patch.object(p, "discover", return_value=list(items)), patch.object(p, "unpublished", return_value=list(items)):
                p.main()
            return {key: json.loads(value) for key, value in (line.split("=", 1) for line in out.read_text().splitlines())}

    def test_outputs_carry_every_field_the_workflows_read(self):
        def reads(workflow):
            return set(re.findall(r"\bmatrix\.([a-z_]+)", (Path(__file__).parents[1] / ".github/workflows" / workflow).read_text()))
        for mode, items, count in (("fixture", (), 6), ("dry-run", self.unpublished(), 6), ("discover", self.discovered(), 5)):
            found = self.outputs(mode, items)
            self.assertIs(found["any"], True)
            builds = found["builds"]["include"]
            self.assertEqual(len(builds), count)
            for build in builds:
                self.assertLessEqual(reads("crabx-build.yml"), build.keys(), mode)
                self.assertEqual(build["os"], dict(p.TARGETS)[build["target"]])
        # The fixture publishes nothing, so only discovered releases carry a tag.
        for release in found["releases"]["include"]:
            self.assertLessEqual(reads("crabx.yml"), release.keys())
        self.assertEqual(self.outputs("discover"), {"builds": {"include": []}, "releases": {"include": []}, "any": False})

    def unpublished(self, members=None):
        """Run the real unpublished() with Cargo on a git workspace whose tracked members are given as manifests."""
        members = members or {
            "cli": '[package]\nname="demo"\nversion.workspace=true\nrepository.workspace=true\n[[bin]]\nname="demo"\npath="main.rs"\n',
            "lib": '[package]\nname="demo-lib"\nversion="0.1.0"\nrepository.workspace=true\n[lib]\npath="lib.rs"\n',
            "private": '[package]\nname="demo-private"\nversion="0.1.0"\npublish=false\nrepository.workspace=true\n[[bin]]\nname="private"\npath="main.rs"\n',
            "fork": '[package]\nname="demo-fork"\nversion="0.1.0"\nrepository="https://github.com/other/demo"\n[[bin]]\nname="fork"\npath="main.rs"\n',
        }
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            workspace = ", ".join(f'"{m}"' for m in [*members, "untracked"])
            (root / "Cargo.toml").write_text(f'[workspace]\nmembers=[{workspace}]\n[workspace.package]\nversion="{ITEM["version"]}"\nrepository="{REPO}"\n')
            untracked = '[package]\nname="demo-untracked"\nversion="0.1.0"\nrepository.workspace=true\n[[bin]]\nname="untracked"\npath="main.rs"\n'
            for member, manifest in {**members, "untracked": untracked}.items():
                (root / member).mkdir()
                (root / member / "Cargo.toml").write_text(manifest)
                for source in ("main.rs", "lib.rs"):
                    (root / member / source).write_text("")
            subprocess.run(["git", "init", "-q", str(root)], check=True)
            subprocess.run(["git", "-C", str(root), "add", "Cargo.toml", *(f"{m}/Cargo.toml" for m in members)], check=True)
            cwd = os.getcwd()
            os.chdir(root)
            try:
                with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/demo", "GITHUB_SHA": ITEM["sha"]}):
                    return p.unpublished()
            finally:
                os.chdir(cwd)

    def test_dry_run_plans_this_commits_tracked_binary_crates(self):
        # Only the tracked, publishable binary crate naming this repository, at its inherited version.
        self.assertEqual(self.unpublished(), [{k: v for k, v in ITEM.items() if k != "tag"}])
        # A gate with nothing to build would pass vacuously, so it fails instead.
        with self.assertRaisesRegex(ValueError, "nothing to build"):
            self.unpublished({"lib": '[package]\nname="demo-lib"\nversion="0.1.0"\n[lib]\npath="lib.rs"\n'})

    def test_build_runs_cargo_without_runner_apis(self):
        metadata = json.dumps({"target_directory": "target", "packages": [{"name": "demo", "version": "1.2.3", "targets": [{"name": "demo", "kind": ["bin"]}]}]})
        runner = {name: "secret" for name in p.RUNNER_ENV}
        with patch.dict(os.environ, {**runner, "PATH": "/bin"}), patch.object(p, "run", side_effect=[metadata, RuntimeError("stop")]) as run, self.assertRaisesRegex(RuntimeError, "stop"):
            p.build(ITEM, "x86_64-unknown-linux-gnu")
        for call in run.call_args_list:
            self.assertFalse(p.RUNNER_ENV & call.kwargs["env"].keys())
            self.assertEqual(call.kwargs["env"]["PATH"], "/bin")

    def test_uploads_and_downloads_cover_every_target(self):
        root = Path(__file__).parents[1]
        build = (root / ".github/workflows/crabx-build.yml").read_text()
        # The upload's name and path, and the job name the publish job finds, follow artifact() and asset().
        self.assertIn("name: ${{ inputs.upload && 'crabx' || 'check' }}-${{ matrix.name }}@${{ matrix.version }}@${{ matrix.target }}\n    strategy", build)
        self.assertIn("path: archives/${{ matrix.name }}-${{ matrix.target }}-v${{ matrix.version }}.tgz", build)
        action = (root / ".github/actions/crabx-archives/action.yml").read_text()
        self.assertEqual(re.findall(r"path: \$\{\{ runner.temp \}\}/crabx/(\S+)", action), [t for t, _ in p.TARGETS])

    def run_api(self, artifacts, jobs):
        """Answer inventory()'s paginated artifact and job listings."""
        def run(*args, **kwargs):
            path = args[-1]
            if "/artifacts?" in path:
                return json.dumps([{"artifacts": artifacts[:1]}, {"artifacts": artifacts[1:]}])
            if "/jobs?" in path:
                return json.dumps([{"jobs": jobs}])
            raise AssertionError(path)
        return patch.object(p, "run", side_effect=run)

    def inventory(self, item, artifacts, jobs):
        with patch.dict(os.environ, {"GITHUB_REPOSITORY": "example/demo", "GITHUB_RUN_ID": "9"}), self.run_api(artifacts, jobs):
            return p.inventory(item)

    @staticmethod
    def uploaded(item, target, id, digest="sha256:" + "0" * 64, expired=False):
        return {"id": id, "name": p.artifact(item, target), "digest": digest, "expired": expired}

    @staticmethod
    def job(item, target, conclusion="success", attempt=1, caller="build"):
        return {"name": f"{caller} / {p.artifact(item, target)}", "run_attempt": attempt, "status": "completed", "conclusion": conclusion}

    def test_inventory(self):
        all_targets = [t for t, _ in p.TARGETS]
        artifacts = [self.uploaded(ITEM, t, n) for n, t in enumerate(all_targets)]
        jobs = [self.job(ITEM, t) for t in all_targets]
        found = self.inventory(ITEM, artifacts, jobs)
        self.assertEqual({t: e["id"] for t, e in found.items()}, dict(zip(all_targets, range(6))))
        # A backfill of one missing target needs only that target's artifact and job.
        last = {**ITEM, "targets": all_targets[-1:]}
        self.assertEqual(list(self.inventory(last, artifacts[-1:], jobs[-1:])), all_targets[-1:])
        # Another crate's failed build or duplicate artifact holds back only that crate.
        other = {**ITEM, "name": "other"}
        self.assertEqual(len(self.inventory(ITEM, artifacts + [self.uploaded(other, all_targets[0], 50)] * 2, jobs + [self.job(other, all_targets[0], "failure")])), 6)
        # "Re-run failed jobs" keeps the prior attempt's successful builds and their artifacts.
        rerun = jobs[1:] + [self.job(ITEM, all_targets[0], "failure"), self.job(ITEM, all_targets[0], attempt=2)]
        self.assertEqual(len(self.inventory(ITEM, artifacts, rerun)), 6)
        # release.yml's pre-publish check of the same targets uploads nothing and is named apart.
        checks = [{**self.job(ITEM, t, "failure", caller="check"), "name": "check / check-" + p.artifact(ITEM, t).removeprefix("crabx-")} for t in all_targets]
        self.assertEqual(len(self.inventory(ITEM, artifacts, jobs + checks)), 6)
        # An expired artifact is neither a duplicate nor an archive.
        self.assertEqual(len(self.inventory(ITEM, artifacts + [self.uploaded(ITEM, all_targets[0], 60, expired=True)], jobs)), 6)
        for case, artifacts_, jobs_, error in (
            ("missing artifact", artifacts[1:], jobs, "found 0"),
            ("only an expired artifact", [{**artifacts[0], "expired": True}] + artifacts[1:], jobs, "found 0"),
            ("duplicate artifact", artifacts + [self.uploaded(ITEM, all_targets[0], 70)], jobs, "found 2"),
            ("failed build", artifacts, jobs[1:] + [self.job(ITEM, all_targets[0], "failure")], "did not succeed"),
            ("missing build job", artifacts, jobs[1:], "did not succeed"),
            ("unfinished build job", artifacts, jobs[1:] + [{**jobs[0], "status": "in_progress", "conclusion": None}], "did not succeed"),
            ("ambiguous build job", artifacts, jobs + [self.job(ITEM, all_targets[0], caller="other")], "did not succeed"),
            ("job name suffix only", artifacts, jobs[1:] + [{**jobs[0], "name": "build / x" + p.artifact(ITEM, all_targets[0])}], "did not succeed"),
            # Re-running a build that already uploaded fails its upload, so its latest attempt fails.
            ("re-run uploaded build", artifacts, jobs + [self.job(ITEM, all_targets[0], "failure", attempt=2)], "did not succeed"),
            ("missing digest", [{**artifacts[0], "digest": None}] + artifacts[1:], jobs, "no SHA-256"),
        ):
            with self.subTest(case), self.assertRaisesRegex(ValueError, error):
                self.inventory(ITEM, artifacts_, jobs_)

    def zipped(self, path, members):
        """Write an artifact ZIP holding (name, data, unix mode) members, and return its inventory entry."""
        with zipfile.ZipFile(path, "w") as archive:
            for name, data, mode in members:
                info = zipfile.ZipInfo(name)
                info.external_attr = mode << 16
                archive.writestr(info, data)
        return {"id": 1, "digest": "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()}

    def collect(self, item, layout):
        """Run collect() over per-target downloads, each a list of ZIP members or None for no download."""
        with tempfile.TemporaryDirectory() as tmp:
            downloads = Path(tmp, "crabx")
            found = {}
            for target, members in layout.items():
                (downloads / target).mkdir(parents=True)
                if members is not None:
                    found[target] = self.zipped(downloads / target / "artifact.zip", members)
            out = Path(tmp, "archives")
            p.collect(item, found, downloads, out)
            return {path.name: path.read_bytes() for path in out.iterdir()}

    def test_collect(self):
        target = p.TARGETS[0][0]
        item = {**ITEM, "targets": [target]}
        name = p.asset(item, target)
        regular = stat.S_IFREG | 0o644
        self.assertEqual(self.collect(item, {target: [(name, b"tgz", regular)]}), {name: b"tgz"})
        # A ZIP may record permissions without a file type.
        self.assertEqual(self.collect(item, {target: [(name, b"tgz", 0o644)]}), {name: b"tgz"})
        everything = {t: [(p.asset(ITEM, t), t.encode(), regular)] for t, _ in p.TARGETS}
        self.assertEqual(self.collect(ITEM, everything), {p.asset(ITEM, t): t.encode() for t, _ in p.TARGETS})
        for case, members, error in (
            ("missing archive", None, "one downloaded artifact"),
            ("extra archive", [(name, b"tgz", regular), ("other.tgz", b"x", regular)], "only"),
            ("wrong filename", [(p.asset(item, p.TARGETS[1][0]), b"tgz", regular)], "only"),
            ("traversal", [("../" + name, b"tgz", regular)], "only"),
            ("symlink", [(name, b"/etc/passwd", stat.S_IFLNK | 0o777)], "regular file"),
            ("directory", [(name + "/", b"", stat.S_IFDIR | 0o755)], "only"),
        ):
            with self.subTest(case), self.assertRaisesRegex(ValueError, error):
                self.collect(item, {target: members})
        # Nothing is unpacked from a ZIP whose digest is not the artifact's.
        with tempfile.TemporaryDirectory() as tmp:
            (Path(tmp, "crabx", target)).mkdir(parents=True)
            entry = self.zipped(Path(tmp, "crabx", target, "artifact.zip"), [(name, b"tgz", regular)])
            with self.assertRaisesRegex(ValueError, "digest"):
                p.collect(item, {target: {**entry, "digest": "sha256:" + "0" * 64}}, Path(tmp, "crabx"), Path(tmp, "archives"))
            self.assertEqual(list(Path(tmp, "archives").iterdir()), [])
            # A staging directory left from before is never added to.
            with self.assertRaises(FileExistsError):
                p.collect(item, {target: entry}, Path(tmp, "crabx"), Path(tmp, "archives"))
        # A second downloaded file beside the artifact is refused.
        with tempfile.TemporaryDirectory() as tmp:
            Path(tmp, "crabx", target).mkdir(parents=True)
            entry = self.zipped(Path(tmp, "crabx", target, "artifact.zip"), [(name, b"tgz", regular)])
            Path(tmp, "crabx", target, "extra").write_bytes(b"")
            with self.assertRaisesRegex(ValueError, "one downloaded artifact"):
                p.collect(item, {target: entry}, Path(tmp, "crabx"), Path(tmp, "archives"))

    def test_a_backfill_past_the_matrix_limit_fails(self):
        items = [{**ITEM, "version": f"1.0.{n}"} for n in range(256 // len(p.TARGETS) + 1)]
        with self.assertRaisesRegex(ValueError, "256-job matrix"):
            self.outputs("discover", items)
        self.outputs("discover", items[:-1])


if __name__ == "__main__":
    unittest.main()
