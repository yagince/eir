import { beforeEach, describe, expect, it, vi } from "vitest";

const openUrl = vi.fn();
const isPermissionGranted = vi.fn();
const requestPermission = vi.fn();

vi.mock("@tauri-apps/plugin-opener", () => ({ openUrl: (url: string) => openUrl(url) }));
vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: () => isPermissionGranted(),
  requestPermission: () => requestPermission(),
}));

const { ensureNotificationPermission, showNotification } = await import("$lib/notify");

/// Stands in for `tauri-plugin-notification`'s window.Notification shim, which
/// carries onclick and close and nothing else. jsdom's built-in Notification is
/// a full EventTarget, so testing against *that* would happily accept
/// addEventListener and miss the very bug these tests exist to prevent.
class NotificationShimFake {
  static last: NotificationShimFake | null = null;
  onclick: (() => void) | null = null;
  closed = false;

  constructor(
    readonly title: string,
    readonly options: { body: string },
  ) {
    NotificationShimFake.last = this;
  }

  close() {
    this.closed = true;
  }

  click() {
    this.onclick?.();
  }
}

beforeEach(() => {
  vi.clearAllMocks();
  NotificationShimFake.last = null;
  // @ts-expect-error — deliberately narrower than lib.dom's Notification
  globalThis.Notification = NotificationShimFake;
});

describe("showNotification", () => {
  it("passes the title and body through to the notification", () => {
    showNotification("eir update available", "Version 1.0.1 is ready to install.");
    expect(NotificationShimFake.last?.title).toBe("eir update available");
    expect(NotificationShimFake.last?.options.body).toBe("Version 1.0.1 is ready to install.");
  });

  it("only uses onclick, never addEventListener", () => {
    // The regression guard: a Notification with no EventTarget surface at all.
    // Anything reaching past onclick/close throws instead of silently working
    // in jsdom and failing in the app.
    showNotification("t", "b", { onClick: () => {} });
    expect(NotificationShimFake.last?.onclick).toBeInstanceOf(Function);
  });

  it("leaves onclick unset when there is nothing to activate", () => {
    showNotification("t", "b");
    expect(NotificationShimFake.last?.onclick).toBeNull();
  });

  it("runs the callback and closes the notification on click", () => {
    const onClick = vi.fn();
    showNotification("t", "b", { onClick });
    NotificationShimFake.last?.click();
    expect(onClick).toHaveBeenCalledOnce();
    expect(NotificationShimFake.last?.closed).toBe(true);
    expect(openUrl).not.toHaveBeenCalled();
  });

  it("opens the url on click when no callback is given", () => {
    showNotification("t", "b", { url: "https://github.com/yagince/eir" });
    NotificationShimFake.last?.click();
    expect(openUrl).toHaveBeenCalledWith("https://github.com/yagince/eir");
    expect(NotificationShimFake.last?.closed).toBe(true);
  });

  it("prefers the callback over the url when both are given", () => {
    const onClick = vi.fn();
    showNotification("t", "b", { onClick, url: "https://example.com" });
    NotificationShimFake.last?.click();
    expect(onClick).toHaveBeenCalledOnce();
    expect(openUrl).not.toHaveBeenCalled();
  });
});

describe("ensureNotificationPermission", () => {
  it("does not prompt when permission is already granted", async () => {
    isPermissionGranted.mockResolvedValue(true);
    await expect(ensureNotificationPermission()).resolves.toBe(true);
    expect(requestPermission).not.toHaveBeenCalled();
  });

  it("prompts and reports the answer when permission is missing", async () => {
    isPermissionGranted.mockResolvedValue(false);
    requestPermission.mockResolvedValue("granted");
    await expect(ensureNotificationPermission()).resolves.toBe(true);

    requestPermission.mockResolvedValue("denied");
    await expect(ensureNotificationPermission()).resolves.toBe(false);
  });
});
