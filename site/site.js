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
