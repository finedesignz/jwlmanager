import { fireEvent, render as rtlRender, screen, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactElement } from "react";
import MediaAddDialog from "./MediaAddDialog";
import { I18nProvider } from "../i18n/I18nContext";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { MediaAddApplyReport } from "../bindings/MediaAddApplyReport";
import type { MediaPrecheckResult } from "../bindings/MediaPrecheckResult";

function render(ui: ReactElement) {
  return rtlRender(
    <I18nProvider locale="en" setLocale={() => {}}>
      {ui}
    </I18nProvider>,
  );
}

const invokeMock = vi.fn();
const openMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

vi.mock("@tauri-apps/plugin-dialog", () => ({
  open: (...args: unknown[]) => openMock(...args),
}));

beforeEach(() => {
  invokeMock.mockReset();
  openMock.mockReset();
});

function precheck(
  overrides: Partial<MediaPrecheckResult> & { path: string },
): MediaPrecheckResult {
  return { status: "new", reason: null, ...overrides };
}

async function pickFilesAndPrecheck(results: MediaPrecheckResult[]) {
  openMock.mockResolvedValue(results.map((r) => r.path));
  invokeMock.mockImplementation((cmd: string) => {
    if (cmd === "media_add_precheck") return Promise.resolve(results);
    return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
  });
  render(<MediaAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);
  fireEvent.click(screen.getByTestId("media-add-pick-files"));
  await screen.findByTestId("media-add-dialog");
}

describe("MediaAddDialog — pre-check (D8-06, 08-06-PLAN.md Task 2)", () => {
  it("invokes media_add_precheck exactly once per pick and renders one row per file", async () => {
    await pickFilesAndPrecheck([
      precheck({ path: "/tmp/a.png", status: "new" }),
      precheck({ path: "/tmp/b.png", status: "duplicate" }),
    ]);

    await waitFor(() => expect(screen.getByTestId("media-add-file-row-0")).toBeInTheDocument());
    expect(screen.getByTestId("media-add-file-row-1")).toBeInTheDocument();
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith("media_add_precheck", {
      paths: ["/tmp/a.png", "/tmp/b.png"],
    });
  });

  it("disables Confirm and shows the all-duplicates sentence when every file is a duplicate", async () => {
    await pickFilesAndPrecheck([precheck({ path: "/tmp/a.png", status: "duplicate" })]);

    await waitFor(() =>
      expect(screen.getByTestId("media-add-all-duplicates")).toHaveTextContent(
        "All selected files are already in this archive.",
      ),
    );
    expect(screen.queryByTestId("media-add-confirm")).not.toBeInTheDocument();
  });

  it('label reads "Add Media (N)" where N is the count of new rows', async () => {
    await pickFilesAndPrecheck([
      precheck({ path: "/tmp/a.png", status: "new" }),
      precheck({ path: "/tmp/b.png", status: "new" }),
      precheck({ path: "/tmp/c.png", status: "duplicate" }),
    ]);
    fireEvent.change(screen.getByTestId("media-add-playlist-name"), {
      target: { value: "My Playlist" },
    });

    await waitFor(() =>
      expect(screen.getByTestId("media-add-confirm")).toHaveTextContent("Add Media (2)"),
    );
  });
});

