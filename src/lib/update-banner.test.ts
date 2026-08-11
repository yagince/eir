import { describe, expect, it } from "vitest";
import { updateBannerFor, type UpdateBannerInput } from "$lib/update-banner";

function input(overrides: Partial<UpdateBannerInput> = {}): UpdateBannerInput {
  return {
    availableVersion: null,
    downloading: false,
    installError: null,
    dismissedVersion: null,
    ...overrides,
  };
}

describe("updateBannerFor", () => {
  it("shows nothing when no update is waiting", () => {
    expect(updateBannerFor(input())).toBeNull();
  });

  it("offers the available version", () => {
    expect(updateBannerFor(input({ availableVersion: "1.0.2" }))).toEqual({
      state: "available",
      version: "1.0.2",
    });
  });

  it("stays quiet about a version the user dismissed", () => {
    expect(
      updateBannerFor(input({ availableVersion: "1.0.2", dismissedVersion: "1.0.2" })),
    ).toBeNull();
  });

  it("speaks up again for a version newer than the dismissed one", () => {
    // The point of per-version dismissal: "not now" applies to that release,
    // not to updating forever.
    expect(
      updateBannerFor(input({ availableVersion: "1.1.0", dismissedVersion: "1.0.2" })),
    ).toEqual({ state: "available", version: "1.1.0" });
  });

  it("reports an install in progress ahead of anything else", () => {
    expect(
      updateBannerFor(
        input({ availableVersion: "1.0.2", downloading: true, installError: "boom" }),
      ),
    ).toEqual({ state: "downloading" });
  });

  it("reports a failed install even once the offer is gone", () => {
    // updateStatus flips to error on failure, so availableVersion is null by
    // then; the banner must still say something rather than vanishing and
    // leaving the user wondering whether the click registered.
    expect(updateBannerFor(input({ installError: "network unreachable" }))).toEqual({
      state: "failed",
    });
  });

  it("keeps a dismissed version dismissed while an install error is showing", () => {
    // The failed state outranks dismissal — the user asked for this install.
    expect(
      updateBannerFor(
        input({
          availableVersion: "1.0.2",
          dismissedVersion: "1.0.2",
          installError: "boom",
        }),
      ),
    ).toEqual({ state: "failed" });
  });

  it("ignores a check failure, which never reaches the banner", () => {
    // installError is only ever set by a failed install. A failed *check* leaves
    // it null, so an offline laptop doesn't grow a red bar on every popup open.
    expect(updateBannerFor(input({ availableVersion: null, installError: null }))).toBeNull();
  });
});
