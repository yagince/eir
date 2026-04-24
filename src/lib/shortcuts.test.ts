// @vitest-environment jsdom
import { describe, expect, it, vi } from "vitest";
import {
  dispatchShortcut,
  isTextCaretTarget,
  matchShortcut,
  type Shortcut,
} from "./shortcuts";

function event(
  key: string,
  opts: Partial<Pick<KeyboardEvent, "metaKey" | "ctrlKey" | "shiftKey" | "altKey">> & {
    target?: EventTarget | null;
  } = {},
): KeyboardEvent {
  return {
    key,
    metaKey: opts.metaKey ?? false,
    ctrlKey: opts.ctrlKey ?? false,
    shiftKey: opts.shiftKey ?? false,
    altKey: opts.altKey ?? false,
    target: opts.target ?? null,
    preventDefault: vi.fn(),
  } as unknown as KeyboardEvent;
}

describe("matchShortcut", () => {
  it("matches a plain letter key case-insensitively", () => {
    const def: Shortcut = { key: "u", run: () => {} };
    expect(matchShortcut(event("u"), def)).toBe(true);
    expect(matchShortcut(event("U", { shiftKey: true }), def)).toBe(true);
  });

  it("rejects a plain symbol when shift is held (different character)", () => {
    const def: Shortcut = { key: "/", run: () => {} };
    expect(matchShortcut(event("/"), def)).toBe(true);
    expect(matchShortcut(event("?", { shiftKey: true }), def)).toBe(false);
  });

  it("requires explicit shift when mods include it", () => {
    const def: Shortcut = { key: "a", mods: ["cmdOrCtrl", "shift"], run: () => {} };
    expect(matchShortcut(event("A", { metaKey: true, shiftKey: true }), def)).toBe(true);
    expect(matchShortcut(event("a", { metaKey: true }), def)).toBe(false);
  });

  it("cmdOrCtrl matches either modifier", () => {
    const def: Shortcut = { key: "r", mods: ["cmdOrCtrl"], run: () => {} };
    expect(matchShortcut(event("r", { metaKey: true }), def)).toBe(true);
    expect(matchShortcut(event("r", { ctrlKey: true }), def)).toBe(true);
    expect(matchShortcut(event("r"), def)).toBe(false);
  });

  it("cmd-only rejects ctrl-only presses", () => {
    const def: Shortcut = { key: ",", mods: ["cmd"], run: () => {} };
    expect(matchShortcut(event(",", { metaKey: true }), def)).toBe(true);
    expect(matchShortcut(event(",", { ctrlKey: true }), def)).toBe(false);
  });

  it("rejects unwanted modifiers when mods are empty", () => {
    const def: Shortcut = { key: "u", run: () => {} };
    expect(matchShortcut(event("u", { metaKey: true }), def)).toBe(false);
    expect(matchShortcut(event("u", { altKey: true }), def)).toBe(false);
  });

  it("matches named keys like Enter or Backspace", () => {
    const def: Shortcut = { key: "Backspace", run: () => {} };
    expect(matchShortcut(event("Backspace"), def)).toBe(true);
    expect(matchShortcut(event("Enter"), def)).toBe(false);
  });
});

describe("isTextCaretTarget", () => {
  it("returns true for textareas and text-type inputs", () => {
    const textarea = document.createElement("textarea");
    const text = document.createElement("input");
    text.type = "text";
    const search = document.createElement("input");
    search.type = "search";
    expect(isTextCaretTarget(textarea)).toBe(true);
    expect(isTextCaretTarget(text)).toBe(true);
    expect(isTextCaretTarget(search)).toBe(true);
  });

  it("returns false for checkboxes, radios, buttons, selects", () => {
    const check = document.createElement("input");
    check.type = "checkbox";
    const radio = document.createElement("input");
    radio.type = "radio";
    const btn = document.createElement("button");
    const select = document.createElement("select");
    expect(isTextCaretTarget(check)).toBe(false);
    expect(isTextCaretTarget(radio)).toBe(false);
    expect(isTextCaretTarget(btn)).toBe(false);
    expect(isTextCaretTarget(select)).toBe(false);
    expect(isTextCaretTarget(null)).toBe(false);
  });
});

describe("dispatchShortcut", () => {
  function input(): HTMLElement {
    return { tagName: "INPUT" } as HTMLElement;
  }

  it("runs the matching shortcut and preventDefaults", () => {
    const run = vi.fn();
    const e = event("u");
    const handled = dispatchShortcut(e, [{ key: "u", run }]);
    expect(handled).toBe(true);
    expect(run).toHaveBeenCalledOnce();
    expect(e.preventDefault).toHaveBeenCalledOnce();
  });

  it("skips shortcuts when focus is in a text field", () => {
    const run = vi.fn();
    const e = event("u", { target: input() });
    const handled = dispatchShortcut(e, [{ key: "u", run }]);
    expect(handled).toBe(false);
    expect(run).not.toHaveBeenCalled();
    expect(e.preventDefault).not.toHaveBeenCalled();
  });

  it("runs allowInInput shortcuts even from a text field", () => {
    const run = vi.fn();
    const e = event(",", { metaKey: true, target: input() });
    const handled = dispatchShortcut(e, [
      { key: ",", mods: ["cmd"], allowInInput: true, run },
    ]);
    expect(handled).toBe(true);
    expect(run).toHaveBeenCalledOnce();
  });

  it("when guard returning falsy lets the key fall through", () => {
    const run = vi.fn();
    const e = event("u");
    const handled = dispatchShortcut(e, [{ key: "u", when: () => false, run }]);
    expect(handled).toBe(false);
    expect(run).not.toHaveBeenCalled();
    expect(e.preventDefault).not.toHaveBeenCalled();
  });

  it("stops at the first matching shortcut", () => {
    const first = vi.fn();
    const second = vi.fn();
    dispatchShortcut(event("u"), [
      { key: "u", run: first },
      { key: "u", run: second },
    ]);
    expect(first).toHaveBeenCalledOnce();
    expect(second).not.toHaveBeenCalled();
  });
});