describe("MediaAddDialog — apply (PD-3, 08-06-PLAN.md Task 1/2)", () => {
  async function confirmedDialog(applyReport: MediaAddApplyReport) {
    const results = [precheck({ path: "/tmp/a.png", status: "new" })];
    openMock.mockResolvedValue(results.map((r) => r.path));
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "media_add_precheck") return Promise.resolve(results);
      if (cmd === "media_add_apply") return Promise.resolve(applyReport);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<MediaAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);
    fireEvent.click(screen.getByTestId("media-add-pick-files"));
    await screen.findByTestId("media-add-file-row-0");
    fireEvent.change(screen.getByTestId("media-add-playlist-name"), {
      target: { value: "My Playlist" },
    });
    fireEvent.click(screen.getByTestId("media-add-confirm"));
  }

  it("invokes media_add_apply exactly once even under a double click", async () => {
    let resolveApply: (report: MediaAddApplyReport) => void = () => {};
    const results = [precheck({ path: "/tmp/a.png", status: "new" })];
    openMock.mockResolvedValue(results.map((r) => r.path));
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "media_add_precheck") return Promise.resolve(results);
      if (cmd === "media_add_apply")
        return new Promise<MediaAddApplyReport>((resolve) => {
          resolveApply = resolve;
        });
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<MediaAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);
    fireEvent.click(screen.getByTestId("media-add-pick-files"));
    await screen.findByTestId("media-add-file-row-0");
    fireEvent.change(screen.getByTestId("media-add-playlist-name"), {
      target: { value: "My Playlist" },
    });

    const confirmButton = screen.getByTestId("media-add-confirm");
    fireEvent.click(confirmButton);
    fireEvent.click(confirmButton); // double-click guard

    const applyInvocations = invokeMock.mock.calls.filter(([cmd]) => cmd === "media_add_apply");
    expect(applyInvocations).toHaveLength(1);

    resolveApply({ added: 1 });
    await waitFor(() => expect(screen.getByTestId("media-add-done")).toBeInTheDocument());
  });

  it('shows a determinate "Copying files… i of N" counter while apply is in flight', async () => {
    let resolveApply: (report: MediaAddApplyReport) => void = () => {};
    const results = [precheck({ path: "/tmp/a.png", status: "new" })];
    openMock.mockResolvedValue(results.map((r) => r.path));
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "media_add_precheck") return Promise.resolve(results);
      if (cmd === "media_add_apply")
        return new Promise<MediaAddApplyReport>((resolve) => {
          resolveApply = resolve;
        });
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<MediaAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={vi.fn()} />);
    fireEvent.click(screen.getByTestId("media-add-pick-files"));
    await screen.findByTestId("media-add-file-row-0");
    fireEvent.change(screen.getByTestId("media-add-playlist-name"), {
      target: { value: "My Playlist" },
    });
    fireEvent.click(screen.getByTestId("media-add-confirm"));

    await waitFor(() =>
      expect(screen.getByTestId("media-add-list-header")).toHaveTextContent("Copying files… 0 of 1"),
    );
    resolveApply({ added: 1 });
    await waitFor(() => expect(screen.getByTestId("media-add-done")).toBeInTheDocument());
  });

  it("post-completion shows final glyphs and an added/failed summary with the failed clause omitted at zero", async () => {
    await confirmedDialog({ added: 1 });

    await waitFor(() =>
      expect(screen.getByTestId("media-add-list-header")).toHaveTextContent("1 added."),
    );
    expect(screen.getByTestId("media-add-list-header")).not.toHaveTextContent("failed");
    expect(screen.getByTestId("media-add-done")).toBeInTheDocument();
  });

  it("Done closes the dialog via onApplied", async () => {
    const onApplied = vi.fn();
    const results = [precheck({ path: "/tmp/a.png", status: "new" })];
    openMock.mockResolvedValue(results.map((r) => r.path));
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "media_add_precheck") return Promise.resolve(results);
      if (cmd === "media_add_apply") return Promise.resolve({ added: 1 });
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<MediaAddDialog onApplied={onApplied} onCancel={vi.fn()} onError={vi.fn()} />);
    fireEvent.click(screen.getByTestId("media-add-pick-files"));
    await screen.findByTestId("media-add-file-row-0");
    fireEvent.change(screen.getByTestId("media-add-playlist-name"), {
      target: { value: "My Playlist" },
    });
    fireEvent.click(screen.getByTestId("media-add-confirm"));

    await waitFor(() => expect(screen.getByTestId("media-add-done")).toBeInTheDocument());
    fireEvent.click(screen.getByTestId("media-add-done"));
    expect(onApplied).toHaveBeenCalledTimes(1);
  });

  it("an apply-path failure leaves no row displaying an added state", async () => {
    const onError = vi.fn();
    const results = [precheck({ path: "/tmp/a.png", status: "new" })];
    openMock.mockResolvedValue(results.map((r) => r.path));
    const err: ErrorDto = {
      code: "media_add_failed",
      operation: "media_add_apply",
      safe_file_name: null,
      message_key: "error.archive.media_add_failed",
    };
    invokeMock.mockImplementation((cmd: string) => {
      if (cmd === "media_add_precheck") return Promise.resolve(results);
      if (cmd === "media_add_apply") return Promise.reject(err);
      return Promise.reject(new Error(`unexpected invoke: ${cmd}`));
    });
    render(<MediaAddDialog onApplied={vi.fn()} onCancel={vi.fn()} onError={onError} />);
    fireEvent.click(screen.getByTestId("media-add-pick-files"));
    await screen.findByTestId("media-add-file-row-0");
    fireEvent.change(screen.getByTestId("media-add-playlist-name"), {
      target: { value: "My Playlist" },
    });
    fireEvent.click(screen.getByTestId("media-add-confirm"));

    await waitFor(() => expect(onError).toHaveBeenCalledWith(err));
    // No row was ever flipped to "added" — the row's status glyph classes
    // never include the added-state class.
    const row = screen.getByTestId("media-add-file-row-0");
    expect(row.querySelector(".media-add-status-added")).not.toBeInTheDocument();
    // The dialog stays open on the pre-confirm surface (retry is possible).
    expect(screen.getByTestId("media-add-confirm")).toBeInTheDocument();
  });
});
