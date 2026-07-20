import { render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi, beforeEach } from "vitest";
import ErrorBanner from "./ErrorBanner";
import JwlCoreNotice from "./JwlCoreNotice";
import { describeError, isZipSlipRejection } from "../lib/errors";
import type { ErrorDto } from "../bindings/ErrorDto";
import type { JwlCoreStatus } from "../bindings/JwlCoreStatus";

const invokeMock = vi.fn();

vi.mock("@tauri-apps/api/core", () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

beforeEach(() => {
  invokeMock.mockReset();
});

function makeError(overrides: Partial<ErrorDto> = {}): ErrorDto {
  return {
    code: "not_a_zip",
    operation: "open_archive",
    safe_file_name: "archive.jwlibrary",
    message_key: "error.archive.not_a_zip",
    ...overrides,
  };
}

const ALL_KNOWN_CODES = [
  "not_a_zip",
  "missing_manifest",
  "missing_user_data_backup",
  "unsupported_schema",
  "zip_slip_rejected",
  "state_poisoned",
  "missing_resources_language",
  "missing_resources_db",
  "io_error",
  "sqlite_error",
  "zip_error",
  "json_error",
];

describe("ErrorBanner / lib/errors mapping", () => {
  it("maps every known ErrorDto code to a specific, non-blank sentence", () => {
    const seen = new Set<string>();
    for (const code of ALL_KNOWN_CODES) {
      const sentence = describeError(makeError({ code }));
      expect(sentence.length).toBeGreaterThan(20);
      expect(seen.has(sentence)).toBe(false); // every code maps to a DISTINCT sentence
      seen.add(sentence);
    }
  });

  it("never renders a bare code or the message_key alone", () => {
    for (const code of ALL_KNOWN_CODES) {
      const err = makeError({ code, message_key: `error.archive.${code}` });
      const sentence = describeError(err);
      expect(sentence).not.toBe(err.code);
      expect(sentence).not.toBe(err.message_key);
    }
  });

  it("never renders a raw Rust Display-style string (no ErrorDto field is a raw path/source fragment)", () => {
    render(<ErrorBanner error={makeError()} />);
    const banner = screen.getByTestId("error-banner");
    // A raw Display string would contain characters/patterns like "Os {" or
    // a full Windows/Unix path; the rendered sentence must not.
    expect(banner.textContent).not.toMatch(/Os \{|errno|C:\\\\|\/home\//);
  });

  it("renders the zip-slip rejection as distinct security copy, not the generic open-failure sentence", () => {
    const zipSlipErr = makeError({ code: "zip_slip_rejected" });
    const genericErr = makeError({ code: "not_a_zip" });

    expect(isZipSlipRejection(zipSlipErr)).toBe(true);
    expect(isZipSlipRejection(genericErr)).toBe(false);
    expect(describeError(zipSlipErr)).not.toBe(describeError(genericErr));
    expect(describeError(zipSlipErr)).toMatch(/write outside the extraction folder/i);

    render(<ErrorBanner error={zipSlipErr} />);
    const banner = screen.getByTestId("error-banner");
    expect(banner.className).toContain("error-banner-security");
  });

  it("is sticky: rendering does not auto-dismiss (no timers/effects clear it)", () => {
    vi.useFakeTimers();
    render(<ErrorBanner error={makeError()} />);
    vi.advanceTimersByTime(60_000);
    expect(screen.getByTestId("error-banner")).toBeInTheDocument();
    vi.useRealTimers();
  });

  it("appends the safe_file_name (never a full path) when present", () => {
    render(<ErrorBanner error={makeError({ safe_file_name: "notes.jwlibrary" })} />);
    expect(screen.getByTestId("error-banner").textContent).toContain("notes.jwlibrary");
  });
});

describe("JwlCoreNotice", () => {
  it("shows the muted capability notice when JwlCoreStatus.loaded is false with a reason", async () => {
    const status: JwlCoreStatus = {
      loaded: false,
      arch: "aarch64",
      version: null,
      reason: "no jwlCore build exists yet for Windows on Arm",
    };
    invokeMock.mockResolvedValue(status);
    render(<JwlCoreNotice />);

    const notice = await screen.findByTestId("jwlcore-notice");
    expect(notice).toBeInTheDocument();
    expect(notice.textContent).toMatch(/merge isn.t available/i);
    expect(notice.className).not.toMatch(/error/i);
  });

  it("never renders when JwlCoreStatus.loaded is true", async () => {
    const status: JwlCoreStatus = {
      loaded: true,
      arch: "x86_64",
      version: "1.2.3",
      reason: null,
    };
    invokeMock.mockResolvedValue(status);
    render(<JwlCoreNotice />);

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_jwlcore"));
    expect(screen.queryByTestId("jwlcore-notice")).not.toBeInTheDocument();
  });

  it("distinguishes a genuine check_jwlcore Err from a non-loaded Ok: an Err never renders the notice", async () => {
    invokeMock.mockRejectedValue({
      LoadFailed: { reason: "corrupt binary" },
    });
    render(<JwlCoreNotice />);

    await waitFor(() => expect(invokeMock).toHaveBeenCalledWith("check_jwlcore"));
    expect(screen.queryByTestId("jwlcore-notice")).not.toBeInTheDocument();
  });
});
