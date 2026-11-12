import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SettingsScreen } from "./SettingsScreen";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";
import { makeSettings } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  settingsUpdate: vi.fn(),
  hwidGenerate: vi.fn(),
  helperInstall: vi.fn(),
  helperUninstall: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

describe("SettingsScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
  });

  it("показывает загрузку без настроек", () => {
    render(<SettingsScreen />);
    expect(screen.getByText("Загрузка…")).toBeInTheDocument();
  });

  it("сохраняет изменённый URL подписки", async () => {
    const user = userEvent.setup();
    const settings = makeSettings();
    const saved = makeSettings({
      subscription: {
        ...settings.subscription,
        url: "https://example.invalid/sub",
      },
    });
    apiMock.settingsUpdate.mockResolvedValue(saved);
    useAppStore.setState({ settings });

    render(<SettingsScreen />);
    const input = screen.getByLabelText(/URL подписки/);
    await user.clear(input);
    await user.type(input, "https://example.invalid/sub");
    await user.click(screen.getByRole("button", { name: "Сохранить настройки" }));

    await waitFor(() =>
      expect(apiMock.settingsUpdate).toHaveBeenCalledWith(
        expect.objectContaining({
          subscription: expect.objectContaining({
            url: "https://example.invalid/sub",
          }),
        }),
      ),
    );
    await waitFor(() =>
      expect(screen.getByText("Настройки сохранены")).toBeInTheDocument(),
    );
  });

  it("генерирует новый HWID", async () => {
    const user = userEvent.setup();
    apiMock.hwidGenerate.mockResolvedValue("abcdef0123456789");
    useAppStore.setState({ settings: makeSettings() });

    render(<SettingsScreen />);
    await user.click(screen.getByRole("button", { name: "Сгенерировать" }));

    await waitFor(() =>
      expect(screen.getByLabelText(/HWID/)).toHaveValue("abcdef0123456789"),
    );
  });

  it("предупреждает о неподтверждённом разделении DNS", () => {
    useAppStore.setState({ settings: makeSettings() });
    render(<SettingsScreen />);
    expect(
      screen.getByText(/разделение DNS по процессу не подтверждено/i),
    ).toBeInTheDocument();
  });

  it("показывает ошибку установки helper'а", async () => {
    const user = userEvent.setup();
    apiMock.helperInstall.mockRejectedValue({
      code: "INSTALLER",
      message: "нужны права",
    });
    useAppStore.setState({ settings: makeSettings() });

    render(<SettingsScreen />);
    await user.click(screen.getByRole("button", { name: "Установить" }));

    await waitFor(() =>
      expect(
        screen.getByText("Установка helper'а недоступна."),
      ).toBeInTheDocument(),
    );
  });
});
