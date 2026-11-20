import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";
import { onConnectionChanged, onReconnectFailed } from "../shared/api/client";
import { resetAppStore, useAppStore } from "../shared/state/appStore";
import { makeSettings, stoppedStatus } from "../test/fixtures";

const apiMock = vi.hoisted(() => ({
  settingsGet: vi.fn(),
  rulesList: vi.fn(),
  profilesList: vi.fn(),
  helperStatus: vi.fn(),
  autoconnectStatus: vi.fn(),
  reconnectStatus: vi.fn(),
  logsList: vi.fn(),
  updateStatus: vi.fn(),
  trafficStats: vi.fn(),
}));

vi.mock("../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  onAutoconnectFailed: vi.fn(() => Promise.resolve(() => {})),
  onReconnectProgress: vi.fn(() => Promise.resolve(() => {})),
  onReconnectFailed: vi.fn(() => Promise.resolve(() => {})),
  onSubscriptionUpdated: vi.fn(() => Promise.resolve(() => {})),
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
      subscriptions: [],
      profiles: [],
      selected_profile_id: null,
    });
    apiMock.helperStatus.mockResolvedValue(stoppedStatus);
    apiMock.autoconnectStatus.mockResolvedValue(null);
    apiMock.reconnectStatus.mockResolvedValue({
      attempt: 0,
      max_attempts: 3,
      exhausted: null,
    });
    apiMock.logsList.mockResolvedValue([]);
    apiMock.updateStatus.mockResolvedValue({
      current_version: "0.1.0",
      channel: "stable",
      configured: true,
    });
    apiMock.trafficStats.mockResolvedValue({
      available: false,
      upload_total: 0,
      download_total: 0,
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

  it("показывает ошибку автоподключения при запуске (B2)", async () => {
    apiMock.autoconnectStatus.mockResolvedValue({
      code: "HELPER_NOT_INSTALLED",
      message: "helper не установлен",
    });
    render(<App />);

    expect(await screen.findByText("Helper не установлен.")).toBeInTheDocument();
    // Остальные экраны и ручное подключение остаются доступными.
    expect(screen.getByRole("button", { name: "Подключить" })).toBeInTheDocument();
  });

  /// Серия автореконнекта (S4-T15): событие исчерпания показывает ошибку,
  /// восстановление ядра снимает её без ручного действия.
  it("показывает исчерпание серии автореконнекта и снимает его при восстановлении", async () => {
    let failHandler: ((error: { code: string; message: string }) => void) | undefined;
    let changedHandler:
      | ((status: { state: "running" | "stopped"; pid?: number }) => void)
      | undefined;
    vi.mocked(onReconnectFailed).mockImplementation((handler) => {
      failHandler = handler;
      return Promise.resolve(() => {});
    });
    vi.mocked(onConnectionChanged).mockImplementation((handler) => {
      changedHandler = handler;
      return Promise.resolve(() => {});
    });

    render(<App />);
    await waitFor(() => {
      expect(failHandler).toBeDefined();
      expect(changedHandler).toBeDefined();
    });

    act(() => {
      failHandler?.({ code: "RECONNECT_FAILED", message: "" });
    });
    expect(
      await screen.findByText(
        "Ядро не удалось подключить заново автоматически (3 попытки). При включённом kill switch прямой трафик остаётся заблокированным — подключитесь или отключитесь вручную.",
      ),
    ).toBeInTheDocument();
    expect(useAppStore.getState().connection).toBe("error");

    act(() => {
      changedHandler?.({ state: "running", pid: 42 });
    });
    await waitFor(() =>
      expect(
        screen.queryByText(
          "Ядро не удалось подключить заново автоматически (3 попытки). При включённом kill switch прямой трафик остаётся заблокированным — подключитесь или отключитесь вручную.",
        ),
      ).not.toBeInTheDocument(),
    );
    expect(useAppStore.getState().connection).toBe("connected");
    expect(useAppStore.getState().reconnect).toBeNull();
  });

  /// Статус серии читается при старте окна: серия могла идти при закрытом окне.
  it("показывает прогресс серии автореконнекта после открытия окна", async () => {
    apiMock.reconnectStatus.mockResolvedValue({
      attempt: 1,
      max_attempts: 3,
      exhausted: null,
    });
    render(<App />);

    await waitFor(() =>
      expect(useAppStore.getState().reconnect).toEqual({
        attempt: 1,
        max_attempts: 3,
        exhausted: null,
      }),
    );
    expect(
      await screen.findByText(
        "Автореконнект: попытка 1 из 3 — правила kill switch сохраняются",
      ),
    ).toBeInTheDocument();
  });
});
