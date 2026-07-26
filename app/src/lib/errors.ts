import type { ErrorDto } from "../bindings/ErrorDto";

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
 */
export function describeError(err: ErrorDto): string {
  switch (err.code) {
    case "not_a_zip":
      return "Couldn't open this archive — the file isn't a valid .jwlibrary backup. Choose a different file or check that it hasn't been moved.";
    case "missing_manifest":
      return "Couldn't open this archive — it's missing the manifest.json file every .jwlibrary backup must contain. Choose a different file or check that it hasn't been moved.";
    case "missing_user_data_backup":
      return "Couldn't open this archive — it's missing the user_data backup every .jwlibrary backup must contain. Choose a different file or check that it hasn't been moved.";
    case "schema_too_old":
      return "Couldn't open this archive — it was created with a schema version too old for this app to open (the oldest supported version is 12). Choose a different file or use an older version of the app.";
    case "schema_too_new":
      return "Couldn't open this archive — it was created by a newer version of JW Library than this app supports. Update this app to the latest version, or choose a different file.";
    case "schema_upgrade_failed":
      return "Couldn't open this archive — upgrading its internal database format failed. The original file is unchanged. Choose a different file or try again.";
    case "schema_downgrade_failed":
      return "Couldn't downgrade this archive to the older format — some archives can't be converted without losing or conflicting data. Your original session is unchanged. Choose a different file or keep the current format.";
    case "zip_slip_rejected":
      return "This archive can't be opened — it contains files that try to write outside the extraction folder, which isn't safe. This file may be corrupted or tampered with.";
    case "state_poisoned":
      return "Something went wrong with this app's internal session state. Close and reopen the archive, then try again.";
    case "missing_resources_language":
      return "Couldn't open this archive — the language data needed to label its notes is missing from this app's bundled resources. Reinstall the app and try again.";
    case "missing_resources_db":
      return "Couldn't open this archive — this app's bundled resources database is missing. Reinstall the app and try again.";
    case "io_error":
      return "Couldn't complete this operation — a file system error occurred. Check that the file still exists, isn't open in another program, and that you have permission to access it, then try again.";
    case "sqlite_error":
      return "Couldn't read this archive's data — its internal database appears to be corrupted. Choose a different file or restore from an earlier backup.";
    case "zip_error":
      return "Couldn't read this archive's zip container — it may be corrupted. Choose a different file or restore from an earlier backup.";
    case "json_error":
      return "Couldn't read this archive's manifest — it isn't valid JSON. Choose a different file or restore from an earlier backup.";
    case "delete_failed":
      return "Couldn't delete the selected items — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.";
    case "favorite_failed":
      return "Couldn't add this favorite — the archive is unchanged. Try a different edition or try again.";
    case "favorite_duplicate":
      return "This edition is already marked as a favorite. Choose a different edition, or check your existing Favorites.";
    case "color_failed":
      return "Couldn't change the color of the selected items — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.";
    case "tag_failed":
      return "Couldn't update tags for the selected items — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.";
    case "reorder_failed":
      return "Couldn't sort tags — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.";
    case "clean_failed":
      return "Couldn't clean this archive — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.";
    case "mask_failed":
      return "Couldn't mask this archive — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.";
    case "merge_unavailable":
      return "Couldn't merge — the merge engine isn't available on this device (for example on Windows on Arm, which has no merge build yet). Your open archive is unchanged.";
    case "merge_failed":
      return "Couldn't merge these archives — the merge could not be completed and your open archive is unchanged. Choose a different source archive or try again.";
    default:
      return `Couldn't complete this operation (${err.operation}). Choose a different file or try again.`;
  }
}

/** True when this ErrorDto is the zip-slip security rejection (distinct copy, not the generic open-failure sentence). */
export function isZipSlipRejection(err: ErrorDto): boolean {
  return err.code === "zip_slip_rejected";
}
