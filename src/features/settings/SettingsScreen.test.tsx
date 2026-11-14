import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { SettingsScreen } from "./SettingsScreen";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";
import { makeSettings, stoppedStatus } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  settingsUpdate: vi.fn(),
  hwidGenerate: vi.fn(),
  helperInstall: vi.fn(),
  helperUninstall: vi.fn(),
  helperStatus: vi.fn(),
  updateStatus: vi.fn(),
  updateCheck: vi.fn(),
  updateInstall: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  onUpdateProgress: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

describe("SettingsScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
    apiMock.helperStatus.mockResolvedValue(stoppedStatus);
    apiMock.updateStatus.mockResolvedValue({
      current_version: "0.1.0",
      channel: "stable",
      configured: true,
    });
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

  it("переключает язык, применяет сразу и сохраняет через settingsUpdate", async () => {
    const user = userEvent.setup();
    useAppStore.setState({ settings: makeSettings() });
    apiMock.settingsUpdate.mockImplementation(async (settings) => settings);

    render(<SettingsScreen />);
    expect(
      screen.getByRole("button", { name: "Сохранить настройки" }),
    ).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText(/Язык интерфейса/), "en");

    await waitFor(() =>
      expect(apiMock.settingsUpdate).toHaveBeenCalledWith(
        expect.objectContaining({
          preferences: expect.objectContaining({ language: "en" }),
        }),
      ),
    );
    // Язык применился без перезапуска: видимый текст уже английский.
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Save settings" }),
      ).toBeInTheDocument(),
    );
    expect(screen.getByLabelText(/Interface language/)).toHaveValue("en");
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

  it("предлагает только два DNS-режима с подсказками", async () => {
    const user = userEvent.setup();
    useAppStore.setState({ settings: makeSettings() });
    render(<SettingsScreen />);

    const select = screen.getByLabelText(/DNS-режим/);
    const options = Array.from(select.querySelectorAll("option")).map(
      (option) => option.getAttribute("value"),
    );
    expect(options).toEqual(["proxy", "system"]);
    expect(screen.getByText(/Весь DNS идёт через прокси, утечек нет/)).toBeInTheDocument();

    await user.selectOptions(select, "system");
    expect(
      screen.getByText(/DNS выбранных приложений уходит локально/),
    ).toBeInTheDocument();
  });

  it("включает kill switch с предупреждением и сохраняет настройку", async () => {
    const user = userEvent.setup();
    const settings = makeSettings();
    const saved = {
      ...settings,
      network: { ...settings.network, kill_switch: true },
    };
    apiMock.settingsUpdate.mockResolvedValue(saved);
    useAppStore.setState({ settings });

    render(<SettingsScreen />);
    expect(
      screen.getByText(/прямой выход в интернет будет заблокирован/),
    ).toBeInTheDocument();

    await user.click(
      screen.getByLabelText("Блокировать прямой трафик при обрыве туннеля"),
    );
    await user.click(screen.getByRole("button", { name: "Сохранить настройки" }));

    await waitFor(() =>
      expect(apiMock.settingsUpdate).toHaveBeenCalledWith(
        expect.objectContaining({
          network: expect.objectContaining({ kill_switch: true }),
        }),
      ),
    );
  });

  it("включает автовыбор и автопереключение и сохраняет параметры", async () => {
    const user = userEvent.setup();
    const settings = makeSettings();
    apiMock.settingsUpdate.mockImplementation(async (next) => next);
    useAppStore.setState({ settings });

    render(<SettingsScreen />);
    expect(screen.getByText(/TCP-connect/)).toBeInTheDocument();

    await user.click(
      screen.getByLabelText("Выбирать самую быструю локацию"),
    );
    await user.click(screen.getByLabelText("Переключать при деградации"));
    await user.click(screen.getByRole("button", { name: "Сохранить настройки" }));

    await waitFor(() =>
      expect(apiMock.settingsUpdate).toHaveBeenCalledWith(
        expect.objectContaining({
          preferences: expect.objectContaining({
            auto_select: expect.objectContaining({
              enabled: true,
              auto_switch: true,
              degradation_ms: 100,
              interval_secs: 60,
            }),
          }),
        }),
      ),
    );
  });

  it("показывает статус helper'а из helper_status", async () => {
    useAppStore.setState({
      settings: makeSettings(),
      helperStatus: { state: "stopped", version: "sing-box 1.14.4" },
    });
    render(<SettingsScreen />);

    expect(screen.getByText("sing-box 1.14.4")).toBeInTheDocument();
  });

  it("сохраняет канал обновлений и проверку при запуске", async () => {
    const user = userEvent.setup();
    apiMock.settingsUpdate.mockImplementation(async (next) => next);
    useAppStore.setState({ settings: makeSettings() });

    render(<SettingsScreen />);
    await waitFor(() => expect(screen.getByText("0.1.0")).toBeInTheDocument());

    await user.selectOptions(screen.getByLabelText("Канал обновлений"), "beta");
    await user.click(
      screen.getByLabelText("Проверять обновления при запуске"),
    );
    await user.click(screen.getByRole("button", { name: "Сохранить настройки" }));

    await waitFor(() =>
      expect(apiMock.settingsUpdate).toHaveBeenCalledWith(
        expect.objectContaining({
          preferences: expect.objectContaining({
            updates: { channel: "beta", check_on_start: true },
          }),
        }),
      ),
    );
  });

  it("показывает точную причину: нужны права администратора", async () => {
    const user = userEvent.setup();
    apiMock.helperInstall.mockRejectedValue({
      code: "INSTALLER_PRIVILEGES",
      message: "для установки helper'а нужны права администратора",
    });
    useAppStore.setState({ settings: makeSettings() });

    render(<SettingsScreen />);
    await user.click(screen.getByRole("button", { name: "Установить" }));

    await waitFor(() =>
      expect(screen.getByText("Нужны права администратора.")).toBeInTheDocument(),
    );
    expect(
      screen.getByText("для установки helper'а нужны права администратора"),
    ).toBeInTheDocument();
  });

  it("показывает отмену системного запроса прав", async () => {
    const user = userEvent.setup();
    apiMock.helperInstall.mockRejectedValue({
      code: "INSTALLER_FAILED",
      message: "операция отменена пользователем",
    });
    useAppStore.setState({ settings: makeSettings() });

    render(<SettingsScreen />);
    await user.click(screen.getByRole("button", { name: "Установить" }));

    await waitFor(() =>
      expect(
        screen.getByText("операция отменена пользователем"),
      ).toBeInTheDocument(),
    );
  });

  it("показывает отсутствующий бинарь и подпись helper'а", async () => {
    const user = userEvent.setup();
    apiMock.helperInstall.mockRejectedValue({
      code: "INSTALLER_MISSING_BINARY",
      message: "не найден файл: splitroute-helper",
    });
    useAppStore.setState({ settings: makeSettings() });

    render(<SettingsScreen />);
    await user.click(screen.getByRole("button", { name: "Установить" }));

    await waitFor(() =>
      expect(screen.getByText("не найден файл: splitroute-helper")).toBeInTheDocument(),
    );
  });

  it("показывает неподдерживаемую платформу", async () => {
    const user = userEvent.setup();
    apiMock.helperInstall.mockRejectedValue({
      code: "INSTALLER_UNSUPPORTED",
      message: "установка helper'а не поддерживается на этой платформе",
    });
    useAppStore.setState({ settings: makeSettings() });

    render(<SettingsScreen />);
    await user.click(screen.getByRole("button", { name: "Установить" }));

    await waitFor(() =>
      expect(
        screen.getByText("Установка helper'а не поддерживается на этой платформе."),
      ).toBeInTheDocument(),
    );
  });

  it("показывает успешную установку и обновляет статус из ядра", async () => {
    const user = userEvent.setup();
    apiMock.helperInstall.mockResolvedValue(undefined);
    apiMock.helperStatus.mockResolvedValue({
      state: "stopped",
      version: "sing-box 1.14.4",
    });
    useAppStore.setState({ settings: makeSettings(), helperStatus: null });

    render(<SettingsScreen />);
    await user.click(screen.getByRole("button", { name: "Установить" }));

    await waitFor(() =>
      expect(screen.getByText("Helper установлен.")).toBeInTheDocument(),
    );
    expect(apiMock.helperStatus).toHaveBeenCalled();
    // После установки кнопка меняется на «Переустановить» — статус пришёл из ядра.
    await waitFor(() =>
      expect(
        screen.getByRole("button", { name: "Переустановить" }),
      ).toBeInTheDocument(),
    );
  });
});
