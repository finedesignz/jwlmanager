import type { ErrorDto } from "../bindings/ErrorDto";
import { describeError, isZipSlipRejection } from "../lib/errors";

/**
 * Renders a sanitized `ErrorDto` (SAFE-05) as the actionable sentence from
 * 01-UI-SPEC.md's Copywriting Contract. Sticky by construction — the parent
 * (`App.tsx`) only clears `error` on the next user-initiated action, never on
 * a timer, so real errors are never silently auto-dismissed. The zip-slip
 * rejection renders with a distinct security modifier class, not the generic
 * failure styling.
 */
export default function ErrorBanner({ error }: { error: ErrorDto }) {
  const zipSlip = isZipSlipRejection(error);
  const className = zipSlip ? "error-banner error-banner-security" : "error-banner";

  return (
    <div className={className} role="alert" data-testid="error-banner">
      {describeError(error)}
      {error.safe_file_name ? ` (${error.safe_file_name})` : ""}
    </div>
  );
}
