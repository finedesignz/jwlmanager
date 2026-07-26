import { describe, expect, it } from "vitest";
import { operationSet } from "./operations";

describe("operationSet (D6-08 capability descriptor)", () => {
  it("Notes at 0 selected: delete/export/view/color/tag present, Notes:delete/color/tag/view are the live ops (all disabled at 0)", () => {
    const ops = operationSet("Notes", 0);
    const byOp = new Map(ops.map((o) => [o.op, o]));

    // Notes capability includes every selection-gated op.
    for (const op of ["delete", "export", "view", "color", "tag"] as const) {
      expect(byOp.has(op), `Notes should surface ${op}`).toBe(true);
    }

    // delete/color/tag/view are LIVE but need a selection: not deferred, but
    // disabled at 0 (07-02-PLAN.md: Notes:color; 07-03-PLAN.md: Notes:tag;
    // 07-05-PLAN.md: Notes:view, the record editor).
    expect(byOp.get("delete")).toMatchObject({ deferred: false, enabled: false });
    expect(byOp.get("color")).toMatchObject({ deferred: false, enabled: false });
    expect(byOp.get("tag")).toMatchObject({ deferred: false, enabled: false });
    expect(byOp.get("view")).toMatchObject({ deferred: false, enabled: false });

    // Every op OTHER than delete/color/tag/view is deferred (not in LIVE).
    for (const o of ops) {
      if (o.op === "delete" || o.op === "color" || o.op === "tag" || o.op === "view") continue;
      expect(o.deferred, `${o.op} should be deferred`).toBe(true);
      expect(o.enabled, `${o.op} should be disabled`).toBe(false);
    }
  });

  it("Notes at 2 selected: Notes:delete becomes enabled (live AND selection > 0)", () => {
    const ops = operationSet("Notes", 2);
    const del = ops.find((o) => o.op === "delete");
    expect(del).toMatchObject({ deferred: false, enabled: true });
  });

  it("Bookmarks at 3 selected: Bookmarks:delete is live and enabled (D7-10, 07-05-PLAN.md); export/import stay deferred", () => {
    const ops = operationSet("Bookmarks", 3);
    expect(ops.length).toBeGreaterThan(0);
    const del = ops.find((o) => o.op === "delete");
    expect(del).toMatchObject({ deferred: false, enabled: true });
    for (const o of ops) {
      if (o.op === "delete") continue;
      expect(o.deferred, `${o.op} should be deferred`).toBe(true);
      expect(o.enabled, `${o.op} should be disabled`).toBe(false);
    }
  });

  it("Favorites at 2 selected: Favorites:delete and Favorites:add are live (07-01-PLAN.md), export/import stay deferred", () => {
    const ops = operationSet("Favorites", 2);
    const del = ops.find((o) => o.op === "delete");
    expect(del).toMatchObject({ deferred: false, enabled: true });
    const add = ops.find((o) => o.op === "add");
    expect(add).toMatchObject({ deferred: false, enabled: true });

    for (const o of ops) {
      if (o.op === "delete" || o.op === "add") continue;
      expect(o.deferred, `${o.op} should be deferred`).toBe(true);
      expect(o.enabled, `${o.op} should be disabled`).toBe(false);
    }
  });

  it("Favorites at 0 selected: Favorites:delete is live but disabled (needs a selection); Favorites:add is live AND enabled (selection-independent)", () => {
    const ops = operationSet("Favorites", 0);
    const del = ops.find((o) => o.op === "delete");
    expect(del).toMatchObject({ deferred: false, enabled: false });
    const add = ops.find((o) => o.op === "add");
    expect(add).toMatchObject({ deferred: false, enabled: true });
  });

  it("Playlists' add stays deferred even though Favorites' add is live — LIVE is keyed per-category, not per-op", () => {
    const ops = operationSet("Playlists", 0);
    const add = ops.find((o) => o.op === "add");
    expect(add).toMatchObject({ deferred: true, enabled: false });
  });

  it("every op carries { op, enabled, deferred }; deferred is true exactly when (category, op) is not LIVE", () => {
    // Mirrors operations.ts's own LIVE set — kept as an explicit local list
    // (rather than importing LIVE, which is module-private) so this test
    // fails loudly the moment a new pair is flipped live without updating it.
    const LIVE_PAIRS = new Set([
      "Notes:delete",
      "Notes:color",
      "Notes:tag",
      "Notes:view",
      "Favorites:delete",
      "Favorites:add",
      "Highlights:color",
      "Highlights:delete",
      "Bookmarks:delete",
      "Annotations:delete",
      "Annotations:view",
    ]);
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
        const isLive = LIVE_PAIRS.has(`${cat}:${o.op}`);
        expect(o.deferred).toBe(!isLive);
      }
    }
  });
});
