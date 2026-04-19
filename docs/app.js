// Fetch the latest release metadata and rewrite the CTA buttons to point at the
// exact .dmg assets, so visitors get the binary in one click instead of landing
// on the releases page and scanning for the right file. Falls back silently to
// the generic /releases/latest link if the API is rate-limited or unreachable.
(async function updateDownloadLinks() {
  const versionEl = document.getElementById("version-line");
  const arm64Btn = document.getElementById("download-arm64");
  const intelBtn = document.getElementById("download-intel");

  try {
    const res = await fetch(
      "https://api.github.com/repos/yagince/eir/releases/latest",
      { headers: { Accept: "application/vnd.github+json" } },
    );
    if (!res.ok) throw new Error("GitHub API returned " + res.status);
    const release = await res.json();

    const assets = release.assets || [];
    const arm64 = assets.find(
      (a) => /aarch64/i.test(a.name) && a.name.endsWith(".dmg"),
    );
    const intel = assets.find(
      (a) => /x64|x86_64/i.test(a.name) && a.name.endsWith(".dmg"),
    );

    if (arm64) arm64Btn.href = arm64.browser_download_url;
    if (intel) intelBtn.href = intel.browser_download_url;

    versionEl.textContent = "Latest release: " + release.tag_name;
  } catch (err) {
    versionEl.textContent = "Latest release: see GitHub";
  }
})();

// Clipboard helper for the xattr command block.
document.querySelectorAll(".copy-btn").forEach((btn) => {
  btn.addEventListener("click", async () => {
    const target = document.getElementById(btn.dataset.copyTarget);
    if (!target) return;
    try {
      await navigator.clipboard.writeText(target.textContent);
    } catch {
      return;
    }
    const original = btn.textContent;
    btn.textContent = "Copied";
    btn.classList.add("copied");
    setTimeout(() => {
      btn.textContent = original;
      btn.classList.remove("copied");
    }, 1500);
  });
});
