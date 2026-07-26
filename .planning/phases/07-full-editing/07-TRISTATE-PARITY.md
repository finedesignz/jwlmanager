# Phase 7 — Tri-state Tag Dialog Parity Check

**Question:** when the user confirms the tag dialog without touching a row rendered
INDETERMINATE (partially checked), what happens to that tag's `TagMap` rows?

**Verdict: (A) — nothing. The partial state is preserved.** The shipped Rust/React
implementation matches the Python.

## Evidence

`res/ui_extras.py:264-277` — `TagDialog.apply_changes` builds `self.modified` by
diffing each row's FINAL check state against the count it was constructed with:

```python
if tag is None and state == Qt.CheckState.Checked:
    self.modified.append((tag, name, self.selected_count))
elif state == Qt.CheckState.Checked and original_count != self.selected_count:
    self.modified.append((tag, name, self.selected_count))
elif state == Qt.CheckState.Unchecked and original_count != 0:
    self.modified.append((tag, name, 0))
```

A row left in `Qt.CheckState.PartiallyChecked` matches none of these branches and
therefore contributes no entry. Tri-state is only enabled for genuinely partial rows
(`res/ui_extras.py:189-190`, `0 < count < selected_count`), and initial state is set
from the same count comparison (`:193-198`).

## The one real difference (benign, recorded deliberately)

Python diffs **final state vs. original count**. The shipped `TagDialog.tsx` tracks
**explicitly toggled rows**. These agree on every path that changes archive state, and
disagree only when a user toggles a row and then returns it to its starting state:

| Start | User action | Python emits | Shipped emits | Net effect on TagMap |
|-------|-------------|--------------|---------------|----------------------|
| Partial | none | nothing | nothing | identical |
| Partial | → Checked | add-to-all | add-to-all | identical |
| Partial | → Checked → Unchecked | remove-from-all | remove-from-all | identical |
| Checked | → Unchecked → Checked | nothing | possibly a redundant add | `INSERT OR IGNORE` on rows that already exist — no-op |
| Unchecked | → Checked → Unchecked | nothing | possibly a redundant remove | `DELETE` matching zero rows — no-op |

Both divergent cases produce a redundant delta that resolves to a no-op against the
database. No archive state differs between the two apps. No fix required.

Checked because the Wave 3 executor honestly flagged this as an interpretation call
rather than a verified port — `res/ui_extras.py` was outside that plan's read scope.
