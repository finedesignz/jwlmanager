//! Shared export-file header (IO-01) — ports `export_header`
//! (`JWLManager.py:1367-1369`) verbatim, byte-for-byte:
//!
//! ```text
//! {category_tag}
//! ·
//! Exported from {archive_name}
//! by {APP} ({VERSION}) on {timestamp}
//! ****************************************************************************
//! ```
//!
//! (the `·` line above is a rendering stand-in for a single literal space
//! character — see the doc comment on [`build_export_header`] for why it is
//! load-bearing). Every value is INJECTED via [`ExportHeaderCtx`], never read
//! from the wall clock or the session inside the formatter itself: the
//! golden-fixture byte comparison (`export_wireformat_tests.rs`) is only
//! deterministic if the header-building function is a pure function of its
//! inputs.

/// Injected context for [`build_export_header`]. `category_tag` is the
/// literal tag line each category writes (`"{FAVORITES}"`, `"{BOOKMARKS}"`,
/// ...); `archive_name` is `Path(current_archive).name` — the archive's base
/// file name, never a full path (`JWLManager.py:1829`); `app_version` is
/// `env!("CARGO_PKG_VERSION")`; `timestamp` is a `%Y-%m-%d @ %H:%M:%S`-shaped
/// string built by the caller (`crate::time`).
pub struct ExportHeaderCtx<'a> {
    pub category_tag: &'a str,
    pub archive_name: String,
    pub app_version: String,
    pub timestamp: String,
}

/// The English literals `export_header` hardcodes via `_()` gettext calls —
/// localization is Phase 11 (out of scope here); ported as fixed literals.
const APP_NAME: &str = "JWL Manager";

/// Builds the shared export-file header, byte-for-byte matching
/// `export_header` (`JWLManager.py:1367-1369`):
/// `category + '\n \n' + 'Exported from' + f' {archive}\n' + 'by' +
/// f' {APP} ({VERSION}) ' + 'on' + f" {timestamp}\n" + '*'*76`.
///
/// The second line is a SINGLE SPACE CHARACTER between two newlines — an
/// "invisible char on first line to force UTF-8 encoding" per Python's own
/// comment at `:1368`. This is load-bearing wire-format content, not
/// incidental whitespace: **never trim it**. The returned string ends with
/// exactly 76 `*` characters and NO trailing newline — the caller appends
/// each data row's own leading `\n` (`export.rs`).
pub fn build_export_header(ctx: &ExportHeaderCtx) -> String {
    let stars = "*".repeat(76);
    format!(
        "{category}\n \nExported from {archive}\nby {app} ({version}) on {timestamp}\n{stars}",
        category = ctx.category_tag,
        archive = ctx.archive_name,
        app = APP_NAME,
        version = ctx.app_version,
        timestamp = ctx.timestamp,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_export_header_matches_python_shape() {
        let ctx = ExportHeaderCtx {
            category_tag: "{FAVORITES}",
            archive_name: "MyArchive.jwlibrary".to_string(),
            app_version: "0.1.0".to_string(),
            timestamp: "2026-01-01 @ 00:00:00".to_string(),
        };
        let header = build_export_header(&ctx);
        let expected = format!(
            "{{FAVORITES}}\n \nExported from MyArchive.jwlibrary\nby JWL Manager (0.1.0) on 2026-01-01 @ 00:00:00\n{}",
            "*".repeat(76)
        );
        assert_eq!(header, expected);
        assert!(!header.ends_with('\n'), "header must not end with a newline");
        assert_eq!(header.matches('*').count(), 76);
    }
}
