import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import CommandBar from "./CommandBar";
import type { BrowseRow } from "../bindings/BrowseRow";
import type { ErrorDto } from "../bindings/ErrorDto";

const invokeMock = vi.fn();
const openMock = vi.fn();
const saveMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openMock(...args),
  save: (...args: unknown[]) => saveMock(...args),
}));

function renderBar(overrides: Partial<Parameters<typeof CommandBar>[0]> = {}) {
  const handlers = {
    archiveOpen: false,
    onOpened: vi.fn(),
    onNewArchive: vi.fn(),
    onSaved: vi.fn(),
    onError: vi.fn(),
    onCancelled: vi.fn(),
    currentCategory: "Notes" as const,
    onCategoryRowsChanged: vi.fn(),
    ...overrides,
  };
  render(<CommandBar {...handlers} />);
  return handlers;
}

beforeEach(() => {
  invokeMock.mockReset();
  openMock.mockReset();
  saveMock.mockReset();
});

describe("CommandBar", () => {
  it("invokes open_archive with the chosen path when Open Archive is clicked", async () => {
    openMock.mockResolvedValue("C:/archives/one.jwlibrary");
    const notes: BrowseRow[] = [];
    invokeMock.mockResolvedValue(notes);
    const handlers = renderBar();

    fireEvent.click(screen.getByRole("button", { name: /open archive/i }));
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(1));

    expect(invokeMock).toHaveBeenCalledWith("open_archive", {
      path: "C:/archives/one.jwlibrary",
    });
    await vi.waitFor(() => expect(handlers.onOpened).toHaveBeenCalledWith(notes));
  });

  it("invokes save_archive when Save is clicked with an archive open", async () => {
    invokeMock.mockResolvedValue(undefined);
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByRole("button", { name: /^save$/i }));
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("save_archive"));
    await vi.waitFor(() => expect(handlers.onSaved).toHaveBeenCalledTimes(1));
  });

  it("invokes save_as with the chosen path when Save As is clicked", async () => {
    saveMock.mockResolvedValue("C:/archives/copy.jwlibrary");
    invokeMock.mockResolvedValue(undefined);
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByRole("button", { name: /save as/i }));
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_as", {
        path: "C:/archives/copy.jwlibrary",
      }),
    );
    await vi.waitFor(() => expect(handlers.onSaved).toHaveBeenCalledTimes(1));
  });

  it("invokes new_archive with the chosen path when New Archive is clicked", async () => {
    saveMock.mockResolvedValue("C:/archives/new.jwlibrary");
    invokeMock.mockResolvedValue(undefined);
    const handlers = renderBar();

    fireEvent.click(screen.getByRole("button", { name: /new archive/i }));
    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("new_archive", {
        path: "C:/archives/new.jwlibrary",
      }),
    );
    await vi.waitFor(() => expect(handlers.onNewArchive).toHaveBeenCalledTimes(1));
  });

  it("double-click guard: a rapid second click while pending fires the invoke only once", async () => {
    let resolveOpen: (value: string) => void = () => {};
    openMock.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolveOpen = resolve;
        }),
    );
    invokeMock.mockResolvedValue([]);
    renderBar();

    const button = screen.getByRole("button", { name: /open archive/i });
    fireEvent.click(button);
    fireEvent.click(button); // rapid second click while the dialog promise is pending

    expect(openMock).toHaveBeenCalledTimes(1);

    resolveOpen("C:/archives/one.jwlibrary");
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(1));
  });

  it("shows a pending state and disables actions while an invoke is in flight", async () => {
    let resolveOpen: (value: string) => void = () => {};
    openMock.mockImplementation(
      () =>
        new Promise<string>((resolve) => {
          resolveOpen = resolve;
        }),
    );
    invokeMock.mockResolvedValue([]);
    renderBar();

    const openButton = screen.getByRole("button", { name: /open archive/i });
    fireEvent.click(openButton);

    expect(await screen.findByRole("button", { name: /opening/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /new archive/i })).toBeDisabled();

    resolveOpen("C:/archives/one.jwlibrary");
    await vi.waitFor(() =>
      expect(screen.getByRole("button", { name: /open archive/i })).not.toBeDisabled(),
    );
  });

  it("cancelled dialog produces no error and no invoke", async () => {
    openMock.mockResolvedValue(null); // user dismissed the OS file picker
    const handlers = renderBar();

    fireEvent.click(screen.getByRole("button", { name: /open archive/i }));
    await vi.waitFor(() => expect(handlers.onCancelled).toHaveBeenCalledTimes(1));

    expect(invokeMock).not.toHaveBeenCalled();
    expect(handlers.onError).not.toHaveBeenCalled();
  });

  it("surfaces an ErrorDto via onError when open_archive rejects", async () => {
    openMock.mockResolvedValue("C:/archives/bad.jwlibrary");
    const dto: ErrorDto = {
      code: "not_a_zip",
      operation: "open_archive",
      safe_file_name: "bad.jwlibrary",
      message_key: "error.archive.not_a_zip",
    };
    invokeMock.mockRejectedValue(dto);
    const handlers = renderBar();

    fireEvent.click(screen.getByRole("button", { name: /open archive/i }));
    await vi.waitFor(() => expect(handlers.onError).toHaveBeenCalledWith(dto));
  });

  it("disables Save and Save As when no archive is open", () => {
    renderBar({ archiveOpen: false });
    expect(screen.getByRole("button", { name: /^save$/i })).toBeDisabled();
    expect(screen.getByRole("button", { name: /save as/i })).toBeDisabled();
  });

  const V14_REPORT = {
    added: {},
    overwritten: {},
    deleted: { Location: 2 },
    total_deleted: 2,
  };

  it("Save v14 runs downgrade_dry_run then shows the preview dialog", async () => {
    saveMock.mockResolvedValue("C:/archives/copy-v14.jwlibrary");
    invokeMock.mockResolvedValue(V14_REPORT);
    renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("save-v14-button"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("downgrade_dry_run"),
    );
    expect(await screen.findByTestId("edit-preview-dialog")).toBeInTheDocument();
    expect(screen.getByTestId("edit-preview-summary")).toHaveTextContent(
      "2 Locations will be merged",
    );
    // Preview only — no write yet.
    expect(invokeMock).not.toHaveBeenCalledWith("save_v14_copy", expect.anything());
  });

  it("Confirm in the v14 preview invokes save_v14_copy with the chosen path", async () => {
    saveMock.mockResolvedValue("C:/archives/copy-v14.jwlibrary");
    invokeMock.mockResolvedValue(V14_REPORT);
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("save-v14-button"));
    const confirm = await screen.findByTestId("edit-preview-confirm");
    invokeMock.mockResolvedValue(undefined);
    fireEvent.click(confirm);

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("save_v14_copy", {
        path: "C:/archives/copy-v14.jwlibrary",
      }),
    );
    await vi.waitFor(() => expect(handlers.onSaved).toHaveBeenCalledTimes(1));
  });

  it("Cancel in the v14 preview never invokes save_v14_copy", async () => {
    saveMock.mockResolvedValue("C:/archives/copy-v14.jwlibrary");
    invokeMock.mockResolvedValue(V14_REPORT);
    renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("save-v14-button"));
    const cancel = await screen.findByTestId("edit-preview-cancel");
    fireEvent.click(cancel);

    await vi.waitFor(() =>
      expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument(),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("save_v14_copy", expect.anything());
  });

  it("dismissed v14 save dialog produces no invoke and no error", async () => {
    saveMock.mockResolvedValue(null); // user dismissed the OS save picker
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("save-v14-button"));
    await vi.waitFor(() => expect(handlers.onCancelled).toHaveBeenCalledTimes(1));

    expect(invokeMock).not.toHaveBeenCalled();
    expect(handlers.onError).not.toHaveBeenCalled();
    expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument();
  });

  it("disables the Save v14 action when no archive is open", () => {
    renderBar({ archiveOpen: false });
    expect(screen.getByTestId("save-v14-button")).toBeDisabled();
  });

  const MERGE_REPORT = {
    added: { Note: 1, UserMark: 1, Tag: 1 },
    overwritten: { Note: 2 },
    deleted: {},
    total_deleted: 0,
  };

  it("disables the Merge action when no archive is open", () => {
    renderBar({ archiveOpen: false });
    expect(screen.getByTestId("merge-button")).toBeDisabled();
  });

  it("Merge runs merge_dry_run on the chosen source then shows the preview with counts", async () => {
    openMock.mockResolvedValue("C:/archives/source.jwlibrary");
    invokeMock.mockResolvedValue(MERGE_REPORT);
    renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("merge-button"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("merge_dry_run", {
        sourcePath: "C:/archives/source.jwlibrary",
      }),
    );
    expect(await screen.findByTestId("edit-preview-dialog")).toBeInTheDocument();
    // 3 added (1+1+1), 2 overwritten, base file name of the source.
    expect(screen.getByTestId("edit-preview-summary")).toHaveTextContent(
      "3 records added, 2 updated from source.jwlibrary",
    );
    // Preview only — no commit yet.
    expect(invokeMock).not.toHaveBeenCalledWith("merge_commit", expect.anything());
  });

  it("Confirm in the merge preview invokes merge_commit then reloads notes via list_notes", async () => {
    openMock.mockResolvedValue("C:/archives/source.jwlibrary");
    invokeMock.mockResolvedValue(MERGE_REPORT);
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("merge-button"));
    const confirm = await screen.findByTestId("edit-preview-confirm");
    // After commit, list_notes returns the merged Notes list.
    const mergedNotes: BrowseRow[] = [];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "merge_commit") return Promise.resolve(undefined);
      if (cmd === "list_notes") return Promise.resolve(mergedNotes);
      return Promise.resolve(MERGE_REPORT);
    });
    fireEvent.click(confirm);

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("merge_commit", {
        sourcePath: "C:/archives/source.jwlibrary",
      }),
    );
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("list_notes"));
    // The merged list re-renders through the existing open refresh path.
    await vi.waitFor(() => expect(handlers.onOpened).toHaveBeenCalledWith(mergedNotes));
  });

  it("Cancel in the merge preview never invokes merge_commit", async () => {
    openMock.mockResolvedValue("C:/archives/source.jwlibrary");
    invokeMock.mockResolvedValue(MERGE_REPORT);
    renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("merge-button"));
    const cancel = await screen.findByTestId("edit-preview-cancel");
    fireEvent.click(cancel);

    await vi.waitFor(() =>
      expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument(),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("merge_commit", expect.anything());
  });

  it("dismissed merge source dialog produces no invoke and no error", async () => {
    openMock.mockResolvedValue(null); // user dismissed the OS file picker
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("merge-button"));
    await vi.waitFor(() => expect(handlers.onCancelled).toHaveBeenCalledTimes(1));

    expect(invokeMock).not.toHaveBeenCalled();
    expect(handlers.onError).not.toHaveBeenCalled();
    expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument();
  });

  it("surfaces a merge_unavailable ErrorDto via onError when merge_dry_run rejects", async () => {
    openMock.mockResolvedValue("C:/archives/source.jwlibrary");
    const dto: ErrorDto = {
      code: "merge_unavailable",
      operation: "merge_dry_run",
      safe_file_name: "source.jwlibrary",
      message_key: "error.merge.unavailable",
    };
    invokeMock.mockRejectedValue(dto);
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("merge-button"));
    await vi.waitFor(() => expect(handlers.onError).toHaveBeenCalledWith(dto));
    expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument();
  });

  const FOLD_SOURCES = [
    "C:/archives/one.jwlibrary",
    "C:/archives/two.jwlibrary",
    "C:/archives/three.jwlibrary",
  ];

  const FOLD_REPORT = {
    added: { Note: 2, UserMark: 1 },
    overwritten: { Note: 3 },
    deleted: {},
    total_deleted: 0,
  };

  it("disables the Merge Multiple Archives action when no archive is open", () => {
    renderBar({ archiveOpen: false });
    expect(screen.getByTestId("fold-merge-button")).toBeDisabled();
  });

  it("Merge Multiple Archives opens a multi-select picker; cancelling makes zero invoke calls", async () => {
    openMock.mockResolvedValue(null); // user dismissed the OS file picker
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("fold-merge-button"));
    await vi.waitFor(() => expect(handlers.onCancelled).toHaveBeenCalledTimes(1));

    expect(openMock).toHaveBeenCalledWith(
      expect.objectContaining({ multiple: true }),
    );
    expect(invokeMock).not.toHaveBeenCalled();
    expect(handlers.onError).not.toHaveBeenCalled();
    expect(screen.queryByTestId("fold-merge-dialog")).not.toBeInTheDocument();
  });

  it("choosing files opens FoldMergeDialog with them in the chosen order; no backend call yet", async () => {
    openMock.mockResolvedValue(FOLD_SOURCES);
    renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("fold-merge-button"));

    expect(await screen.findByTestId("fold-merge-dialog")).toBeInTheDocument();
    expect(screen.getByTestId("fold-merge-row-0")).toHaveTextContent("one.jwlibrary");
    expect(screen.getByTestId("fold-merge-row-1")).toHaveTextContent("two.jwlibrary");
    expect(screen.getByTestId("fold-merge-row-2")).toHaveTextContent("three.jwlibrary");
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it("Continue calls fold_merge_dry_run once with the displayed order and opens the aggregate preview", async () => {
    openMock.mockResolvedValue(FOLD_SOURCES);
    invokeMock.mockResolvedValue(FOLD_REPORT);
    renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("fold-merge-button"));
    await screen.findByTestId("fold-merge-dialog");
    fireEvent.click(screen.getByTestId("fold-merge-continue"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("fold_merge_dry_run", {
        sourcePaths: FOLD_SOURCES,
      }),
    );
    expect(await screen.findByTestId("edit-preview-dialog")).toBeInTheDocument();
    // 3 added (2+1), 3 overwritten, naming the number of source archives.
    expect(screen.getByTestId("edit-preview-summary")).toHaveTextContent(
      "3 records added, 3 updated from the combined effect of 3 archives",
    );
    expect(invokeMock).not.toHaveBeenCalledWith("fold_merge_commit", expect.anything());
  });

  it("reordering in the dialog before Continue changes the array passed to fold_merge_dry_run", async () => {
    openMock.mockResolvedValue(FOLD_SOURCES);
    invokeMock.mockResolvedValue(FOLD_REPORT);
    renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("fold-merge-button"));
    await screen.findByTestId("fold-merge-dialog");
    // Move row 2 (two.jwlibrary) up, ahead of row 1 (one.jwlibrary).
    fireEvent.click(screen.getByTestId("fold-merge-row-1-up"));
    fireEvent.click(screen.getByTestId("fold-merge-continue"));

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("fold_merge_dry_run", {
        sourcePaths: [
          "C:/archives/two.jwlibrary",
          "C:/archives/one.jwlibrary",
          "C:/archives/three.jwlibrary",
        ],
      }),
    );
  });

  it("Preview Confirm calls fold_merge_commit with the SAME array, then list_notes, then onOpened", async () => {
    openMock.mockResolvedValue(FOLD_SOURCES);
    invokeMock.mockResolvedValue(FOLD_REPORT);
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("fold-merge-button"));
    await screen.findByTestId("fold-merge-dialog");
    fireEvent.click(screen.getByTestId("fold-merge-continue"));
    const confirm = await screen.findByTestId("edit-preview-confirm");

    const foldedNotes: BrowseRow[] = [];
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "fold_merge_commit") return Promise.resolve(undefined);
      if (cmd === "list_notes") return Promise.resolve(foldedNotes);
      return Promise.resolve(FOLD_REPORT);
    });
    fireEvent.click(confirm);

    await vi.waitFor(() =>
      expect(invokeMock).toHaveBeenCalledWith("fold_merge_commit", {
        sourcePaths: FOLD_SOURCES,
      }),
    );
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledWith("list_notes"));
    await vi.waitFor(() => expect(handlers.onOpened).toHaveBeenCalledWith(foldedNotes));
  });

  it("Preview Cancel makes no fold_merge_commit call", async () => {
    openMock.mockResolvedValue(FOLD_SOURCES);
    invokeMock.mockResolvedValue(FOLD_REPORT);
    renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("fold-merge-button"));
    await screen.findByTestId("fold-merge-dialog");
    fireEvent.click(screen.getByTestId("fold-merge-continue"));
    const cancel = await screen.findByTestId("edit-preview-cancel");
    fireEvent.click(cancel);

    await vi.waitFor(() =>
      expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument(),
    );
    expect(invokeMock).not.toHaveBeenCalledWith("fold_merge_commit", expect.anything());
  });

  it("a rejected fold_merge_dry_run routes to onError and opens no preview", async () => {
    openMock.mockResolvedValue(FOLD_SOURCES);
    const dto: ErrorDto = {
      code: "merge_unavailable",
      operation: "fold_merge_dry_run",
      safe_file_name: "one.jwlibrary",
      message_key: "error.merge.unavailable",
    };
    invokeMock.mockRejectedValue(dto);
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("fold-merge-button"));
    await screen.findByTestId("fold-merge-dialog");
    fireEvent.click(screen.getByTestId("fold-merge-continue"));

    await vi.waitFor(() => expect(handlers.onError).toHaveBeenCalledWith(dto));
    expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument();
  });

  it("a rejected fold_merge_commit routes to onError and closes the preview", async () => {
    openMock.mockResolvedValue(FOLD_SOURCES);
    invokeMock.mockResolvedValue(FOLD_REPORT);
    const handlers = renderBar({ archiveOpen: true });

    fireEvent.click(screen.getByTestId("fold-merge-button"));
    await screen.findByTestId("fold-merge-dialog");
    fireEvent.click(screen.getByTestId("fold-merge-continue"));
    const confirm = await screen.findByTestId("edit-preview-confirm");

    const dto: ErrorDto = {
      code: "merge_unavailable",
      operation: "fold_merge_commit",
      safe_file_name: "one.jwlibrary",
      message_key: "error.merge.unavailable",
    };
    invokeMock.mockRejectedValue(dto);
    fireEvent.click(confirm);

    await vi.waitFor(() => expect(handlers.onError).toHaveBeenCalledWith(dto));
    await vi.waitFor(() =>
      expect(screen.queryByTestId("edit-preview-dialog")).not.toBeInTheDocument(),
    );
  });

  it("the fold-merge action is aria-busy during the picker phase and cannot be re-invoked meanwhile", async () => {
    let resolveOpen: (value: string[]) => void = () => {};
    openMock.mockImplementation(
      () =>
        new Promise<string[]>((resolve) => {
          resolveOpen = resolve;
        }),
    );
    renderBar({ archiveOpen: true });

    const button = screen.getByTestId("fold-merge-button");
    fireEvent.click(button);
    fireEvent.click(button); // rapid second click while the picker promise is pending

    expect(openMock).toHaveBeenCalledTimes(1);
    expect(await screen.findByRole("button", { name: /preparing/i })).toBeDisabled();

    resolveOpen(FOLD_SOURCES);
    await vi.waitFor(() => expect(screen.getByTestId("fold-merge-dialog")).toBeInTheDocument());
  });
});
