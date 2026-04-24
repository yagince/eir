export type Mod = "cmd" | "ctrl" | "cmdOrCtrl" | "shift" | "alt";

export type Shortcut = {
  key: string;
  mods?: Mod[];
  when?: () => boolean;
  allowInInput?: boolean;
  run: () => void | Promise<void>;
};

function normalizeKey(key: string): string {
  return key.length === 1 ? key.toLowerCase() : key;
}

function isLetter(key: string): boolean {
  return key.length === 1 && key >= "a" && key <= "z";
}

export function matchShortcut(e: KeyboardEvent, def: Shortcut): boolean {
  const key = normalizeKey(e.key);
  if (key !== def.key) return false;

  const mods = new Set(def.mods ?? []);
  const wantCmdOrCtrl = mods.has("cmdOrCtrl");
  const wantCmd = mods.has("cmd");
  const wantCtrl = mods.has("ctrl");
  const wantShift = mods.has("shift");
  const wantAlt = mods.has("alt");

  if (wantCmdOrCtrl) {
    if (!(e.metaKey || e.ctrlKey)) return false;
  } else {
    if (e.metaKey !== wantCmd) return false;
    if (e.ctrlKey !== wantCtrl) return false;
  }

  // Shift controls letter case; when not explicitly required, ignore it for
  // letter keys so "u" and "U" both match. For symbols like "/", enforce
  // strictly — Shift+/ produces "?", a different binding.
  if (wantShift) {
    if (!e.shiftKey) return false;
  } else if (!isLetter(key) && e.shiftKey) {
    return false;
  }

  if (e.altKey !== wantAlt) return false;
  return true;
}

export function isEditableTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  const tag = el?.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

export function dispatchShortcut(e: KeyboardEvent, defs: Shortcut[]): boolean {
  const inField = isEditableTarget(e.target);
  for (const def of defs) {
    if (!matchShortcut(e, def)) continue;
    if (inField && !def.allowInInput) continue;
    if (def.when && !def.when()) continue;
    e.preventDefault();
    void def.run();
    return true;
  }
  return false;
}
