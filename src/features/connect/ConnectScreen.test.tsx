import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ConnectScreen } from "./ConnectScreen";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";
import { makeProfile, makeSettings, runningStatus, stoppedStatus } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  connect: vi.fn(),
  disconnect: vi.fn(),
  trafficStats: vi.fn(),
  helperStatus: vi.fn(),
  profilesPing: vi.fn(),
  profileSwitch: vi.fn(),
  reconnectStatus: vi.fn(),
}));

const noTraffic = { available: false, upload_total: 0, download_total: 0 };
const emptyLatency = {
  results: [],
  best_profile_id: null,
  switch_to: null,
  checked_at: 1,
};

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  onAutoconnectFailed: vi.fn(() => Promise.resolve(() => {})),
  onReconnectProgress: vi.fn(() => Promise.resolve(() => {})),
  onReconnectFailed: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

describe("ConnectScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    apiMock.trafficStats.mockReset();
    apiMock.trafficStats.mockResolvedValue(noTraffic);
    apiMock.helperStatus.mockReset();
    apiMock.helperStatus.mockResolvedValue(stoppedStatus);
    apiMock.profilesPing.mockReset();
    apiMock.profilesPing.mockResolvedValue(emptyLatency);
    apiMock.profileSwitch.mockReset();
    resetAppStore();
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it("показывает пустое состояние без локации и блокирует подключение", () => {
    useAppStore.setState({ settings: makeSettings(), helperStatus: stoppedStatus });
    render(<ConnectScreen />);

    expect(screen.getByText("Локация не выбрана")).toBeInTheDocument();
    expect(screen.getByText(/Подписка не добавлена/)).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Подключить" })).toBeDisabled();
    expect(screen.getByText("Через прокси")).toBeInTheDocument();
    expect(
      screen.queryByText(/уходит в локальный resolver/),
    ).not.toBeInTheDocument();
  });

  it("показывает предупреждение об утечке в системном режиме", () => {
    const settings = makeSettings();
    settings.network.dns_mode = "system";
    useAppStore.setState({ settings, helperStatus: stoppedStatus });
    render(<ConnectScreen />);

    expect(screen.getByText("Системный")).toBeInTheDocument();
    expect(
      screen.getByText(/уходит в локальный resolver/),
    ).toBeInTheDocument();
  });

  it("подключается и показывает статус и локацию", async () => {
    const user = userEvent.setup();
    apiMock.connect.mockResolvedValue(runningStatus);
    useAppStore.setState({
      settings: makeSettings(),
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: stoppedStatus,
      connection: "disconnected",
    });

    render(<ConnectScreen />);
    expect(screen.getByText("Тестовая локация A")).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Подключить" }));

    await waitFor(() => expect(apiMock.connect).toHaveBeenCalledTimes(1));
    await waitFor(() =>
      expect(screen.getByText("Подключено")).toBeInTheDocument(),
    );
  });

  it("показывает время сессии из статуса helper'а", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-11-12T12:00:00Z"));
    const now = Math.floor(Date.now() / 1000);
    useAppStore.setState({
      settings: makeSettings(),
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: { state: "running", pid: 42, started_at: now - 65 },
      connection: "connected",
    });

    render(<ConnectScreen />);
    await act(async () => {});
    expect(screen.getByText("00:01:05")).toBeInTheDocument();
  });

  it("показывает ↑/↓ за сессию в читаемом виде", async () => {
    apiMock.trafficStats.mockResolvedValue({
      available: true,
      upload_total: 1536,
      download_total: 5 * 1024 * 1024,
    });
    useAppStore.setState({
      settings: makeSettings(),
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: runningStatus,
      connection: "connected",
    });

    render(<ConnectScreen />);

    expect(await screen.findByText("1,5 КиБ")).toBeInTheDocument();
    expect(screen.getByText("5 МиБ")).toBeInTheDocument();
    expect(screen.getByLabelText("Отправлено")).toBeInTheDocument();
    expect(screen.getByLabelText("Получено")).toBeInTheDocument();
  });

  it("при недоступном источнике показывает «—» без общей ошибки", async () => {
    apiMock.trafficStats.mockResolvedValue({
      available: false,
      upload_total: 0,
      download_total: 0,
    });
    useAppStore.setState({
      settings: makeSettings(),
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: runningStatus,
      connection: "connected",
    });

    render(<ConnectScreen />);

    await waitFor(() => expect(apiMock.trafficStats).toHaveBeenCalled());
    expect(screen.getByText("Трафик за сессию:")).toBeInTheDocument();
    expect(screen.getByText("—")).toBeInTheDocument();
    expect(screen.queryByText("Ошибка")).not.toBeInTheDocument();
  });

  it("обновляет счётчики раз в секунду без перезагрузки", async () => {
    vi.useFakeTimers();
    apiMock.trafficStats
      .mockResolvedValueOnce({
        available: true,
        upload_total: 1024,
        download_total: 0,
      })
      .mockResolvedValueOnce({
        available: true,
        upload_total: 2048,
        download_total: 0,
      });
    useAppStore.setState({
      settings: makeSettings(),
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: runningStatus,
      connection: "connected",
    });

    render(<ConnectScreen />);
    await act(async () => {});
    expect(screen.getByText("1 КиБ")).toBeInTheDocument();

    await act(async () => {
      vi.advanceTimersByTime(1000);
    });
    expect(screen.getByText("2 КиБ")).toBeInTheDocument();
  });

  it("показывает активный kill switch из статуса helper'а", () => {
    const settings = makeSettings();
    settings.network.kill_switch = true;
    useAppStore.setState({
      settings,
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: {
        ...runningStatus,
        kill_switch: { enabled: true },
      },
      connection: "connected",
    });

    render(<ConnectScreen />);
    expect(screen.getByText("Активен")).toBeInTheDocument();
    expect(screen.queryByText("Выключен")).not.toBeInTheDocument();
  });

  it("показывает ошибку правил kill switch", () => {
    const settings = makeSettings();
    settings.network.kill_switch = true;
    useAppStore.setState({
      settings,
      helperStatus: {
        ...runningStatus,
        kill_switch: { enabled: false, error: "нет прав root" },
      },
      connection: "connected",
    });

    render(<ConnectScreen />);
    expect(screen.getByText("Ошибка")).toBeInTheDocument();
    expect(screen.getByText("нет прав root")).toBeInTheDocument();
  });

  it("при падении ядра с kill switch показывает ошибку и кнопку снятия защиты", async () => {
    vi.useFakeTimers();
    const settings = makeSettings();
    settings.network.kill_switch = true;
    useAppStore.setState({
      settings,
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: {
        ...runningStatus,
        kill_switch: { enabled: true },
      },
      connection: "connected",
    });
    apiMock.helperStatus.mockResolvedValue({
      state: "stopped",
      kill_switch: { enabled: true },
    });

    render(<ConnectScreen />);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });

    expect(
      screen.getByText("Ядро остановилось. Kill switch блокирует прямой трафик."),
    ).toBeInTheDocument();
    expect(
      screen.getByText(/Kill switch блокирует прямой трафик — отключитесь/),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Отключить и снять защиту" }),
    ).toBeInTheDocument();
  });

  it("различает состояния helper'а: нет, устарел, не отвечает", () => {
    useAppStore.setState({
      settings: makeSettings(),
      helperStatus: null,
      helperStatusError: null,
    });
    const { rerender } = render(<ConnectScreen />);
    expect(screen.getByText("Не установлен")).toBeInTheDocument();

    useAppStore.setState({
      helperStatus: null,
      helperStatusError: { code: "HELPER_PROTOCOL", message: "версия" },
    });
    rerender(<ConnectScreen />);
    expect(screen.getByText("Устарел")).toBeInTheDocument();

    useAppStore.setState({
      helperStatus: null,
      helperStatusError: { code: "HELPER_UNAVAILABLE", message: "нет ответа" },
    });
    rerender(<ConnectScreen />);
    expect(screen.getByText("Не отвечает")).toBeInTheDocument();
  });

  it("показывает состояние автовыбора локации", () => {
    const settings = makeSettings();
    settings.preferences.auto_select.auto_switch = true;
    useAppStore.setState({
      settings,
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: runningStatus,
      connection: "connected",
    });

    render(<ConnectScreen />);
    expect(screen.getByText("Автовыбор локации")).toBeInTheDocument();
    expect(screen.getByText("Включён")).toBeInTheDocument();
    expect(screen.getByText("Проверка каждые 60 с")).toBeInTheDocument();
  });

  it("автопереключается по рекомендации Rust", async () => {
    const settings = makeSettings();
    settings.preferences.auto_select.auto_switch = true;
    const first = makeProfile({ id: "profile-1", name: "Первая" });
    const second = makeProfile({ id: "profile-2", name: "Вторая" });
    apiMock.profilesPing.mockResolvedValue({
      results: [
        { profile_id: "profile-1", latency_ms: 300 },
        { profile_id: "profile-2", latency_ms: 40 },
      ],
      best_profile_id: "profile-2",
      switch_to: "profile-2",
      checked_at: 1,
    });
    apiMock.profileSwitch.mockResolvedValue({
      profiles: [first, second],
      selected_profile_id: "profile-2",
      fetched_at: 1,
    });
    useAppStore.setState({
      settings,
      profiles: [first, second],
      selectedProfileId: "profile-1",
      helperStatus: runningStatus,
      connection: "connected",
    });

    render(<ConnectScreen />);

    await waitFor(() =>
      expect(apiMock.profileSwitch).toHaveBeenCalledWith("profile-2"),
    );
    await waitFor(() =>
      expect(useAppStore.getState().selectedProfileId).toBe("profile-2"),
    );
  });

  it("показывает прогресс серии автореконнекта (S4-T15)", () => {
    useAppStore.setState({
      settings: makeSettings(),
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: stoppedStatus,
      connection: "error",
      reconnect: { attempt: 2, max_attempts: 3, exhausted: null },
    });

    render(<ConnectScreen />);
    expect(
      screen.getByText("Автореконнект: попытка 2 из 3 — правила kill switch сохраняются"),
    ).toBeInTheDocument();
  });

  it("успешное подключение снимает ошибку падения ядра и прогресс серии", async () => {
    const user = userEvent.setup();
    apiMock.connect.mockResolvedValue(runningStatus);
    useAppStore.setState({
      settings: makeSettings(),
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
      helperStatus: stoppedStatus,
      connection: "error",
      actionError: { code: "CORE_STOPPED", message: "" },
      reconnect: { attempt: 1, max_attempts: 3, exhausted: null },
    });

    render(<ConnectScreen />);
    expect(
      screen.getByText("Ядро остановилось. Kill switch блокирует прямой трафик."),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Подключить" }));

    await waitFor(() =>
      expect(
        screen.queryByText(
          "Ядро остановилось. Kill switch блокирует прямой трафик.",
        ),
      ).not.toBeInTheDocument(),
    );
    expect(
      screen.queryByText(/Автореконнект: попытка/),
    ).not.toBeInTheDocument();
    expect(useAppStore.getState().reconnect).toBeNull();
  });
});
