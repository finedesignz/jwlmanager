import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import type { JwlCoreStatus } from "../bindings/JwlCoreStatus";

/**
 * Informational jwlCore capability notice (D-13a). Calls `check_jwlcore`
 * (01-03) once after mount and renders the muted, dismissible notice ONLY
 * when the unified `JwlCoreStatus.loaded === false` — the arm64-Windows
 * no-binary case is `Ok`, not `Err`, and is expected/known, never a fault.
 * Renders nothing when `loaded === true`, and nothing on a genuine
 * `check_jwlcore` failure (an actual load fault is out of this phase's
 * scope to surface — merge itself is a Phase 5 feature).
 */
export default function JwlCoreNotice() {
  const [status, setStatus] = useState<JwlCoreStatus | null>(null);
  const [dismissed, setDismissed] = useState(false);

  useEffect(() => {
    let cancelled = false;
    invoke<JwlCoreStatus>("check_jwlcore")
      .then((result) => {
        if (!cancelled) {
          setStatus(result);
        }
      })
      .catch(() => {
        // Intentionally swallowed: a genuine JwlCoreError here is not a
        // fault the Walking Skeleton (open/view/save) needs to surface.
      });
    return () => {
      cancelled = true;
    };
  }, []);

  if (dismissed || status === null || status.loaded) {
    return null;
  }

  return (
    <div className="jwlcore-notice" role="status" data-testid="jwlcore-notice">
      <span>
        Merge isn&rsquo;t available on this device
        {status.reason ? ` (${status.reason})` : ""}. Everything else —
        opening, viewing, and saving archives — works normally.
      </span>
      <button
        type="button"
        className="jwlcore-notice-dismiss"
        onClick={() => setDismissed(true)}
        aria-label="Dismiss jwlCore capability notice"
      >
        ×
      </button>
    </div>
  );
}
