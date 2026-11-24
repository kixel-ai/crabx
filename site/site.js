// Enhancements only: without this script the tabs still switch and the commands still select on click.

// Pick the install command for this OS.
const platform = navigator.userAgentData?.platform ?? navigator.platform ?? "";
if (/^win/i.test(platform)) document.getElementById("os-windows").checked = true;

// Copy the visible command.
const copy = document.querySelector(".copy");
copy.hidden = false;
copy.addEventListener("click", async () => {
	const windows = document.getElementById("os-windows").checked;
	const command = document.querySelector(windows ? ".windows .sel" : ".unix .sel");
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
