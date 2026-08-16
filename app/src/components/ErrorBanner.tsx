import type { ErrorDto } from "../bindings/ErrorDto";
import { describeError, isZipSlipRejection } from "../lib/errors";
import { useI18n } from "../i18n/I18nContext";

/**
 * Renders a sanitized `ErrorDto` (SAFE-05) as the actionable sentence from
 * 01-UI-SPEC.md's Copywriting Contract. Sticky by construction — the parent
 * (`App.tsx`) only clears `error` on the next user-initiated action, never on
 * a timer, so real errors are never silently auto-dismissed. The zip-slip
 * rejection renders with a distinct security modifier class, not the generic
 * failure styling.
 *
 * Every `ErrorBanner` instance in the tree sits below `I18nProvider`
 * (App.tsx / SettingsProvider.tsx, 11-03-PLAN.md), so this is the ONE call
 * site that pulls `t` from `useI18n()` and passes it into `describeError`
 * (11-04-PLAN.md) — `describeError` itself is a plain lib function, not a
 * component, and cannot call `useI18n()` on its own.
 */
export default function ErrorBanner({ error }: { error: ErrorDto }) {
  const { t } = useI18n();
  const zipSlip = isZipSlipRejection(error);
  const className = zipSlip ? "error-banner error-banner-security" : "error-banner";

  return (
    <div className={className} role="alert" data-testid="error-banner">
      {describeError(error, t)}
      {error.safe_file_name ? ` (${error.safe_file_name})` : ""}
    </div>
  );
}
