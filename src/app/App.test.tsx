import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { resetAppStore } from "../shared/state/appStore";
import { makeSettings, stoppedStatus } from "../test/fixtures";

const apiMock = vi.hoisted(() => ({
  settingsGet: vi.fn(),
  rulesList: vi.fn(),
  profilesList: vi.fn(),
  helperStatus: vi.fn(),
  logsList: vi.fn(),
  updateStatus: vi.fn(),
}));

vi.mock("../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  onUpdateProgress: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

describe("App", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
    apiMock.settingsGet.mockResolvedValue(makeSettings());
    apiMock.rulesList.mockResolvedValue([]);
    apiMock.profilesList.mockResolvedValue({
      profiles: [],
      selected_profile_id: null,
      fetched_at: null,
    });
    apiMock.helperStatus.mockResolvedValue(stoppedStatus);
    apiMock.logsList.mockResolvedValue([]);
    apiMock.updateStatus.mockResolvedValue({
      current_version: "0.1.0",
      channel: "stable",
      configured: true,
    });
  });

  it("показывает заголовок и вкладки", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "SplitRoute" })).toBeInTheDocument();
    for (const label of ["Главная", "Локации", "Приложения", "Настройки", "Логи"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  it("переключает экраны", async () => {
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: "Приложения" }));
    expect(screen.getByRole("heading", { name: "Приложения" })).toBeInTheDocument();
  });
});
