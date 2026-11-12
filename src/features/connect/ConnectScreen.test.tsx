import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ConnectScreen } from "./ConnectScreen";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";
import { makeProfile, makeSettings, runningStatus, stoppedStatus } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  connect: vi.fn(),
  disconnect: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

describe("ConnectScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
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

  it("показывает время сессии из статуса helper'а", () => {
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
    expect(screen.getByText("00:01:05")).toBeInTheDocument();
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
});
