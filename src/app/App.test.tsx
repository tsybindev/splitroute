import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { App } from "./App";

describe("App", () => {
  it("показывает заголовок и вкладки", () => {
    render(<App />);
    expect(screen.getByRole("heading", { name: "SplitRoute" })).toBeInTheDocument();
    for (const label of ["Главная", "Локации", "Приложения", "Настройки", "Логи"]) {
      expect(screen.getByRole("button", { name: label })).toBeInTheDocument();
    }
  });

  it("переключает экраны", async () => {
    render(<App />);
    await userEvent.click(screen.getByRole("button", { name: "Приложения" }));
    expect(screen.getByRole("heading", { name: "Приложения" })).toBeInTheDocument();
  });
});
