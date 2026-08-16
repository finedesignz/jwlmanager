import type { ErrorDto } from "../bindings/ErrorDto";
import type { useI18n } from "../i18n/I18nContext";

/** The exact `t` shape `useI18n()` returns, borrowed via `ReturnType` rather
 * than duplicating the signature -- `describeError` is a plain lib function
 * (not a component), so it cannot call `useI18n()` itself; every call site
 * passes its own `t` explicitly (11-04-PLAN.md). */
type TFunction = ReturnType<typeof useI18n>["t"];

/**
 * Maps every sanitized `ErrorDto` (code/operation/safe_file_name/message_key —
 * the ONLY error shape crossing IPC, per 01-07's error.rs `to_dto`) to the
 * exact actionable sentence pattern from 01-UI-SPEC.md's Copywriting
 * Contract: what happened (verb-led) + why (typed reason) + what to do next.
 *
 * Keyed off `code` (the stable snake_case string `ArchiveError::to_dto`
 * emits, e.g. "not_a_zip", "zip_slip_rejected") — NEVER a raw Rust `Display`
 * string. Every known code has a specific sentence; an unrecognized code
 * still gets a generic-but-actionable sentence (never blank, never the bare
 * code/message_key alone).
 *
 * Every branch resolves through the `errors.*` catalog (11-04-PLAN.md) —
 * never a literal string. `describeError full coverage`
 * (app/src/lib/errors.test.ts) derives the real code list from
 * `src-tauri/src/error.rs`/`settings.rs`'s `to_dto` match arms at test time
 * and asserts every one of them resolves here.
 */
export function describeError(err: ErrorDto, t: TFunction): string {
  switch (err.code) {
    case "not_a_zip":
      return t("errors.notAZip");
    case "missing_manifest":
      return t("errors.missingManifest");
    case "missing_user_data_backup":
      return t("errors.missingUserDataBackup");
    case "schema_too_old":
      return t("errors.schemaTooOld");
    case "schema_too_new":
      return t("errors.schemaTooNew");
    case "schema_upgrade_failed":
      return t("errors.schemaUpgradeFailed");
    case "schema_downgrade_failed":
      return t("errors.schemaDowngradeFailed");
    case "trim_failed":
      return t("errors.trimFailed");
    case "zip_slip_rejected":
      return t("errors.zipSlipRejected");
    case "state_poisoned":
      return t("errors.statePoisoned");
    case "missing_resources_language":
      return t("errors.missingResourcesLanguage");
    case "missing_resources_db":
      return t("errors.missingResourcesDb");
    case "io_error":
      return t("errors.ioError");
    case "sqlite_error":
      return t("errors.sqliteError");
    case "zip_error":
      return t("errors.zipError");
    case "json_error":
      return t("errors.jsonError");
    case "delete_failed":
      return t("errors.deleteFailed");
    case "favorite_failed":
      return t("errors.favoriteFailed");
    case "favorite_duplicate":
      return t("errors.favoriteDuplicate");
    case "color_failed":
      return t("errors.colorFailed");
    case "tag_failed":
      return t("errors.tagFailed");
    case "reorder_failed":
      return t("errors.reorderFailed");
    case "clean_failed":
      return t("errors.cleanFailed");
    case "mask_failed":
      return t("errors.maskFailed");
    case "record_edit_failed":
      return t("errors.recordEditFailed");
    case "merge_unavailable":
      return t("errors.mergeUnavailable");
    case "merge_failed":
      return t("errors.mergeFailed");
    case "export_failed":
      return t("errors.exportFailed");
    case "import_malformed":
      return t("errors.importMalformed");
    case "import_failed":
      return t("errors.importFailed");
    case "playlist_export_failed":
      return t("errors.playlistExportFailed");
    case "playlist_import_failed":
      return t("errors.playlistImportFailed");
    case "media_add_failed":
      return t("errors.mediaAddFailed");
    case "media_unsupported_format":
      return t("errors.mediaUnsupportedFormat");
    case "media_delete_failed":
      return t("errors.mediaDeleteFailed");
    case "settings_app_data_dir_unavailable":
      return t("errors.settingsAppDataDirUnavailable");
    case "settings_write_failed":
      return t("errors.settingsWriteFailed");
    case "settings_read_failed":
      return t("errors.settingsReadFailed");
    case "settings_parse_failed":
      return t("errors.settingsParseFailed");
    default:
      return t("errors.default", { operation: err.operation });
  }
}

/** True when this ErrorDto is the zip-slip security rejection (distinct copy, not the generic open-failure sentence). */
export function isZipSlipRejection(err: ErrorDto): boolean {
  return err.code === "zip_slip_rejected";
}
