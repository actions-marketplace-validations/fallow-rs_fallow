import * as fs from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { expect, it, vi } from "vitest";

vi.mock("node:fs", async (importOriginal) => {
  const actual = await importOriginal<typeof import("node:fs")>();
  return { ...actual, writeSync: vi.fn(actual.writeSync) };
});
vi.mock("vscode", () => ({}));

import { tryAcquireInstallLock, withInstallLock } from "../src/download.js";

it("removes its own lock when writing its owner fails", ({ onTestFinished }) => {
  const directory = fs.mkdtempSync(join(tmpdir(), "fallow-install-lock-"));
  onTestFinished(() => fs.rmSync(directory, { recursive: true, force: true }));
  const lock = join(directory, ".install.lock");
  const failure = Object.assign(new Error("disk full"), { code: "ENOSPC" });
  vi.mocked(fs.writeSync).mockImplementationOnce(() => {
    throw failure;
  });

  expect(() => tryAcquireInstallLock(lock)).toThrow(failure);
  expect(fs.existsSync(lock)).toBe(false);
  expect(tryAcquireInstallLock(lock)).toBe(true);
  expect(tryAcquireInstallLock(lock)).toBe(false);
  expect(fs.readFileSync(lock, "utf8")).toBe(String(process.pid));
});

it("cleans up before falling back to an unlocked install", async ({ onTestFinished }) => {
  const directory = fs.mkdtempSync(join(tmpdir(), "fallow-install-lock-"));
  onTestFinished(() => fs.rmSync(directory, { recursive: true, force: true }));
  const lock = join(directory, ".install.lock");
  vi.mocked(fs.writeSync).mockImplementationOnce(() => {
    throw Object.assign(new Error("disk full"), { code: "ENOSPC" });
  });

  await withInstallLock(directory, async () => {
    expect(fs.existsSync(lock)).toBe(false);
  });
  await withInstallLock(directory, async () => {
    expect(fs.readFileSync(lock, "utf8")).toBe(String(process.pid));
  });
  expect(fs.existsSync(lock)).toBe(false);
});
