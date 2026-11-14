import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ProfilesScreen } from "./ProfilesScreen";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";
import { makeProfile } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  profileSelect: vi.fn(),
  profilesPing: vi.fn(),
  subscriptionFetch: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

describe("ProfilesScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
  });

  it("показывает пустое состояние и переход к настройкам", () => {
    render(<ProfilesScreen />);
    expect(screen.getByText("Нет локаций")).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Открыть настройки" }),
    ).toBeInTheDocument();
  });

  it("показывает ошибку подписки", () => {
    useAppStore.setState({
      fetchError: { code: "NETWORK", message: "нет сети" },
    });
    render(<ProfilesScreen />);
    expect(
      screen.getByText("Нет сети или сервер недоступен."),
    ).toBeInTheDocument();
  });

  it("показывает понятные тексты для ошибок подписки", () => {
    const cases: Array<[string, string]> = [
      ["UNAUTHORIZED", "Сервер отклонил HWID (401). Проверьте HWID в настройках."],
      ["FORBIDDEN", "Доступ к подписке запрещён (403)."],
      ["NOT_FOUND", "Подписка не найдена (404). Проверьте URL."],
      ["INVALID_JSON", "Сервер вернул некорректный JSON."],
      ["EMPTY", "Подписка пуста."],
      ["NO_PROFILES", "В подписке нет поддерживаемых профилей."],
    ];

    for (const [code, message] of cases) {
      resetAppStore();
      useAppStore.setState({ fetchError: { code, message: "server text" } });
      const { unmount } = render(<ProfilesScreen />);
      expect(screen.getByText(message)).toBeInTheDocument();
      unmount();
    }
  });

  it("повторяет обновление по кнопке «Повторить»", async () => {
    const user = userEvent.setup();
    apiMock.subscriptionFetch.mockResolvedValue({
      profiles: [],
      selected_profile_id: null,
      fetched_at: null,
      skipped: [],
    });
    useAppStore.setState({ fetchError: { code: "NETWORK", message: "нет сети" } });

    render(<ProfilesScreen />);
    await user.click(screen.getByRole("button", { name: "Повторить" }));

    await waitFor(() =>
      expect(apiMock.subscriptionFetch).toHaveBeenCalledTimes(1),
    );
  });

  it("проверяет задержки, сортирует и помечает лучшую", async () => {
    const user = userEvent.setup();
    const slow = makeProfile({ id: "slow", name: "Медленная" });
    const fast = makeProfile({ id: "fast", name: "Быстрая" });
    apiMock.profilesPing.mockResolvedValue({
      results: [
        { profile_id: "slow", latency_ms: 180 },
        { profile_id: "fast", latency_ms: 40 },
      ],
      best_profile_id: "fast",
      switch_to: null,
      checked_at: 1,
    });
    useAppStore.setState({
      profiles: [slow, fast],
      selectedProfileId: "slow",
    });

    render(<ProfilesScreen />);
    await user.click(
      screen.getByRole("button", { name: "Проверить задержки" }),
    );

    expect(await screen.findByText("180 мс")).toBeInTheDocument();
    expect(screen.getByText("40 мс")).toBeInTheDocument();
    expect(screen.getByText("Лучшая")).toBeInTheDocument();
    const items = screen.getAllByRole("listitem");
    expect(items[0]).toHaveTextContent("Быстрая");
    expect(items[1]).toHaveTextContent("Медленная");
  });

  it("показывает типизированную ошибку замера", async () => {
    const user = userEvent.setup();
    apiMock.profilesPing.mockResolvedValue({
      results: [
        { profile_id: "profile-1", latency_ms: null, error: "PING_TIMEOUT" },
      ],
      best_profile_id: null,
      switch_to: null,
      checked_at: 1,
    });
    useAppStore.setState({
      profiles: [makeProfile()],
      selectedProfileId: "profile-1",
    });

    render(<ProfilesScreen />);
    await user.click(
      screen.getByRole("button", { name: "Проверить задержки" }),
    );

    expect(
      await screen.findByText("Сервер не ответил за отведённое время."),
    ).toBeInTheDocument();
  });

  it("выбирает локацию из списка", async () => {
    const user = userEvent.setup();
    const first = makeProfile({ id: "p1", name: "Локация A" });
    const second = makeProfile({ id: "p2", name: "Локация B" });
    apiMock.profileSelect.mockResolvedValue({
      profiles: [first, second],
      selected_profile_id: "p2",
      fetched_at: 1,
    });
    useAppStore.setState({
      profiles: [first, second],
      selectedProfileId: "p1",
      fetchedAt: 1,
    });

    render(<ProfilesScreen />);
    await user.click(screen.getByText("Локация B"));

    await waitFor(() => expect(apiMock.profileSelect).toHaveBeenCalledWith("p2"));
    await waitFor(() =>
      expect(useAppStore.getState().selectedProfileId).toBe("p2"),
    );
  });
});
