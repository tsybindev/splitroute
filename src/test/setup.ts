import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach } from "vitest";
import { setLanguage } from "../shared/i18n";

afterEach(() => {
  cleanup();
  // Язык живёт в модуле: без сброса EN из одного теста протекал бы в другие.
  setLanguage("ru");
});
