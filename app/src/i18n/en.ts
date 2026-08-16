/**
 * The ONE complete catalog (D11-02, 11-03-PLAN.md Task 1). `StringKey`
 * (../i18n/strings.ts) is derived as `keyof typeof en` -- there is no
 * separate hand-maintained key list, so `en` cannot omit a key any other
 * code references without a compile error.
 *
 * Keys are dot-namespaced by owning surface (`app.*` for App.tsx's shell
 * chrome, `settings.*` for SettingsDialog.tsx). The product name
 * ("JWL Manager") is deliberately NOT a catalog entry -- proper nouns are
 * not translated.
 *
 * Mixed-markup convention: for a sentence whose middle segment is wrapped
 * in a literal JSX element (App.tsx's empty-state `.jwlibrary` sentence),
 * split it into a "before" and "after" key around the literal element
 * rather than inventing a markup-aware template language. Plan 11-04 reuses
 * this exact split for CommandBar's JSX-embedded summary sentences.
 *
 * Template values may contain a `{name}` token, substituted at call time by
 * `I18nContext.t(key, params)` (e.g. `settings.versionLine`'s `{version}`).
 */
export const en = {
  "app.settingsButton": "Settings…",
  "app.emptyState.title": "No archive open",
  "app.emptyState.bodyBefore": "Open a ",
  "app.emptyState.bodyAfter": " file to view your Notes, or create a new archive.",

  "settings.title": "Settings",
  "settings.themeLabel": "Theme",
  "settings.themeLight": "Light",
  "settings.themeDark": "Dark",
  "settings.languageLabel": "Language",
  "settings.aboutTitle": "About",
  "settings.versionLine": "Version {version}",
  "settings.closeButton": "Close",

  /**
   * errors.* (11-04-PLAN.md Task 1) -- describeError's ~39-case copy
   * catalog, keyed by `ErrorDto.code` (never a raw Rust Display string).
   * Every value below is the EXACT prior literal English sentence from
   * lib/errors.ts, moved verbatim -- this task moves copy, it does not
   * rewrite it. `errors.trimFailed`/`errors.recordEditFailed` are new
   * branches (the Rust source already emits these two codes; describeError
   * had no case for either before this plan -- Rule 2, required for
   * `describeError full coverage` to pass).
   */
  "errors.notAZip":
    "Couldn't open this archive — the file isn't a valid .jwlibrary backup. Choose a different file or check that it hasn't been moved.",
  "errors.missingManifest":
    "Couldn't open this archive — it's missing the manifest.json file every .jwlibrary backup must contain. Choose a different file or check that it hasn't been moved.",
  "errors.missingUserDataBackup":
    "Couldn't open this archive — it's missing the user_data backup every .jwlibrary backup must contain. Choose a different file or check that it hasn't been moved.",
  "errors.schemaTooOld":
    "Couldn't open this archive — it was created with a schema version too old for this app to open (the oldest supported version is 12). Choose a different file or use an older version of the app.",
  "errors.schemaTooNew":
    "Couldn't open this archive — it was created by a newer version of JW Library than this app supports. Update this app to the latest version, or choose a different file.",
  "errors.schemaUpgradeFailed":
    "Couldn't open this archive — upgrading its internal database format failed. The original file is unchanged. Choose a different file or try again.",
  "errors.schemaDowngradeFailed":
    "Couldn't downgrade this archive to the older format — some archives can't be converted without losing or conflicting data. Your original session is unchanged. Choose a different file or keep the current format.",
  "errors.trimFailed":
    "Couldn't save this archive — cleaning up its internal database failed. Your original session is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.deleteFailed":
    "Couldn't delete the selected items — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.favoriteFailed":
    "Couldn't add this favorite — the archive is unchanged. Try a different edition or try again.",
  "errors.favoriteDuplicate":
    "This edition is already marked as a favorite. Choose a different edition, or check your existing Favorites.",
  "errors.colorFailed":
    "Couldn't change the color of the selected items — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.tagFailed":
    "Couldn't update tags for the selected items — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.reorderFailed":
    "Couldn't sort tags — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.cleanFailed":
    "Couldn't clean this archive — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.maskFailed":
    "Couldn't mask this archive — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.recordEditFailed":
    "Couldn't save these changes — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.exportFailed":
    "Couldn't export this file — the archive is unchanged. Check that the destination folder is writable, then try again.",
  "errors.importMalformed":
    "Couldn't read this file — it doesn't look like a file exported from JW Library or JWL Manager. Choose a different file.",
  "errors.importFailed":
    "Couldn't import this file — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.playlistExportFailed":
    "Couldn't export this playlist — the archive is unchanged. Check that the destination folder is writable, then try again.",
  "errors.playlistImportFailed":
    "Couldn't import this playlist — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.mediaAddFailed":
    "Couldn't add media — no files were added and the archive is unchanged. Try again, or choose different files.",
  "errors.mediaUnsupportedFormat": "This file isn't a supported image format and wasn't added.",
  "errors.mediaDeleteFailed":
    "Couldn't delete the selected playlist items — the archive is unchanged. Try again, or close and reopen the archive if the problem continues.",
  "errors.mergeUnavailable":
    "Couldn't merge — the merge engine isn't available on this device (for example on Windows on Arm, which has no merge build yet). Your open archive is unchanged.",
  "errors.mergeFailed":
    "Couldn't merge these archives — the merge could not be completed and your open archive is unchanged. Choose a different source archive or try again.",
  "errors.zipSlipRejected":
    "This archive can't be opened — it contains files that try to write outside the extraction folder, which isn't safe. This file may be corrupted or tampered with.",
  "errors.statePoisoned":
    "Something went wrong with this app's internal session state. Close and reopen the archive, then try again.",
  "errors.missingResourcesLanguage":
    "Couldn't open this archive — the language data needed to label its notes is missing from this app's bundled resources. Reinstall the app and try again.",
  "errors.missingResourcesDb":
    "Couldn't open this archive — this app's bundled resources database is missing. Reinstall the app and try again.",
  "errors.ioError":
    "Couldn't complete this operation — a file system error occurred. Check that the file still exists, isn't open in another program, and that you have permission to access it, then try again.",
  "errors.sqliteError":
    "Couldn't read this archive's data — its internal database appears to be corrupted. Choose a different file or restore from an earlier backup.",
  "errors.zipError":
    "Couldn't read this archive's zip container — it may be corrupted. Choose a different file or restore from an earlier backup.",
  "errors.jsonError":
    "Couldn't read this archive's manifest — it isn't valid JSON. Choose a different file or restore from an earlier backup.",
  "errors.settingsAppDataDirUnavailable":
    "Couldn't save your settings — this app's settings folder isn't reachable. Your choice is applied for this session but won't be remembered on restart.",
  "errors.settingsWriteFailed":
    "Couldn't save your settings — your choice won't be remembered on restart. Check that this app has permission to write to its settings folder, then try again.",
  "errors.settingsReadFailed": "Couldn't read this app's saved settings — using the defaults for this session.",
  "errors.settingsParseFailed":
    "This app's saved settings file looks corrupted — using the defaults for this session.",
  "errors.default": "Couldn't complete this operation ({operation}). Choose a different file or try again.",

  /** common.* -- exact-duplicate words rendered verbatim across multiple
   * retrofitted components (11-04-PLAN.md Tasks 1-2); one key, many callers,
   * so the same word never drifts between components. */
  "common.cancel": "Cancel",
  "common.preparing": "Preparing…",
  "common.delete": "Delete",
  "common.add": "Add",
  "common.saving": "Saving…",
  "common.deleting": "Deleting…",
  "common.confirmDelete": "Confirm delete",
  "common.color": "Color",
  "common.ok": "OK",

  /** category.* -- the six Category enum DISPLAY labels (D6-06/DATA-08):
   * presentation only. CategorySwitcher.tsx/CategoryList.tsx keep the raw
   * `Category` enum value for every onSelect/IPC/data-testid use -- never
   * this translated label. */
  "category.groupAriaLabel": "Category",
  "category.Notes": "Notes",
  "category.Bookmarks": "Bookmarks",
  "category.Favorites": "Favorites",
  "category.Highlights": "Highlights",
  "category.Annotations": "Annotations",
  "category.Playlists": "Playlists",

  /** color.* -- the seven PALETTE swatch display names (ColorMenu.tsx +
   * RecordEditor.tsx); presentation only -- `colorIndex`, never `colorName`,
   * is what crosses IPC. */
  "color.Grey": "Grey",
  "color.Yellow": "Yellow",
  "color.Green": "Green",
  "color.Blue": "Blue",
  "color.Red": "Red",
  "color.Orange": "Orange",
  "color.Purple": "Purple",

  "jwlCoreNotice.message":
    "Merge isn’t available on this device{reason}. Everything else — opening, viewing, and saving archives — works normally.",
  "jwlCoreNotice.dismissAriaLabel": "Dismiss jwlCore capability notice",

  "colorMenu.confirmTitle": "Change color to {color}?",
  "colorMenu.confirmAriaLabel": "Confirm color change",
  "colorMenu.confirmLabel": "Change Color",
  "colorMenu.confirmPending": "Changing…",
  "colorMenu.summaryWithNew":
    "{recolored} highlight{recoloredPlural} will be recolored to {color}. {synthesized} note{synthesizedPlural} will become highlighted in {color} for the first time.",
  "colorMenu.summaryRecoloredOnly": "{recolored} highlight{recoloredPlural} will be recolored to {color}.",
  "colorMenu.greyDisabledTitle": "Grey has no effect on existing highlights",

  "utilitiesMenu.ariaLabel": "Utilities",
  "utilitiesMenu.clean": "Clean Archive…",
  "utilitiesMenu.mask": "Mask Archive…",
  "utilitiesMenu.sort": "Sort Tags…",
  "utilitiesMenu.sortNoopSummary": "No tag assignments need renumbering.",
  "utilitiesMenu.sortTitle": "Sort tags?",
  "utilitiesMenu.sortAriaLabel": "Confirm sort tags",
  "utilitiesMenu.sortConfirmLabel": "Sort Tags",
  "utilitiesMenu.sortConfirmPending": "Sorting…",
  "utilitiesMenu.sortSummary":
    "This renumbers tag order for every tagged note, sorted by note. {count} tag assignment{plural} will be renumbered.",
  "utilitiesMenu.cleanTitle": "Clean this archive?",
  "utilitiesMenu.cleanAriaLabel": "Confirm clean archive",
  "utilitiesMenu.cleanConfirmLabel": "Clean Archive",
  "utilitiesMenu.cleanConfirmPending": "Cleaning…",
  "utilitiesMenu.cleanSummary":
    "This normalizes hidden separator characters (like non-breaking spaces) in note titles, note content, and annotations. {count} row{plural} will be updated.",
  "utilitiesMenu.maskTitle": "Mask this archive?",
  "utilitiesMenu.maskAriaLabel": "Confirm mask archive",
  "utilitiesMenu.maskConfirmLabel": "Mask Archive",
  "utilitiesMenu.maskConfirmPending": "Masking…",
  "utilitiesMenu.maskWarning":
    "This permanently replaces all text in this archive with randomized characters — note titles and content, annotation values, bookmark titles and snippets, and location titles. This cannot be undone.",
  "utilitiesMenu.maskExtraSummary": "{count} record{plural} across {tables} will be masked.",

  "commandBar.toolbarAriaLabel": "Archive commands",
  "commandBar.filterBackup": "JW Library Backup",
  "commandBar.opening": "Opening…",
  "commandBar.openArchive": "Open Archive",
  "commandBar.creating": "Creating…",
  "commandBar.newArchive": "New Archive",
  "commandBar.save": "Save",
  "commandBar.saveAs": "Save As",
  "commandBar.saveV14": "Save v14-compatible copy…",
  "commandBar.merge": "Merge Archive…",
  "commandBar.foldMerge": "Merge Multiple Archives…",
  "commandBar.utilitiesTrigger": "Utilities ▾",
  "commandBar.v14Title": "Save v14-compatible copy?",
  "commandBar.v14AriaLabel": "Confirm v14-compatible save",
  "commandBar.v14ConfirmLabel": "Save v14 copy",
  "commandBar.v14Summary":
    "{count} Location{plural} will be merged for v14 compatibility. This writes a separate copy — your open archive is left unchanged.",
  "commandBar.mergeTitle": "Merge this archive?",
  "commandBar.mergeAriaLabel": "Confirm merge",
  "commandBar.mergeConfirmLabel": "Merge",
  "commandBar.merging": "Merging…",
  "commandBar.mergeSummary":
    "{added} record{plural} added, {updated} updated from {fileName}. This merges into your open archive — nothing is written until you save.",
  "commandBar.foldMergeTitle": "Merge these archives?",
  "commandBar.foldMergeAriaLabel": "Confirm fold merge",
  "commandBar.foldMergeSummary":
    "{added} record{plural} added, {updated} updated from the combined effect of {archiveCount} archives in the shown order. This merges into your open archive — nothing is written until you save.",

  "editPreviewDialog.defaultTitle": "Delete these Notes?",
  "editPreviewDialog.typedConfirmAriaLabel": "Type {value} to confirm",
  "editPreviewDialog.defaultSummary":
    "This will remove {items} ({count} row{plural} total). This can't be undone once you save.",
  "editPreviewDialog.nothing": "nothing",

  "foldMergeDialog.ariaLabel": "Merge multiple archives",
  "foldMergeDialog.title": "Merge Multiple Archives",
  "foldMergeDialog.orderNote":
    "Archives merge in the order shown, top to bottom. When two archives change the same record, the one lower in the list wins.",
  "foldMergeDialog.moveUp": "Move {name} up",
  "foldMergeDialog.moveDown": "Move {name} down",
  "foldMergeDialog.remove": "Remove {name}",
  "foldMergeDialog.reason": "Pick at least {count} archives to merge.",
  "foldMergeDialog.continue": "Continue",

  "tagDialog.title": "Edit Tags",
  "tagDialog.loading": "Loading tags…",
  "tagDialog.empty": "No tags yet — type a name below to create one.",
  "tagDialog.newTagLabel": "{name} (new)",
  "tagDialog.newTagPlaceholder": "New tag name…",
  "tagDialog.apply": "Apply",
  "tagDialog.previewTitle": "Update tags for {count} item{plural}?",
  "tagDialog.previewAriaLabel": "Confirm tag update",
  "tagDialog.previewConfirmLabel": "Update Tags",
  "tagDialog.previewConfirmPending": "Updating…",

  "favoriteDialog.title": "Add Favorite",
  "favoriteDialog.addTitle": "Add this favorite?",
  "favoriteDialog.addAriaLabel": "Confirm add favorite",
  "favoriteDialog.adding": "Adding…",
  "favoriteDialog.languageLabel": "Language",
  "favoriteDialog.loadingEditions": "Loading editions…",
  "favoriteDialog.noEditions": "No editions found for {language}. Try a different language.",

  "mediaAddDialog.filterImages": "Images",
  "mediaAddDialog.ariaLabel": "Add Media",
  "mediaAddDialog.heading": "Add media?",
  "mediaAddDialog.playlistLabel": "Playlist",
  "mediaAddDialog.playlistPlaceholder": "Select existing or type a new playlist name",
  "mediaAddDialog.checkingFiles": "Checking files…",
  "mediaAddDialog.chooseFiles": "Choose files…",
  "mediaAddDialog.allDuplicates": "All selected files are already in this archive.",
  "mediaAddDialog.copyingFiles": "Copying files… 0 of {count}",
  "mediaAddDialog.addedSummary": "{count} added{failedSuffix}.",
  "mediaAddDialog.failedSuffix": ", {count} failed",
  "mediaAddDialog.statusNew": "new",
  "mediaAddDialog.statusDuplicate": "already added",
  "mediaAddDialog.statusUnsupportedWithReason": "unreadable — {reason}",
  "mediaAddDialog.statusUnsupported": "unreadable — not a supported image",
  "mediaAddDialog.statusAdded": "added",
  "mediaAddDialog.done": "Done",
  "mediaAddDialog.addMediaButton": "Add Media ({count})",

  "recordEditor.editNote": "Edit Note",
  "recordEditor.editAnnotation": "Edit Annotation",
  "recordEditor.loading": "Loading…",
  "recordEditor.titleLabel": "Title",
  "recordEditor.contentLabel": "Content",
  "recordEditor.noColor": "No color",
  "recordEditor.valueLabel": "Value",
  "recordEditor.saveChanges": "Save Changes",
  "recordEditor.saveTitle": "Save these changes?",
  "recordEditor.saveAriaLabel": "Confirm save",
  "recordEditor.deleteNoteTitle": "Delete this Note?",
  "recordEditor.deleteAnnotationTitle": "Delete this Annotation?",
  "recordEditor.overDeleteBefore": "Deleting this annotation removes ",
  "recordEditor.overDeleteAfter":
    " annotation fields at this location, not just this one — {count} annotation field{plural} total will be deleted.",

  "categoryList.selectionCheckboxAriaLabel": "Select {label}",
  "categoryList.deleteButton": "Delete ({count})",
  "categoryList.emptyState": "No {category} in this archive.",
  "categoryList.filterTextFiles": "Text files",
  "categoryList.filterPlaylists": "JW Library playlists",
  "categoryList.priorExportTitle": "Select a prior {category} export…",
  "categoryList.op.delete": "Delete",
  "categoryList.op.export": "Export…",
  "categoryList.op.view": "Edit",
  "categoryList.op.color": "Color",
  "categoryList.op.tag": "Tag",
  "categoryList.op.add": "Add",
  "categoryList.op.import": "Import…",
  "categoryList.op.addFavorite": "Add Favorite",
  "categoryList.op.addMedia": "Add Media…",
  "categoryList.op.comingSoonTitle": "Coming soon",
  "categoryList.op.deferredLabel": "{label} (soon)",
  "categoryList.export.tooltipNoSelection": "No rows selected — exports all {category}.",
  "categoryList.export.pending": "Exporting…",
  "categoryList.export.done": "Exported",
  "categoryList.export.changedButton": "Export changed…",
  "categoryList.import.pending": "Importing…",
  "categoryList.import.titlePlaylist": "Import Playlist?",
  "categoryList.import.title": "Import {category}?",
  "categoryList.import.labelPlaylist": "Import Playlist",
  "categoryList.import.label": "Import {category}",
  "categoryList.edit.tooManySelected": "Select exactly one row to edit",
  "categoryList.playlistDelete.title": "Delete {count} playlist item{plural}?",
  "categoryList.playlistDelete.summaryDeletes": "This deletes {count} playlist item{plural}.",
  "categoryList.playlistDelete.summaryMediaRemoved":
    "{count} media file{plural} used only by these items will also be removed.",
  "categoryList.playlistDelete.summaryMediaKept":
    "{count} media file{plural} will be kept because {pronoun} still used by other playlist items.",
  "categoryList.import.playlistLeadingClause":
    'This adds the playlist "{name}" and its {count} media file{plural}.',
  "categoryList.import.bucketDeleteOptIn":
    'Importing will first delete the {count} existing Note{plural} under "{bucket}" before adding the file\'s records.',
  "categoryList.import.nothingNew":
    "Nothing in this file is new — every record already matches what's in this archive.",
  "categoryList.import.added": "{count} new record{plural} will be added.",
  "categoryList.import.updated": "{count} existing record{plural} will be updated.",
  "categoryList.import.skipped": "{count} record{plural} in this file will be skipped (already present).",
  "categoryList.incrementalExport.label": "Export changed {category}",
  "categoryList.incrementalExport.summary":
    "{added} new record{addedPlural} and {modified} changed record{modifiedPlural} were written to the file.",
  "categoryList.incrementalExport.deletedCandidates":
    "{count} record{plural} in the prior export {isAre} no longer present in this archive — removals since the prior export cannot be represented in this file format and are NOT written to it.",
} as const;
