import { fireEvent, render as rtlRender, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement } from "react";
import FavoriteAddDialog from "./FavoriteAddDialog";
import { describeError } from "../lib/errors";
import { I18nProvider } from "../i18n/I18nContext";
import { en } from "../i18n/en";
import type { DryRunReport } from "../bindings/DryRunReport";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { FavoriteEdition } from "../bindings/FavoriteEdition";
import type { StringKey } from "../i18n/strings";

function render(ui: ReactElement) {
  return rtlRender(
    <I18nProvider locale="en" setLocale={() => {}}>
      {ui}
    </I18nProvider>,
  );
}

/** A real `t` built from the actual `en` catalog (11-04-PLAN.md Task 3) --
 * NOT a mock that trivially echoes the key. */
function realT(key: StringKey, params?: Record<string, string | number>): string {
  const template = en[key];
  if (!params) return template;
  return template.replace(/\{(\w+)\}/g, (match, token: string) =>
    params[token] === undefined ? match : String(params[token]),
  );
}

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

function makeEdition(overrides: Partial<FavoriteEdition> = {}): FavoriteEdition {
  return {
    language: 0n,
    symbol: "nwt",
    short: "New World Translation",
    lang: "English",
    ...overrides,
  };
}

function makeReport(overrides: Partial<DryRunReport> = {}): DryRunReport {
  return {
    added: { Location: 1, TagMap: 1 },
    overwritten: {},
    deleted: {},
    total_deleted: 0,
    skipped: {},
    ...overrides,
  };
}

beforeEach(() => {
  invokeMock.mockReset();
});

describe("FavoriteAddDialog — picker (EDIT-05 mark, 07-01-PLAN.md Task 3)", () => {
  it("renders a native Language select and a per-edition row; Add Favorite is disabled until an edition is chosen", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_favorite_languages") return Promise.resolve(["English"]);
      if (cmd === "list_favorite_editions") return Promise.resolve([makeEdition()]);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<FavoriteAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);

    const select = await screen.findByTestId("favorite-dialog-language");
    expect(select.tagName).toBe("SELECT");

    const editionRow = await screen.findByTestId("favorite-dialog-edition-nwt");
    expect(editionRow).toHaveTextContent("New World Translation");

    const addButton = screen.getByTestId("favorite-dialog-add");
    expect(addButton).toBeDisabled();

    fireEvent.click(editionRow);
    expect(addButton).not.toBeDisabled();
  });

  it("edition row labels carry a title= attribute equal to their full text", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_favorite_languages") return Promise.resolve(["English"]);
      if (cmd === "list_favorite_editions") return Promise.resolve([makeEdition()]);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<FavoriteAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);

    const editionRow = await screen.findByTestId("favorite-dialog-edition-nwt");
    expect(editionRow).toHaveAttribute("title", "New World Translation");
  });

  it("defaults the Language select to English when present in the catalog", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_favorite_languages") return Promise.resolve(["Deutsch", "English", "Español"]);
      if (cmd === "list_favorite_editions") return Promise.resolve([makeEdition()]);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<FavoriteAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);

    const select = (await screen.findByTestId("favorite-dialog-language")) as HTMLSelectElement;
    await vi.waitFor(() => expect(select.value).toBe("English"));
  });

  it("choosing a language with no editions renders the empty-editions sentence and keeps the language select usable", async () => {
    invokeMock.mockImplementation((cmd: string, args?: { language?: string }) => {
      if (cmd === "list_favorite_languages") return Promise.resolve(["English", "Klingon"]);
      if (cmd === "list_favorite_editions") {
        if (args?.language === "Klingon") {
          return Promise.resolve([]);
        }
        return Promise.resolve([makeEdition()]);
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<FavoriteAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);

    const select = (await screen.findByTestId("favorite-dialog-language")) as HTMLSelectElement;
    await vi.waitFor(() => expect(select.value).toBe("English"));
    await screen.findByTestId("favorite-dialog-edition-nwt");

    fireEvent.change(select, { target: { value: "Klingon" } });

    const empty = await screen.findByTestId("favorite-dialog-empty");
    expect(empty).toHaveTextContent("No editions found for Klingon. Try a different language.");
    expect(select).not.toBeDisabled();
  });

  it("Cancel invokes onCancel and no apply/dry-run command is ever called", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_favorite_languages") return Promise.resolve(["English"]);
      if (cmd === "list_favorite_editions") return Promise.resolve([makeEdition()]);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onCancel = vi.fn();
    render(<FavoriteAddDialog onApplied={vi.fn()} onCancel={onCancel} onError={vi.fn()} />);

    await screen.findByTestId("favorite-dialog");
    fireEvent.click(screen.getByTestId("favorite-dialog-cancel"));

    expect(onCancel).toHaveBeenCalledTimes(1);
    expect(invokeMock).not.toHaveBeenCalledWith("favorite_add_dry_run", expect.anything());
    expect(invokeMock).not.toHaveBeenCalledWith("favorite_add_apply", expect.anything());
  });

  it("Escape key invokes onCancel while picking (not while a dry-run is pending)", async () => {
    let resolveDryRun: (report: DryRunReport) => void = () => {};
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_favorite_languages") return Promise.resolve(["English"]);
      if (cmd === "list_favorite_editions") return Promise.resolve([makeEdition()]);
      if (cmd === "favorite_add_dry_run") {
        return new Promise<DryRunReport>((resolve) => {
          resolveDryRun = resolve;
        });
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onCancel = vi.fn();
    render(<FavoriteAddDialog onApplied={vi.fn()} onCancel={onCancel} onError={vi.fn()} />);

    const editionRow = await screen.findByTestId("favorite-dialog-edition-nwt");
    fireEvent.click(editionRow);
    fireEvent.click(screen.getByTestId("favorite-dialog-add"));

    // While the dry-run is in flight, Escape must be a no-op.
    fireEvent.keyDown(document, { key: "Escape" });
    expect(onCancel).not.toHaveBeenCalled();

    resolveDryRun(makeReport());
    await screen.findByTestId("edit-preview-dialog");
  });
});

