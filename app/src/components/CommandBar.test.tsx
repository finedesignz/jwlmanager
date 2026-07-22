import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import CommandBar from "./CommandBar";
import type { NotesRow } from "../bindings/NotesRow";
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
    const notes: NotesRow[] = [];
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
    expect(await screen.findByTestId("delete-preview-dialog")).toBeInTheDocument();
    expect(screen.getByTestId("delete-preview-summary")).toHaveTextContent(
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
    const confirm = await screen.findByTestId("delete-preview-confirm");
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
    const cancel = await screen.findByTestId("delete-preview-cancel");
    fireEvent.click(cancel);

    await vi.waitFor(() =>
      expect(screen.queryByTestId("delete-preview-dialog")).not.toBeInTheDocument(),
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
    expect(screen.queryByTestId("delete-preview-dialog")).not.toBeInTheDocument();
  });

  it("disables the Save v14 action when no archive is open", () => {
    renderBar({ archiveOpen: false });
    expect(screen.getByTestId("save-v14-button")).toBeDisabled();
  });
});
