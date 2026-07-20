import type { NotesRow } from "../bindings/NotesRow";

/**
 * Thin (NOT-yet-virtualized) render of `NotesRow[]` from the `open_archive`
 * IPC command. Fixed 44px single-line rows per 01-UI-SPEC.md. Virtualization
 * (`@tanstack/react-virtual`), resources.db label resolution, and the
 * independent-notes union all thicken in 01-04 — this plan only proves real
 * rows render end-to-end.
 */
export default function NotesList({ notes }: { notes: NotesRow[] }) {
  if (notes.length === 0) {
    return <p className="notes-list-empty">No notes in this archive.</p>;
  }

  return (
    <ul className="notes-list" role="list">
      {notes.map((note) => (
        <li key={Number(note.id)} className="notes-list-row">
          <span className="notes-list-row-tags">
            {note.tags ?? "* NO TAG *"}
          </span>
          <span className="notes-list-row-modified">
            {note.modified ?? ""}
          </span>
        </li>
      ))}
    </ul>
  );
}
