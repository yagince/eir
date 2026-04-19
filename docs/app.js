// Fetch the latest release metadata and rewrite the CTA buttons to point at
// the exact per-platform assets, so visitors get the binary in one click
// instead of landing on the releases page and scanning for the right file.
// Falls back silently to the generic /releases/latest link if the API is
// rate-limited or unreachable.
(async function updateDownloadLinks() {
  const versionEl = document.getElementById("version-line");

  // Each entry: [button id, predicate matching the asset filename]. Order the
  // checks from most specific to least so macOS arm64 isn't mis-matched as
  // "generic mac" etc.
  const matchers = [
    ["download-arm64", (n) => /aarch64/i.test(n) && n.endsWith(".dmg")],
    [
      "download-intel",
      (n) => /(x64|x86_64)/i.test(n) && n.endsWith(".dmg"),
    ],
    [
      "download-windows",
      (n) => /windows|\.msi$|setup\.exe$/i.test(n) && !n.endsWith(".sig"),
    ],
    [
      "download-linux",
      (n) =>
        (/\.AppImage$/i.test(n) || /\.deb$/i.test(n)) && !n.endsWith(".sig"),
    ],
  ];

  try {
    const res = await fetch(
      "https://api.github.com/repos/yagince/eir/releases/latest",
      { headers: { Accept: "application/vnd.github+json" } },
    );
    if (!res.ok) throw new Error("GitHub API returned " + res.status);
    const release = await res.json();

    const assets = release.assets || [];
    for (const [id, predicate] of matchers) {
      const btn = document.getElementById(id);
      if (!btn) continue;
      const asset = assets.find((a) => predicate(a.name));
      if (asset) btn.href = asset.browser_download_url;
    }

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
