import { isPermissionGranted, requestPermission } from "@tauri-apps/plugin-notification";
import { openUrl } from "@tauri-apps/plugin-opener";

/// What a click should do. `onClick` wins over `url` when both are given.
export type NotifyActivation = {
  url?: string;
  onClick?: () => void;
};

/// The slice of the Notification API this module is allowed to touch.
///
/// It is deliberately this small. `tauri-plugin-notification` replaces
/// `window.Notification` with its own shim, which carries `onclick` and `close`
/// but is **not** an EventTarget — calling `addEventListener` on it throws
/// "is not a function" at runtime. v0.17.7 and v1.0.0 shipped with exactly that
/// bug after a lint suggested the "modern" form, and because the throw happened
/// mid update-check it also took the update banner down with it.
///
/// Keeping the type this narrow means reaching for anything richer fails to
/// compile, and the tests substitute a fake of the same shape so a future
/// refactor toward addEventListener fails there rather than in someone's tray.
type NotificationShim = {
  onclick: (() => void) | null;
  close: () => void;
};

type NotificationCtor = new (title: string, options: { body: string }) => NotificationShim;

/// Ask once, and only escalate to a prompt when the permission isn't already
/// granted.
export async function ensureNotificationPermission(): Promise<boolean> {
  if (await isPermissionGranted()) return true;
  return (await requestPermission()) === "granted";
}

/// Post a desktop notification.
///
/// The plugin's own sendNotification() invokes `new window.Notification(...)`
/// and throws the instance away, and its desktop backend never emits the
/// `actionPerformed` event `onAction` waits for (that wiring only exists on
/// iOS/Android). So constructing it here is the only way to handle a click.
export function showNotification(title: string, body: string, onActivate: NotifyActivation = {}) {
  const Ctor = globalThis.Notification as unknown as NotificationCtor;
  const n = new Ctor(title, { body });
  const { url, onClick } = onActivate;
  if (!onClick && !url) return;
  // oxlint-disable-next-line unicorn/prefer-add-event-listener -- see NotificationShim
  n.onclick = () => {
    if (onClick) onClick();
    else if (url) void openUrl(url);
    n.close();
  };
}
