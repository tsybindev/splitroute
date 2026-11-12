import { beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { AppsScreen } from "./AppsScreen";
import { resetAppStore, useAppStore } from "../../shared/state/appStore";
import { makeRule } from "../../test/fixtures";

const apiMock = vi.hoisted(() => ({
  rulesUpsert: vi.fn(),
  rulesDelete: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  onConnectionChanged: vi.fn(() => Promise.resolve(() => {})),
  toCommandError: (error: unknown) => error,
}));

describe("AppsScreen", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    resetAppStore();
  });

  it("показывает пустое состояние", () => {
    render(<AppsScreen />);
    expect(screen.getByText(/Правил пока нет/)).toBeInTheDocument();
    expect(screen.getByText("Правил: 0")).toBeInTheDocument();
  });

  it("выключает правило", async () => {
    const user = userEvent.setup();
    const rule = makeRule();
    apiMock.rulesUpsert.mockResolvedValue([{ ...rule, enabled: false }]);
    useAppStore.setState({ rules: [rule] });

    render(<AppsScreen />);
    await user.click(screen.getByRole("button", { name: /Через прокси/ }));

    await waitFor(() =>
      expect(apiMock.rulesUpsert).toHaveBeenCalledWith({
        ...rule,
        enabled: false,
      }),
    );
  });

  it("добавляет правило по имени процесса", async () => {
    const user = userEvent.setup();
    apiMock.rulesUpsert.mockResolvedValue([]);

    render(<AppsScreen />);
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

  it("не отправляет пустое правило", async () => {
    const user = userEvent.setup();
    render(<AppsScreen />);
    await user.click(screen.getByRole("button", { name: "Добавить правило" }));
    expect(apiMock.rulesUpsert).not.toHaveBeenCalled();
    expect(
      screen.getByText("Заполните название и значение правила."),
    ).toBeInTheDocument();
  });
});
