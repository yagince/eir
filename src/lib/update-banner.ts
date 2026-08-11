/// What the banner above the list should say, or `null` for "show nothing".
export type UpdateBanner =
  | { state: "available"; version: string }
  | { state: "downloading" }
  | { state: "failed" };

export type UpdateBannerInput = {
  /// Version on deck, or null when no update is waiting.
  availableVersion: string | null;
  downloading: boolean;
  /// Set only by a failed *install*. Check failures are routine (closed laptop,
  /// no network) and deliberately do not reach the banner — they stay in
  /// Settings, or every popup open on a plane would show a scary bar.
  installError: string | null;
  /// The version whose banner the user dismissed, if any.
  dismissedVersion: string | null;
};

/// Precedence: an install in flight outranks everything, then a failed install,
/// then an offer to update. Dismissal is per-version, so silencing one release
/// says nothing about the next.
export function updateBannerFor({
  availableVersion,
  downloading,
  installError,
  dismissedVersion,
}: UpdateBannerInput): UpdateBanner | null {
  if (downloading) return { state: "downloading" };
  if (installError) return { state: "failed" };
  if (availableVersion !== null && availableVersion !== dismissedVersion) {
    return { state: "available", version: availableVersion };
  }
  return null;
}
