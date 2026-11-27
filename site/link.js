// The pure half of the "set up a repository" form, apart from the DOM so site/link.test.mjs can run it.
// It mirrors `crabx init`: workflow() is init.rs's `workflow`, and refusal() is its refusals.

// The caller workflow for `template` (src/binaries.yml) whose pushes to `branch` trigger the pipeline.
function workflow(template, branch) {
	// Escape GitHub's filter pattern characters, then quote for YAML.
	let pattern = "";
	for (const c of branch) {
		if ("*?+[]!\\".includes(c)) pattern += "\\";
		pattern += c;
	}
	const quoted = `'${pattern.replaceAll("'", "''")}'`;
	// A function, so `$` in a branch name is not a replacement pattern.
	return template.replace("BRANCH", () => quoted);
}

const WORKFLOW_PATH = ".github/workflows/binaries.yml";

// Why `crabx init` would refuse this repository, from its public metadata and recursive tree, or "" when it would not.
function refusal(repo, tree) {
	if (repo.private) return "That repository is private; crabx's pipeline needs a public one.";
	if (repo.archived) return "That repository is archived; GitHub accepts no new files in it.";
	if (tree.truncated) return "That repository is too large to check from here; run `crabx init` in a clone instead.";
	const paths = tree.tree.map((entry) => entry.path);
	// A tracked file, symlink, or submodule there leaves no directory to put the workflow in.
	for (const dir of [".github", ".github/workflows"]) {
		const entry = tree.tree.find((entry) => entry.path === dir);
		if (entry && entry.type !== "tree") return `${dir} is not a directory in that repository, so it cannot hold a workflow.`;
	}
	if (!paths.some((path) => path === "Cargo.toml" || path.endsWith("/Cargo.toml"))) {
		return "That repository tracks no Cargo.toml; crabx's pipeline builds Rust crates.";
	}
	if (paths.includes(WORKFLOW_PATH)) {
		return `${WORKFLOW_PATH} already exists; run \`crabx init\` in a clone to check it, or delete it to write the crabx pipeline's caller.`;
	}
	return "";
}

// GitHub's new-file editor on `branch`, with the workflow filled in.
function editorUrl(owner, repo, branch, contents) {
	const path = branch.split("/").map(encodeURIComponent).join("/");
	return (
		`https://github.com/${owner}/${repo}/new/${path}` +
		`?filename=${encodeURIComponent(WORKFLOW_PATH)}&value=${encodeURIComponent(contents)}`
	);
}