describe("FavoriteAddDialog — dry-run to preview to apply", () => {
  it("clicking Add Favorite invokes favorite_add_dry_run exactly once with the chosen edition and opens EditPreviewDialog", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_favorite_languages") return Promise.resolve(["English"]);
      if (cmd === "list_favorite_editions") return Promise.resolve([makeEdition()]);
      if (cmd === "favorite_add_dry_run") return Promise.resolve(makeReport());
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<FavoriteAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);

    const editionRow = await screen.findByTestId("favorite-dialog-edition-nwt");
    fireEvent.click(editionRow);
    fireEvent.click(screen.getByTestId("favorite-dialog-add"));

    await screen.findByTestId("edit-preview-dialog");

    const dryRunCalls = invokeMock.mock.calls.filter(([cmd]) => cmd === "favorite_add_dry_run");
    expect(dryRunCalls).toHaveLength(1);
    expect(dryRunCalls[0][1]).toEqual({ edition: { symbol: "nwt", language: 0n } });
  });

  it("EditPreviewDialog Confirm applies, re-fetches Favorites, and calls onApplied with the fresh rows", async () => {
    const freshRows = [{ id: 900n }];
    invokeMock.mockImplementation((cmd: string, args?: { category?: string }) => {
      if (cmd === "list_favorite_languages") return Promise.resolve(["English"]);
      if (cmd === "list_favorite_editions") return Promise.resolve([makeEdition()]);
      if (cmd === "favorite_add_dry_run") return Promise.resolve(makeReport());
      if (cmd === "favorite_add_apply") return Promise.resolve(makeReport());
      if (cmd === "list_category" && args?.category === "Favorites") {
        return Promise.resolve(freshRows);
      }
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onApplied = vi.fn();
    render(<FavoriteAddDialog onApplied={onApplied} onCancel={vi.fn()} onError={vi.fn()} />);

    const editionRow = await screen.findByTestId("favorite-dialog-edition-nwt");
    fireEvent.click(editionRow);
    fireEvent.click(screen.getByTestId("favorite-dialog-add"));

    await screen.findByTestId("edit-preview-dialog");
    fireEvent.click(screen.getByTestId("edit-preview-confirm"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("favorite_add_apply", {
        edition: { symbol: "nwt", language: 0n },
      }),
    );
    await vi.waitFor(() => expect(onApplied).toHaveBeenCalledWith(freshRows));
  });

  it("EditPreviewDialog Cancel falls back to the picker (not the outer onCancel) and invokes no apply command", async () => {
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_favorite_languages") return Promise.resolve(["English"]);
      if (cmd === "list_favorite_editions") return Promise.resolve([makeEdition()]);
      if (cmd === "favorite_add_dry_run") return Promise.resolve(makeReport());
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onCancel = vi.fn();
    render(<FavoriteAddDialog onApplied={vi.fn()} onCancel={onCancel} onError={vi.fn()} />);

    const editionRow = await screen.findByTestId("favorite-dialog-edition-nwt");
    fireEvent.click(editionRow);
    fireEvent.click(screen.getByTestId("favorite-dialog-add"));

    await screen.findByTestId("edit-preview-dialog");
    fireEvent.click(screen.getByTestId("edit-preview-cancel"));

    // Falls back to the picker — the outer onCancel is NOT called, and the
    // picker (with the same edition still selected) is showing again.
    expect(onCancel).not.toHaveBeenCalled();
    expect(screen.getByTestId("favorite-dialog")).toBeInTheDocument();
    expect(invokeMock).not.toHaveBeenCalledWith("favorite_add_apply", expect.anything());
  });
});

describe("FavoriteAddDialog — error routing", () => {
  it("a favorite_duplicate dry-run error routes the duplicate-specific code through onError, not favorite_failed", async () => {
    const duplicateError: ErrorDto = {
      code: "favorite_duplicate",
      operation: "favorite_add_dry_run",
      safe_file_name: null,
      message_key: "error.archive.favorite_duplicate",
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_favorite_languages") return Promise.resolve(["English"]);
      if (cmd === "list_favorite_editions") return Promise.resolve([makeEdition()]);
      if (cmd === "favorite_add_dry_run") return Promise.reject(duplicateError);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onError = vi.fn();
    render(<FavoriteAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={onError} />);

    const editionRow = await screen.findByTestId("favorite-dialog-edition-nwt");
    fireEvent.click(editionRow);
    fireEvent.click(screen.getByTestId("favorite-dialog-add"));

    await vi.waitFor(() => expect(onError).toHaveBeenCalledWith(duplicateError));

    // The duplicate-specific sentence is distinct from the generic
    // favorite_failed copy — same describeError copy-map the app's
    // ErrorBanner renders from (07-UI-SPEC.md: "routes through the
    // existing ErrorBanner + describeError copy-map pattern").
    const duplicateSentence = describeError(duplicateError, realT);
    const genericSentence = describeError({ ...duplicateError, code: "favorite_failed" }, realT);
    expect(duplicateSentence).toBe(
      "This edition is already marked as a favorite. Choose a different edition, or check your existing Favorites.",
    );
    expect(duplicateSentence).not.toBe(genericSentence);

    // The dialog stays open (falls back to the picker) so the user can
    // immediately try a different edition, per this exact error's own copy.
    expect(screen.getByTestId("favorite-dialog")).toBeInTheDocument();
  });

  it("a catalog-fetch failure (list_favorite_languages) routes through onError", async () => {
    const fetchError: ErrorDto = {
      code: "missing_resources_db",
      operation: "list_favorite_languages",
      safe_file_name: null,
      message_key: "error.archive.missing_resources_db",
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "list_favorite_languages") return Promise.reject(fetchError);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    const onError = vi.fn();
    render(<FavoriteAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={onError} />);

    await vi.waitFor(() => expect(onError).toHaveBeenCalledWith(fetchError));
  });
});
