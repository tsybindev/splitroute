import { beforeEach, describe, expect, it, vi } from "vitest";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { UpdateCard } from "./UpdateCard";
import { resetAppStore } from "../../shared/state/appStore";

const progress = vi.hoisted(() => ({
  handler: undefined as ((value: { downloaded: number; total?: number | null }) => void) | undefined,
}));

const apiMock = vi.hoisted(() => ({
  updateStatus: vi.fn(),
  updateCheck: vi.fn(),
  updateInstall: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(),
  onUpdateProgress: vi.fn((handler) => {
    progress.handler = handler;
    return Promise.resolve(() => {});
  }),
  toCommandError: (error: unknown) => error,
}));

function renderCard(channel = "stable" as const) {
  const onChannelChange = vi.fn();
  const onCheckOnStartChange = vi.fn();
  render(
    <UpdateCard
      channel={channel}
      checkOnStart={false}
      onChannelChange={onChannelChange}
      onCheckOnStartChange={onCheckOnStartChange}
    />,
  );
  return { onChannelChange, onCheckOnStartChange };
}

async function ready() {
  await waitFor(() => expect(apiMock.updateStatus).toHaveBeenCalled());
  await waitFor(() => expect(screen.getByText("0.1.0")).toBeInTheDocument());
}

describe("UpdateCard", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
    progress.handler = undefined;
    apiMock.updateStatus.mockResolvedValue({
      current_version: "0.1.0",
      channel: "stable",
      configured: true,
    });
  });

  it("показывает версию и блокирует проверку без настроенного канала", async () => {
    apiMock.updateStatus.mockResolvedValue({
      current_version: "0.1.0",
      channel: "stable",
      configured: false,
    });
    renderCard();
    await ready();

    expect(
      screen.getByText("Канал обновлений не настроен в этой сборке."),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "Проверить обновления" }),
    ).toBeDisabled();
  });

  it("проверяет обновления и сообщает, что всё актуально", async () => {
    apiMock.updateCheck.mockResolvedValue({
      available: false,
      current_version: "0.1.0",
    });
    const user = userEvent.setup();
    renderCard();
    await ready();

    await user.click(
      screen.getByRole("button", { name: "Проверить обновления" }),
    );

    await waitFor(() =>
      expect(screen.getByText("Обновлений нет.")).toBeInTheDocument(),
    );
    expect(apiMock.updateCheck).toHaveBeenCalledWith("stable");
  });

  it("показывает доступную версию и устанавливает её с перезапуском", async () => {
    apiMock.updateCheck.mockResolvedValue({
      available: true,
      current_version: "0.1.0",
      version: "0.2.0",
      notes: "Исправления и улучшения",
      date: "2026-11-14T00:00:00Z",
    });
    apiMock.updateInstall.mockResolvedValue(undefined);
    const user = userEvent.setup();
    renderCard();
    await ready();

    await user.click(
      screen.getByRole("button", { name: "Проверить обновления" }),
    );
    await waitFor(() =>
      expect(screen.getByText("Доступна версия 0.2.0.")).toBeInTheDocument(),
    );
    expect(screen.getByText("Исправления и улучшения")).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "Установить и перезапустить" }),
    );
    await waitFor(() =>
      expect(apiMock.updateInstall).toHaveBeenCalledWith("stable"),
    );
  });

  it("показывает прогресс загрузки", async () => {
    apiMock.updateCheck.mockResolvedValue({
      available: true,
      current_version: "0.1.0",
      version: "0.2.0",
    });
    apiMock.updateInstall.mockImplementation(() => new Promise(() => {}));
    const user = userEvent.setup();
    renderCard();
    await ready();

    await user.click(
      screen.getByRole("button", { name: "Проверить обновления" }),
    );
    await user.click(
      await screen.findByRole("button", { name: "Установить и перезапустить" }),
    );
    await waitFor(() => expect(progress.handler).toBeDefined());

    act(() => progress.handler?.({ downloaded: 50, total: 100 }));

    await waitFor(() =>
      expect(screen.getByText("Загружено: 50%")).toBeInTheDocument(),
    );
    expect(screen.getByRole("progressbar")).toHaveAttribute(
      "aria-valuenow",
      "50",
    );
  });

  it("показывает ошибку подписи и не устанавливает обновление", async () => {
    apiMock.updateCheck.mockRejectedValue({
      code: "UPDATE_SIGNATURE",
      message: "подпись недействительна",
    });
    const user = userEvent.setup();
    renderCard();
    await ready();

    await user.click(
      screen.getByRole("button", { name: "Проверить обновления" }),
    );

    await waitFor(() =>
      expect(
        screen.getByText("Подпись обновления недействительна. Установка отменена."),
      ).toBeInTheDocument(),
    );
    expect(screen.queryByText(/Установить и перезапустить/)).not.toBeInTheDocument();
  });

  it("передаёт выбранный канал и настройку проверки при запуске", async () => {
    const user = userEvent.setup();
    const { onChannelChange, onCheckOnStartChange } = renderCard();
    await ready();

    await user.selectOptions(screen.getByLabelText("Канал обновлений"), "beta");
    expect(onChannelChange).toHaveBeenCalledWith("beta");

    await user.click(
      screen.getByLabelText("Проверять обновления при запуске"),
    );
    expect(onCheckOnStartChange).toHaveBeenCalledWith(true);
  });

  it("позволяет отложить установку", async () => {
    apiMock.updateCheck.mockResolvedValue({
      available: true,
      current_version: "0.1.0",
      version: "0.2.0",
    });
    const user = userEvent.setup();
    renderCard();
    await ready();

    await user.click(
      screen.getByRole("button", { name: "Проверить обновления" }),
    );
    await user.click(await screen.findByRole("button", { name: "Позже" }));

    await waitFor(() =>
      expect(screen.queryByText("Доступна версия 0.2.0.")).not.toBeInTheDocument(),
    );
  });
});
