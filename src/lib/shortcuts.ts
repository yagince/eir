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

/// `cmdOrCtrl` accepts either key; otherwise both must match the binding
/// exactly, so Cmd+U doesn't fire a Ctrl+U binding.
function commandKeysMatch(e: KeyboardEvent, mods: Set<Mod>): boolean {
  if (mods.has("cmdOrCtrl")) return e.metaKey || e.ctrlKey;
  return e.metaKey === mods.has("cmd") && e.ctrlKey === mods.has("ctrl");
}

/// Shift controls letter case, so when a binding doesn't ask for it we ignore it
/// on letter keys and "u" and "U" both match. Symbols are strict: Shift+/
/// produces "?", which is a different binding.
function shiftMatches(e: KeyboardEvent, key: string, wantShift: boolean): boolean {
  if (wantShift) return e.shiftKey;
  return isLetter(key) || !e.shiftKey;
}

export function matchShortcut(e: KeyboardEvent, def: Shortcut): boolean {
  const key = normalizeKey(e.key);
  if (key !== def.key) return false;

  const mods = new Set(def.mods ?? []);
  if (!commandKeysMatch(e, mods)) return false;
  if (!shiftMatches(e, key, mods.has("shift"))) return false;
  return e.altKey === mods.has("alt");
}

export function isEditableTarget(target: EventTarget | null): boolean {
  const el = target as HTMLElement | null;
  const tag = el?.tagName;
  return tag === "INPUT" || tag === "TEXTAREA" || tag === "SELECT";
}

// Text-caret targets — places where arrow keys move the caret natively and
// must not be hijacked. Narrower than `isEditableTarget`: SELECT and checkbox
// INPUTs are fine to intercept for focus navigation.
export function isTextCaretTarget(target: EventTarget | null): boolean {
  if (target instanceof HTMLTextAreaElement) return true;
  if (target instanceof HTMLInputElement) {
    const type = target.type;
    return type !== "checkbox" && type !== "radio";
  }
  return false;
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
