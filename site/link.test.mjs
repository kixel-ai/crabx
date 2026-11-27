// Run with `node --test site/`: the form's workflow must be byte for byte what `crabx init` writes.
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { test } from "node:test";
import vm from "node:vm";

const read = (path) => readFileSync(new URL(path, import.meta.url), "utf8");
const link = vm.createContext({});
vm.runInContext(read("link.js"), link);
const template = read("../src/binaries.yml");

test("matches the workflow pipeline/README.md shows, which init.rs's tests pin to crabx init", () => {
	assert.ok(read("../pipeline/README.md").includes("```yaml\n" + link.workflow(template, "main") + "```"));
});

test("branch patterns are literal, as in init.rs", () => {
	assert.ok(link.workflow(template, "main").includes("    branches: ['main']\n"));
	assert.ok(link.workflow(template, "it's+1").includes("    branches: ['it''s\\+1']\n"));
	assert.ok(link.workflow(template, "!x[").includes("    branches: ['\\!x\\[']\n"));
	assert.ok(link.workflow(template, "$&").includes("    branches: ['$&']\n"));
});

test("the editor link round-trips the file and the branch", () => {
	const contents = link.workflow(template, "release/1.x #?");
	const url = new URL(link.editorUrl("o", "r.rs", "release/1.x #?", contents));
	assert.equal(url.pathname, "/o/r.rs/new/release/1.x%20%23%3F");
	assert.equal(url.searchParams.get("filename"), ".github/workflows/binaries.yml");
	assert.equal(url.searchParams.get("value"), contents);
});

test("refuses what crabx init refuses", () => {
	const tree = (...paths) => ({ truncated: false, tree: paths.map((path) => ({ path, type: "blob" })) });
	assert.equal(link.refusal({ private: false }, tree("crates/a/Cargo.toml")), "");
	assert.equal(link.refusal({ private: false }, tree("Cargo.toml", "src/main.rs")), "");
	assert.match(link.refusal({ private: true }, tree("Cargo.toml")), /private/);
	assert.match(link.refusal({ private: false }, tree("src/NotCargo.toml")), /no Cargo.toml/);
	assert.match(link.refusal({ private: false }, tree("Cargo.toml", ".github/workflows/binaries.yml")), /already exists/);
	assert.match(link.refusal({ private: false }, { truncated: true, tree: [] }), /too large/);
	assert.match(link.refusal({ private: false, archived: true }, tree("Cargo.toml")), /archived/);
	const blocked = { truncated: false, tree: [{ path: "Cargo.toml", type: "blob" }, { path: ".github", type: "commit" }] };
	assert.match(link.refusal({ private: false }, blocked), /not a directory/);
	const dirs = [{ path: "Cargo.toml", type: "blob" }, { path: ".github", type: "tree" }, { path: ".github/workflows", type: "tree" }];
	assert.equal(link.refusal({ private: false }, { truncated: false, tree: dirs }), "");
});
