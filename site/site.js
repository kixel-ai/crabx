// Enhancement only: without this script the install command still selects on click.

// Copy the install command.
const copy = document.querySelector(".copy");
const command = copy.parentElement.querySelector(".sel");
copy.hidden = false;
copy.addEventListener("click", async () => {
	try {
		await navigator.clipboard.writeText(command.textContent);
		copy.textContent = "Copied";
	} catch {
		// Clipboard access denied: select the command so the reader can copy it.
		getSelection().selectAllChildren(command);
		copy.textContent = "Selected";
	}
	setTimeout(() => (copy.textContent = "Copy"), 1500);
});

// Set up a repository: check it as `crabx init` would, then open GitHub's new-file editor
// with the caller workflow filled in. Everything runs here; crabx holds no credential.
const setup = document.querySelector(".setup");
const status = setup.querySelector(".status");
setup.hidden = false;
setup.addEventListener("submit", async (event) => {
	event.preventDefault();
	const button = setup.querySelector("button");
	const [owner, repo] = setup.elements.repo.value.trim().split("/");
	if (!/^[\w-]+$/.test(owner ?? "") || !/^[\w.-]+$/.test(repo ?? "") || repo.endsWith(".git")) {
		status.textContent = "Enter the repository as owner/repo, such as kixel-ai/crabx.";
		return;
	}
	button.disabled = true;
	status.textContent = "Checking the repository...";
	try {
		// Unauthenticated, so GitHub rate-limits it per visitor: two calls per use.
		const get = async (url) => {
			const response = await fetch(url, { headers: { Accept: "application/vnd.github+json" } });
			if (response.status === 404) throw new Error("GitHub has no public repository by that name.");
			if (!response.ok) throw new Error(`GitHub's API answered ${response.status}; try again later.`);
			return response.json();
		};
		const api = `https://api.github.com/repos/${owner}/${repo}`;
		const info = await get(api);
		const tree = await get(`${api}/git/trees/${encodeURIComponent(info.default_branch)}?recursive=1`);
		const refused = refusal(info, tree);
		if (refused) throw new Error(refused);
		const template = await fetch("binaries.yml");
		if (!template.ok) throw new Error(`This site's workflow template answered ${template.status}.`);
		const contents = workflow(await template.text(), info.default_branch);
		status.textContent = "Opening GitHub's editor...";
		// info.full_name has the canonical case, which the editor's URL is exact about.
		const [name, slug] = info.full_name.split("/");
		location.assign(editorUrl(name, slug, info.default_branch, contents));
	} catch (error) {
		status.textContent = error instanceof TypeError ? "Could not reach GitHub's API; check your connection." : error.message;
	} finally {
		button.disabled = false;
	}
});
