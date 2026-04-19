---
name: tray-icon-design
description: Design, generate, and deploy the macOS menubar tray icon for eir. Use this skill whenever the user asks to update, redesign, regenerate, or improve the tray icon / menubar icon / システムトレイアイコン / メニューバーアイコン — including vague phrases like "アイコンがダサい", "tray icon を変えたい", "こういう雰囲気のアイコンにしたい", "icon に羽足したい", or whenever new icon variants need to be explored. The skill walks through direction proposals, chroma-key image generation via nanobanana, threshold-based binarisation for macOS template-mode silhouettes, and deployment to src-tauri/icons/tray-icon.png.
---

# Tray icon design for eir

The tray icon lives at `src-tauri/icons/tray-icon.png` and is embedded at compile time via `include_bytes!`. It is rendered in macOS **template mode** — the OS treats opaque pixels as black/white silhouette and retints them per theme. That means the PNG must be a **clean black-on-transparent silhouette with no partial alpha**. Partial alpha (from antialiased green chroma-key edges or JPEG-style generation artefacts) shows up as a ghostly halo when macOS tints.

The source PNG that the deployed file comes from lives at `assets/icon-source/menubar-icon.png`. Keep the two in sync so the next person redesigning can start from the current deployed state.

## Workflow overview

Total time per iteration: ~5 minutes of thinking + ~2 minutes of compute.

1. **Propose 2–3 design directions** (text-first, no generation yet)
2. **Generate** each direction with nanobanana Pro on a pure-green chroma-key background
3. **Review together** — show rendered PNGs inline; let the user reject / refine / combine
4. **Iterate** — re-propose variations of the favourites
5. **Chroma-key + trim + resize** once a winner is picked
6. **Deploy** to `src-tauri/icons/tray-icon.png` and `assets/icon-source/menubar-icon.png`
7. **Verify** with `cd src-tauri && cargo check`, and ask the user to run `pnpm tauri dev` for the visual check

Do not commit deployed icon changes until the user has visually confirmed the menubar rendering — template-mode surprises only surface at ~22 pt, not in the source PNG.

## Step 1 — Direction proposals

Before burning compute, lay out 2–3 distinct visual directions in plain text with a short note on what each trades off. The eir identity to preserve:

- **Eye** — the "watcher" semantics of a PR / Issue watcher
- **Feather** — Eir (the Norse goddess of healing) + evokes a quill / writing
- **Silhouette** — must be readable at ~22 pt in a menubar, so 2–3 elements max, no thin lines, no fine detail

Mention menubar-specific pitfalls up-front:

- Anything **below the eye** tends to read as "bags under the eye". Avoid.
- Closed / downcast eyes lose the "watching" semantic. Avoid unless the user explicitly opts into a more tranquil direction.
- Very detailed feather barbs (peacock style) get eaten at 22 pt — the full silhouette reduces to a blob.

Prefer compositions that put visual interest **above or beside** the eye, not below.

## Step 2 — Generate with nanobanana

Use nanobanana's Pro model for production icons (higher fidelity):

```bash
NANOBANANA_MODEL=gemini-3-pro-image-preview gemini -y -p '/icon <prompt>'
```

### Prompt template

Reuse this shape — fill in the specific composition for each direction:

```
Monochrome black silhouette on a SOLID PURE GREEN #00FF00 chroma-key
background. <describe the composition in 1–2 sentences, e.g. "A single
vertical feather above an open almond eye with a round pupil">. Bold
flat-design symbolic icon for macOS menubar. Strong black silhouette,
two colors only (solid black and pure green #00FF00), no gradients,
no soft edges, no fine details. Perfect 1:1 square, 1024x1024,
centered composition with comfortable margin. Must read clearly when
scaled down very small.
```

Avoid phrases like **"22 pixel size"** or **"at 22px"** in the prompt — the Gemini CLI has tried to interpret bare number+unit combinations as option values (`--sizes 22` is not valid) and aborted generation. "Scaled down very small" works fine.

### Parallel generation + flat output

Run multiple directions in parallel via `run_in_background: true` Bash calls, each in its own `_gen_<letter>/` subdir. nanobanana writes into `<cwd>/nanobanana-output/<auto-filename>.png` regardless of what the prompt requested for a filename — after each background task completes, move the file to a flat location:

```bash
mv _gen_a/nanobanana-output/*.png A-eye-of-horus.png
rm -rf _gen_a
```

Keep the exploration directory flat (`assets/tray-icon-v2/A.png`, `B.png`, …) — nested subdirs are a drag to preview. Delete the exploration directory entirely once a winner ships.

