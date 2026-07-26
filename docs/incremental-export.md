# Incremental export ("Export changed...")

Alongside the regular "Export..." action, five categories -- Notes, Favorites,
Bookmarks, Highlights and Annotations -- offer an "Export changed..." action.
Instead of writing every record in the category, it writes only the records
that changed (or were added) since a prior export file you pick.

## What it does

1. Click "Export changed..." next to "Export..." for the category.
2. Pick a prior export file for that SAME category -- typically a `.txt` file
   you exported earlier, from this app or from the original JW Library
   Manager (Python) app.
3. Pick a destination for the new file, same as a regular export.
4. The app writes a new `.txt` file containing only the added and modified
   records, and shows a summary: how many records were added, how many were
   modified, and how many records from the prior file no longer have a
   matching live record in the archive.

The output is an ordinary export file in the SAME wire format the regular
"Export..." action produces. The Python JW Library Manager and this app both
import it unchanged -- there is no separate "incremental" file format.

## The reference point is a file you keep, not app-stored state

The "prior" side of the comparison is whatever export file you hand the app
at step 2 -- never something the app remembers between sessions or stores
alongside the archive. This has two practical consequences:

- It is portable. You can export from this app on one machine, carry the
  file to another machine (or hand it to someone else), and use it as the
  prior file there.
- If you lose the file, or use the wrong one, the app has no other record to
  fall back on -- it only knows what the file in front of it says.

## Change detection is content-based, never timestamp-based

The app decides whether a record changed by comparing the record's own
written-out text (hashed) against every record's text in the prior file --
never by looking at the archive's internal `LastModified`/timestamp columns.
A record whose only difference is a bumped internal timestamp, with
identical content, is NOT reported as changed. Conversely, re-exporting after
a timestamp drift (for example, after re-importing the same content on a
different day) produces nothing new, because the content itself has not
changed.

## Limitations

These are accepted trade-offs of the underlying wire formats and the
category's own data shape, not bugs:

- **Removals are never written into the file.** None of the wire formats
  this app writes or reads have any way to represent "this record was
  deleted." The summary tells you how many records from the prior file no
  longer have a live match in the archive (as a plain count), but that
  information exists only in the summary dialog -- it is never encoded in
  the output file itself. If you need to communicate a removal to whoever
  receives the exported file, you have to tell them separately.
- **Annotations selection can include unchanged records.** The exporter can
  only select and write Annotations by their location, not by the individual
  field that changed. If one field at a location changed, every other field
  recorded at that same location is written out alongside it, even though it
  did not itself change. The summary's added/modified counts still reflect
  only the fields that actually changed -- the extra, unchanged fields are
  simply along for the ride in the output file.
- **Favorites never report a "modified" count.** Every field on a Favorite's
  wire line is part of what identifies it -- there is no separate field that
  can change while the record stays "the same" Favorite. A Favorite can only
  be added or removed between two exports, never modified in place.
- **Playlists have no incremental export.** A playlist export is a whole
  playlist packaged into its own `.jwlplaylist` file (including its media),
  not a set of per-row text records that can be diffed line by line the way
  the other five categories are. "Export changed..." is not offered for
  Playlists; use the regular "Export..." action.
