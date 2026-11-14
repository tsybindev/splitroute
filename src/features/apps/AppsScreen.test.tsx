import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppsScreen } from "./AppsScreen";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";
import { makeInstalledApp, makeRule } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  rulesUpsert: vi.fn(),
  rulesDelete: vi.fn(),
  appsInstalled: vi.fn(),
  appRuleFromInstalled: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

async function openRulesTab() {
  const user = userEvent.setup();
  render(<AppsScreen />);
  await user.click(screen.getByRole("tab", { name: "Все правила" }));
  return user;
}

describe("AppsScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
    apiMock.appsInstalled.mockResolvedValue([]);
  });

  it("открывается на вкладке «Установленные»", () => {
    render(<AppsScreen />);
    expect(screen.getByRole("tab", { name: "Установленные" })).toHaveAttribute(
      "aria-selected",
      "true",
    );
    expect(screen.getByText("Найдено приложений: 0")).toBeInTheDocument();
  });

  it("показывает пустое состояние каталога", async () => {
    render(<AppsScreen />);
    await waitFor(() =>
      expect(
        screen.getByText("Приложения не найдены"),
      ).toBeInTheDocument(),
    );
  });

  it("показывает ошибку команды каталога с повтором", async () => {
    apiMock.appsInstalled.mockRejectedValue({
      code: "UNKNOWN",
      message: "Не удалось прочитать каталог",
    });
    render(<AppsScreen />);
    await waitFor(() =>
      expect(screen.getByRole("alert")).toBeInTheDocument(),
    );
    expect(
      screen.getByText("Не удалось прочитать каталог"),
    ).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Повторить" })).toBeInTheDocument();
  });

  it("фильтрует установленные приложения по имени", async () => {
    const user = userEvent.setup();
    apiMock.appsInstalled.mockResolvedValue([
      makeInstalledApp({ name: "Google Chrome", path: "/Applications/Google Chrome.app" }),
      makeInstalledApp({ name: "Telegram", path: "/Applications/Telegram.app" }),
    ]);
    render(<AppsScreen />);

    await screen.findByText("Google Chrome");
    expect(screen.getByText("Telegram")).toBeInTheDocument();

    await user.type(
      screen.getByLabelText("Поиск по установленным приложениям"),
      "tele",
    );
    expect(screen.queryByText("Google Chrome")).not.toBeInTheDocument();
    expect(screen.getByText("Telegram")).toBeInTheDocument();
    expect(screen.getByText("Найдено приложений: 1")).toBeInTheDocument();
  });

  it("показывает пустое состояние поиска без совпадений", async () => {
    const user = userEvent.setup();
    apiMock.appsInstalled.mockResolvedValue([
      makeInstalledApp({ name: "Telegram", path: "/Applications/Telegram.app" }),
    ]);
    render(<AppsScreen />);

    await screen.findByText("Telegram");
    await user.type(
      screen.getByLabelText("Поиск по установленным приложениям"),
      "zzz",
    );
    expect(
      screen.getByText("По запросу «zzz» ничего не найдено."),
    ).toBeInTheDocument();
  });

  it("создаёт правило из записи каталога через ядро", async () => {
    const user = userEvent.setup();
    const app = makeInstalledApp();
    const rule = makeRule({ id: "rule-new", enabled: false });
    apiMock.appsInstalled.mockResolvedValue([app]);
    apiMock.appRuleFromInstalled.mockResolvedValue(rule);
    apiMock.rulesUpsert.mockResolvedValue([rule]);

    render(<AppsScreen />);
    await screen.findByText(app.name);
    await user.click(
      screen.getByRole("button", { name: `Добавить правило: ${app.name}` }),
    );

    await waitFor(() =>
      expect(apiMock.appRuleFromInstalled).toHaveBeenCalledWith(app),
    );
    expect(apiMock.rulesUpsert).toHaveBeenCalledWith(rule);
    expect(
      screen.getByText(
        "Правило создано выключенным — включите его на вкладке «Все правила».",
      ),
    ).toBeInTheDocument();
    expect(useAppStore.getState().rules).toEqual([rule]);
  });

  it("показывает пустое состояние правил", async () => {
    await openRulesTab();
    expect(screen.getByText(/Правил пока нет/)).toBeInTheDocument();
    expect(screen.getByText("Правил: 0")).toBeInTheDocument();
  });

  it("выключает правило", async () => {
    const user = userEvent.setup();
    const rule = makeRule();
    apiMock.rulesUpsert.mockResolvedValue([{ ...rule, enabled: false }]);
    useAppStore.setState({ rules: [rule] });

    render(<AppsScreen />);
    await user.click(screen.getByRole("tab", { name: "Все правила" }));
    await user.click(screen.getByRole("button", { name: /Через прокси/ }));

    await waitFor(() =>
      expect(apiMock.rulesUpsert).toHaveBeenCalledWith({
        ...rule,
        enabled: false,
      }),
    );
  });

  it("добавляет правило по имени процесса", async () => {
    const user = await openRulesTab();
    apiMock.rulesUpsert.mockResolvedValue([]);

    await user.type(screen.getByLabelText("Название"), "curl");
    await user.type(screen.getByLabelText("Значение"), "curl");
    await user.click(screen.getByRole("button", { name: "Добавить правило" }));

    await waitFor(() =>
      expect(apiMock.rulesUpsert).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "curl",
          process_name: ["curl"],
          enabled: true,
        }),
      ),
    );
  });

  it("добавляет правило по regex пути", async () => {
    const user = await openRulesTab();
    apiMock.rulesUpsert.mockResolvedValue([]);

    await user.type(screen.getByLabelText("Название"), "Chrome");
    await user.selectOptions(screen.getByLabelText("Тип правила"), "regex");
    await user.type(
      screen.getByLabelText("Значение"),
      "^/Applications/Google Chrome\\.app/",
    );
    await user.click(screen.getByRole("button", { name: "Добавить правило" }));

    await waitFor(() =>
      expect(apiMock.rulesUpsert).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "Chrome",
          process_name: [],
          process_path: [],
          path_regex: ["^/Applications/Google Chrome\\.app/"],
          enabled: true,
        }),
      ),
    );
  });

  it("не отправляет пустое правило", async () => {
    const user = await openRulesTab();
    await user.click(screen.getByRole("button", { name: "Добавить правило" }));
    expect(apiMock.rulesUpsert).not.toHaveBeenCalled();
    expect(
      screen.getByText("Заполните название и значение правила."),
    ).toBeInTheDocument();
  });
});
