import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { ProfilesScreen } from "./ProfilesScreen";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";
import { makeProfile, makeSubscription } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  profileSelect: vi.fn(),
  profilesPing: vi.fn(),
  profilesSetEnabled: vi.fn(),
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

  it("показывает группу подписки с её локациями", () => {
    const entry = makeSubscription({
      profiles: [makeProfile()],
      fetched_at: 1_800_000_000,
    });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: entry.profiles,
    });

    render(<ProfilesScreen />);
    expect(screen.getByText("Основная")).toBeInTheDocument();
    expect(screen.getByText("Тестовая локация A")).toBeInTheDocument();
    expect(screen.getByText("Подписок: 1")).toBeInTheDocument();
  });

  it("называет безымянную подписку по порядку", () => {
    const first = makeSubscription({ id: "s1", name: "" });
    const second = makeSubscription({
      id: "s2",
      name: "  ",
      profiles: [makeProfile()],
    });
    useAppStore.setState({
      subscriptions: [first, second],
      profiles: second.profiles,
    });

    render(<ProfilesScreen />);
    expect(screen.getByText("Подписка 1")).toBeInTheDocument();
    expect(screen.getByText("Подписка 2")).toBeInTheDocument();
  });

  it("показывает ошибку загрузки у своей подписки и снимает её после успеха", async () => {
    const user = userEvent.setup();
    const entry = makeSubscription({
      id: "s1",
      name: "Основная",
      profiles: [makeProfile()],
    });
    apiMock.subscriptionFetch.mockResolvedValue({
      subscription_id: "s1",
      name: "Основная",
      profiles: entry.profiles,
      skipped: [],
      selected_profile_id: "profile-1",
      fetched_at: 1_800_000_000,
      next_update_at: null,
    });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: entry.profiles,
      fetchErrors: { s1: { code: "NETWORK", message: "нет сети" } },
    });

    render(<ProfilesScreen />);
    expect(
      screen.getByText("Нет сети или сервер недоступен."),
    ).toBeInTheDocument();

    await user.click(screen.getByRole("button", { name: "Повторить" }));

    await waitFor(() =>
      expect(apiMock.subscriptionFetch).toHaveBeenCalledWith("s1"),
    );
    await waitFor(() =>
      expect(
        screen.queryByText("Нет сети или сервер недоступен."),
      ).not.toBeInTheDocument(),
    );
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
      const entry = makeSubscription({ id: "s1", profiles: [makeProfile()] });
      useAppStore.setState({
        subscriptions: [entry],
        profiles: entry.profiles,
        fetchErrors: { s1: { code, message: "server text" } },
      });
      const { unmount } = render(<ProfilesScreen />);
      expect(screen.getByText(message)).toBeInTheDocument();
      unmount();
    }
  });

  it("проверяет задержки, сохраняет порядок подписки и помечает лучшую", async () => {
    const user = userEvent.setup();
    const slow = makeProfile({ id: "slow", name: "Медленная" });
    const fast = makeProfile({ id: "fast", name: "Быстрая" });
    const entry = makeSubscription({ id: "s1", profiles: [slow, fast] });
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
      subscriptions: [entry],
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
    expect(items[0]).toHaveTextContent("Медленная");
    expect(items[1]).toHaveTextContent("Быстрая");
  });

  it("показывает время следующего автообновления у подписки", () => {
    const fetchedAt = Math.floor(new Date("2026-11-14T10:00:00Z").getTime() / 1000);
    const nextUpdateAt = fetchedAt + 12 * 3600;
    const entry = makeSubscription({
      id: "s1",
      profiles: [makeProfile()],
      fetched_at: fetchedAt,
      next_update_at: nextUpdateAt,
      auto_update_hours: 12,
    });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: entry.profiles,
    });

    render(<ProfilesScreen />);

    const expected = new Date(nextUpdateAt * 1000).toLocaleString("ru-RU");
    expect(
      screen.getByText(`Следующее обновление: ${expected}`),
    ).toBeInTheDocument();
  });

  it("сообщает, что автообновление выключено при интервале 0", () => {
    const entry = makeSubscription({
      id: "s1",
      profiles: [makeProfile()],
      fetched_at: 1_800_000_000,
      next_update_at: null,
      auto_update_hours: 0,
    });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: entry.profiles,
    });

    render(<ProfilesScreen />);

    expect(screen.getByText("Автообновление выключено")).toBeInTheDocument();
  });

  it("обновляет конкретную подписку по её кнопке", async () => {
    const user = userEvent.setup();
    const first = makeSubscription({ id: "s1", name: "A" });
    const second = makeSubscription({ id: "s2", name: "B" });
    useAppStore.setState({ subscriptions: [first, second] });

    render(<ProfilesScreen />);
    const buttons = screen.getAllByRole("button", { name: "Обновить" });
    await user.click(buttons[1]);

    await waitFor(() =>
      expect(apiMock.subscriptionFetch).toHaveBeenCalledWith("s2"),
    );
    expect(apiMock.subscriptionFetch).toHaveBeenCalledTimes(1);
  });

  it("выбирает локацию из списка", async () => {
    const user = userEvent.setup();
    const first = makeProfile({ id: "p1", name: "Локация A" });
    const second = makeProfile({ id: "p2", name: "Локация B" });
    const entry = makeSubscription({ id: "s1", profiles: [first, second] });
    apiMock.profileSelect.mockResolvedValue({
      subscriptions: [entry],
      profiles: [first, second],
      selected_profile_id: "p2",
    });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: [first, second],
      selectedProfileId: "p1",
    });

    render(<ProfilesScreen />);
    await user.click(screen.getByText("Локация B"));

    await waitFor(() => expect(apiMock.profileSelect).toHaveBeenCalledWith("p2"));
    await waitFor(() =>
      expect(useAppStore.getState().selectedProfileId).toBe("p2"),
    );
  });

  it("переключает локацию чекбоксом и применяет ответ", async () => {
    const user = userEvent.setup();
    const first = makeProfile({ id: "p1", name: "Локация A" });
    const second = makeProfile({ id: "p2", name: "Локация B" });
    const entry = makeSubscription({ id: "s1", profiles: [first, second] });
    const updatedEntry = makeSubscription({
      id: "s1",
      profiles: [first, second],
      disabled_profiles: ["p1"],
    });
    apiMock.profilesSetEnabled.mockResolvedValue({
      subscriptions: [updatedEntry],
      profiles: [first, second],
      selected_profile_id: "p2",
    });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: [first, second],
      selectedProfileId: "p2",
    });

    render(<ProfilesScreen />);
    const checkbox = screen.getByRole("checkbox", {
      name: "Включить или выключить локацию: Локация A",
    });
    expect(checkbox).toBeChecked();
    await user.click(checkbox);

    await waitFor(() =>
      expect(apiMock.profilesSetEnabled).toHaveBeenCalledWith(
        "s1",
        ["p1"],
        false,
      ),
    );
    await waitFor(() =>
      expect(useAppStore.getState().subscriptions[0].disabled_profiles).toEqual(
        ["p1"],
      ),
    );
    expect(screen.getByText("Выключена")).toBeInTheDocument();
  });

  it("выключенную локацию нельзя выбрать, включённую можно", async () => {
    const user = userEvent.setup();
    const first = makeProfile({ id: "p1", name: "Локация A" });
    const second = makeProfile({ id: "p2", name: "Локация B" });
    const entry = makeSubscription({
      id: "s1",
      profiles: [first, second],
      disabled_profiles: ["p1"],
    });
    apiMock.profileSelect.mockResolvedValue({
      subscriptions: [entry],
      profiles: [first, second],
      selected_profile_id: "p2",
    });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: [first, second],
      selectedProfileId: "p2",
    });

    render(<ProfilesScreen />);
    expect(screen.getByText("Выключена")).toBeInTheDocument();

    await user.click(screen.getByText("Локация A"));
    expect(apiMock.profileSelect).not.toHaveBeenCalled();

    await user.click(screen.getByText("Локация B"));
    await waitFor(() =>
      expect(apiMock.profileSelect).toHaveBeenCalledWith("p2"),
    );
  });

  it("массово выключает и включает локации подписки", async () => {
    const user = userEvent.setup();
    const first = makeProfile({ id: "p1", name: "Локация A" });
    const second = makeProfile({ id: "p2", name: "Локация B" });
    const entry = makeSubscription({ id: "s1", profiles: [first, second] });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: [first, second],
    });

    render(<ProfilesScreen />);
    // Всё включено: «Включить все» нечего делать.
    expect(
      screen.getByRole("button", { name: "Включить все" }),
    ).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Выключить все" }));
    await waitFor(() =>
      expect(apiMock.profilesSetEnabled).toHaveBeenCalledWith(
        "s1",
        ["p1", "p2"],
        false,
      ),
    );
  });

  it("массовое включение доступно, когда есть выключенные", async () => {
    const user = userEvent.setup();
    const first = makeProfile({ id: "p1", name: "Локация A" });
    const second = makeProfile({ id: "p2", name: "Локация B" });
    const entry = makeSubscription({
      id: "s1",
      profiles: [first, second],
      disabled_profiles: ["p1", "p2"],
    });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: [first, second],
    });

    render(<ProfilesScreen />);
    expect(
      screen.getByRole("button", { name: "Выключить все" }),
    ).toBeDisabled();

    await user.click(screen.getByRole("button", { name: "Включить все" }));
    await waitFor(() =>
      expect(apiMock.profilesSetEnabled).toHaveBeenCalledWith(
        "s1",
        ["p1", "p2"],
        true,
      ),
    );
  });

  it("показывает ошибку на экране, если команда вкл/выкл не удалась", async () => {
    const user = userEvent.setup();
    const first = makeProfile({ id: "p1", name: "Локация A" });
    const entry = makeSubscription({ id: "s1", profiles: [first] });
    apiMock.profilesSetEnabled.mockRejectedValue({
      code: "COMMAND_UNREACHABLE_TEST",
      message: "invoke отклонён",
    });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: [first],
      selectedProfileId: "p1",
    });

    render(<ProfilesScreen />);
    await user.click(
      screen.getByRole("checkbox", {
        name: "Включить или выключить локацию: Локация A",
      }),
    );

    const alert = await screen.findByRole("alert");
    expect(alert).toHaveTextContent("invoke отклонён");
    // Ошибка не должна молча проглатываться: чекбокс остаётся в прежнем
    // состоянии, но пользователь видит причину.
    expect(
      screen.getByRole("checkbox", {
        name: "Включить или выключить локацию: Локация A",
      }),
    ).toBeChecked();
  });
});
