import { describe, expect, it } from "vitest";
import { operationSet } from "./operations";

describe("operationSet (D6-08 capability descriptor)", () => {
  it("Notes at 0 selected: delete/export/view/color/tag present, Notes:delete is the sole live op (all disabled at 0)", () => {
    const ops = operationSet("Notes", 0);
    const byOp = new Map(ops.map((o) => [o.op, o]));

    // Notes capability includes every selection-gated op.
    for (const op of ["delete", "export", "view", "color", "tag"] as const) {
      expect(byOp.has(op), `Notes should surface ${op}`).toBe(true);
    }

    // delete is LIVE but needs a selection: not deferred, but disabled at 0.
    expect(byOp.get("delete")).toMatchObject({ deferred: false, enabled: false });

    // Every op OTHER than delete is deferred (not in the LIVE set).
    for (const o of ops) {
      if (o.op === "delete") continue;
      expect(o.deferred, `${o.op} should be deferred`).toBe(true);
      expect(o.enabled, `${o.op} should be disabled`).toBe(false);
    }
  });

  it("Notes at 2 selected: Notes:delete becomes enabled (live AND selection > 0)", () => {
    const ops = operationSet("Notes", 2);
    const del = ops.find((o) => o.op === "delete");
    expect(del).toMatchObject({ deferred: false, enabled: true });
  });

  it("Bookmarks at 3 selected: every op is deferred and none enabled (only Notes:delete is live in Phase 6)", () => {
    const ops = operationSet("Bookmarks", 3);
    expect(ops.length).toBeGreaterThan(0);
    for (const o of ops) {
      expect(o.deferred, `${o.op} should be deferred`).toBe(true);
      expect(o.enabled, `${o.op} should be disabled`).toBe(false);
    }
  });

  it("every op carries { op, enabled, deferred }; deferred is true exactly when (category, op) is not LIVE", () => {
    for (const cat of [
      "Notes",
      "Bookmarks",
      "Favorites",
      "Highlights",
      "Annotations",
      "Playlists",
    ] as const) {
      for (const o of operationSet(cat, 1)) {
        expect(o).toHaveProperty("op");
        expect(o).toHaveProperty("enabled");
        expect(o).toHaveProperty("deferred");
        const isLive = cat === "Notes" && o.op === "delete";
        expect(o.deferred).toBe(!isLive);
      }
    }
  });
});
