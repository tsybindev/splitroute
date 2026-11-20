import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SubscriptionsCard } from "./SubscriptionsCard";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";
import { makeProfile, makeSubscription } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  subscriptionAdd: vi.fn(),
  subscriptionUpdate: vi.fn(),
  subscriptionDelete: vi.fn(),
  subscriptionFetch: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

// Tauri API в jsdom недоступен: диалог импорта подписывается на drag-drop окна.
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    onDragDropEvent: () => Promise.resolve(() => {}),
  }),
}));

describe("SubscriptionsCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
  });

  it("показывает пустое состояние и открывает форму добавления", async () => {
    const user = userEvent.setup();
    render(<SubscriptionsCard />);

    expect(screen.getByText(/Подписок нет/)).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: "Новая подписка" }));
    expect(screen.getByLabelText("Название")).toBeInTheDocument();
    expect(screen.getByLabelText(/URL подписки/)).toBeInTheDocument();
  });

  it("кнопка «Импорт по QR» открывает диалог импорта", async () => {
    const user = userEvent.setup();
    render(<SubscriptionsCard />);

    await user.click(screen.getByRole("button", { name: "Импорт по QR" }));

    expect(screen.getByText("Импорт подписки по QR")).toBeInTheDocument();
    expect(screen.getByText(/Перетащите картинку с QR/)).toBeInTheDocument();
  });

  it("добавляет подписку и сразу загружает её локации", async () => {
    const user = userEvent.setup();
    const entry = makeSubscription({
      id: "new-1",
      name: "Вторая",
      url: "https://example.invalid/second",
    });
    apiMock.subscriptionAdd.mockResolvedValue(entry);
    apiMock.subscriptionFetch.mockResolvedValue({
      subscription_id: "new-1",
      name: "Вторая",
      profiles: [makeProfile()],
      skipped: [],
      selected_profile_id: "profile-1",
      fetched_at: 1,
      next_update_at: null,
    });

    render(<SubscriptionsCard />);
    await user.click(screen.getByRole("button", { name: "Новая подписка" }));
    await user.type(screen.getByLabelText(/URL подписки/), "https://example.invalid/second");
    await user.type(screen.getByLabelText("Название"), "Вторая");
    await user.click(screen.getByRole("button", { name: "Добавить" }));

    await waitFor(() =>
      expect(apiMock.subscriptionAdd).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "Вторая",
          url: "https://example.invalid/second",
        }),
      ),
    );
    // Локации появляются без отдельного клика по «Обновить».
    await waitFor(() =>
      expect(apiMock.subscriptionFetch).toHaveBeenCalledWith("new-1"),
    );
    await waitFor(() =>
      expect(useAppStore.getState().subscriptions.map((s) => s.id)).toEqual([
        "new-1",
      ]),
    );
  });

  it("сохраняет изменённые параметры подписки отдельной командой", async () => {
    const user = userEvent.setup();
    const entry = makeSubscription({ id: "s1", name: "Основная" });
    useAppStore.setState({ subscriptions: [entry] });
    apiMock.subscriptionUpdate.mockResolvedValue({
      ...entry,
      auto_update_hours: 6,
    });

    render(<SubscriptionsCard />);
    const interval = screen.getByLabelText(/Автообновление/);
    await user.clear(interval);
    await user.type(interval, "6");
    await user.click(screen.getByRole("button", { name: "Сохранить" }));

    await waitFor(() =>
      expect(apiMock.subscriptionUpdate).toHaveBeenCalledWith(
        "s1",
        expect.objectContaining({ auto_update_hours: 6 }),
      ),
    );
  });

  it("удаляет подписку и обновляет общий список", async () => {
    const user = userEvent.setup();
    const entry = makeSubscription({
      id: "s1",
      profiles: [makeProfile()],
    });
    useAppStore.setState({
      subscriptions: [entry],
      profiles: entry.profiles,
      selectedProfileId: "profile-1",
    });
    apiMock.subscriptionDelete.mockResolvedValue({
      subscriptions: [],
      profiles: [],
      selected_profile_id: null,
    });

    render(<SubscriptionsCard />);
    await user.click(screen.getByRole("button", { name: "Удалить" }));

    await waitFor(() =>
      expect(apiMock.subscriptionDelete).toHaveBeenCalledWith("s1"),
    );
    await waitFor(() =>
      expect(useAppStore.getState().subscriptions).toEqual([]),
    );
    expect(useAppStore.getState().selectedProfileId).toBeNull();
  });
});