## Step 3 — Review with the user

Read the PNGs via the Read tool so the user sees them inline. For each, write one line on what the nanobanana output actually looks like (it often differs from the prompt — call out drift) and one line of your own read on how it will scale to menubar.

Make it easy to reject: say "A is X, B is Y" rather than asking the user to describe each themselves.

## Step 4 — Iterate

The first batch almost never wins outright. Patterns that come up:

- User likes **elements from multiple outputs** — combine into a new prompt ("keep B's feather shape, C's open eye")
- User wants to "重ねてみる" (overlap the eye and feather) — these layered compositions often produce the strongest final silhouette
- Outputs look good at 1024×1024 but read as cluttered — propose stripping details (fewer barbs, dropping secondary elements)

Each iteration is cheap — do 2–3 rounds before settling.

## Step 5 — Chroma-key + trim + resize

Once a winning PNG is picked, turn it into a deployable tray icon. **Use threshold-based binarisation, not fuzz-based chroma-key** — fuzz leaves partial alpha at antialiased edges, threshold produces a strict 1-bit silhouette that macOS's template mode tints cleanly:

```bash
magick <winner>.png \
  -colorspace gray -threshold 50% \
  -transparent white \
  -trim +repage \
  -resize 1010x1010 \
  -background none -gravity center -extent 1024x1024 \
  cleaned-1024.png

magick cleaned-1024.png -resize 512x512 tray-512.png
```

Notes:

- `-colorspace gray -threshold 50%` flattens to pure black and white. Pure green's gray value is ~150 (>128), so it becomes white; pure black stays black.
- `-transparent white` makes the white (former green) transparent. Only black pixels remain opaque — exactly what template mode wants.
- `1010x1010` content inside a `1024x1024` canvas is a tight margin (~7 px per side). macOS adds its own breathing room around menubar items, so more padding than this wastes icon area. `960x960` or smaller looks visibly small on the menubar.
- Final deploy size is **512x512**. Tauri scales down from there at runtime.

### Verify transparency

Always sanity-check before deploying — a mistake here is a ghostly halo that you won't notice until after shipping:

```bash
magick tray-512.png -background magenta -alpha remove check-magenta.png
```

Open `check-magenta.png` and confirm the silhouette is cleanly black-on-magenta with no grey halo around the edges, especially at the four corners.

## Step 6 — Deploy

Two locations, identical content:

```bash
cp tray-512.png src-tauri/icons/tray-icon.png
cp tray-512.png assets/icon-source/menubar-icon.png
```

The first is what gets `include_bytes!`d into the binary. The second is the reference source — future icon work starts from there, not from the (possibly rounded / compressed) bundled copy.

## Step 7 — Verify

```bash
cd src-tauri && cargo check
```

Then hand off to the user:

> `pnpm tauri dev` で menubar でどう見えるか確認してください。light / dark 両方と、22pt のデフォルトサイズで。問題あれば教えてください。

Do not commit until the user has visually confirmed. Template-mode rendering issues only show up at menubar size.

## Pitfalls seen in the wild

- **"Bags under the eye"** — any element curving below the eye outline reads as dark circles at 22 pt. Keep feathers/decorations above or beside.
- **Partial alpha halo** — if you used fuzz-based chroma-key instead of threshold, antialiased green pixels survive as semi-transparent grey. macOS tints them lighter than the main silhouette, creating a halo. Redo with threshold.
- **Padded-to-small** — generating at 1024×1024 with a big visual canvas margin (nanobanana sometimes adds its own) means the actual silhouette only fills ~60% of the frame. After trim+resize, the deployed icon looks tiny in the menubar. Always trim before resizing, and verify the result visually fills its 1024×1024 canvas before downscaling.
- **Numbers in prompts** — Gemini CLI can parse "22 pixel" as `--sizes 22` and refuse. Use "scaled down very small" or "optimised for small sizes" instead.
- **Nested `nanobanana-output/` dirs** — always flatten after generation, or reviewing a batch of candidates becomes click-fatigue.

## What goes in the git commit

- `src-tauri/icons/tray-icon.png` (deployed)
- `assets/icon-source/menubar-icon.png` (source, same file content)
- No exploration directory — delete `assets/tray-icon-v2/` (or wherever the candidates lived) before committing

Commit subject format: `feat: new tray icon — <one-line description>`. Body should call out what visual problem the redesign solves (e.g., the "bags under eye" that forced this round).
