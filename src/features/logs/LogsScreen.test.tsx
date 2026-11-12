import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { LogsScreen } from "./LogsScreen";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";

const apiMock = vi.hoisted(() => ({
  logsList: vi.fn(),
  diagnosticsGet: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

describe("LogsScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
  });

  it("показывает пустое состояние", () => {
    render(<LogsScreen />);
    expect(screen.getByText("Логов пока нет.")).toBeInTheDocument();
  });

  it("показывает строки и фильтрует по уровню", async () => {
    const user = userEvent.setup();
    apiMock.logsList.mockResolvedValue([]);
    useAppStore.setState({
      logs: [
        { timestamp: 1_762_950_000, level: "info", message: "Подписка обновлена" },
        { timestamp: 1_762_950_001, level: "error", message: "Ошибка ядра" },
      ],
    });

    render(<LogsScreen />);
    expect(screen.getByText("Подписка обновлена")).toBeInTheDocument();
    expect(screen.getByText("Ошибка ядра")).toBeInTheDocument();

    await user.selectOptions(screen.getByLabelText("Уровень"), "error");
    await waitFor(() =>
      expect(apiMock.logsList).toHaveBeenCalledWith("error"),
    );
  });

  it("копирует диагностику без секретов", async () => {
    const user = userEvent.setup();
    apiMock.diagnosticsGet.mockResolvedValue("SplitRoute 0.1.0");
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText },
      configurable: true,
    });

    render(<LogsScreen />);
    await user.click(
      screen.getByRole("button", { name: "Копировать диагностику" }),
    );

    await waitFor(() =>
      expect(writeText).toHaveBeenCalledWith("SplitRoute 0.1.0"),
    );
  });
});
