import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QrImportDialog } from "./QrImportDialog";

const apiMock = vi.hoisted(() => ({
  qrDecodeImage: vi.fn(),
  qrDecodeFile: vi.fn(),
}));

vi.mock("../../shared/api/client", () => ({
  api: apiMock,
  toCommandError: (error: unknown) => error,
}));

// Tauri API в jsdom недоступен: диалог должен спокойно подписываться на drag-drop.
vi.mock("@tauri-apps/api/webviewWindow", () => ({
  getCurrentWebviewWindow: () => ({
    onDragDropEvent: () => Promise.resolve(() => {}),
  }),
}));

const QR_URL = "https://example.invalid/sub-from-qr";

function makePngFile(): File {
  // Подпись PNG (8 байт): достаточно для FileReader и accept="image/png".
  const bytes = new Uint8Array([137, 80, 78, 71, 13, 10, 26, 10]);
  return new File([bytes], "qr.png", { type: "image/png" });
}

function fileInput(): HTMLInputElement {
  const input = document.querySelector('input[type="file"]');
  if (!(input instanceof HTMLInputElement)) {
    throw new Error("диалог не содержит input[type=file]");
  }
  return input;
}

describe("QrImportDialog", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("выбранный файл декодируется в base64 без префикса и отдаёт URL наружу", async () => {
    const user = userEvent.setup();
    const onImported = vi.fn();
    apiMock.qrDecodeImage.mockResolvedValue({ kind: "url", url: QR_URL });
    render(<QrImportDialog open onClose={() => {}} onImported={onImported} />);

    await user.upload(fileInput(), makePngFile());

    await waitFor(() =>
      expect(apiMock.qrDecodeImage).toHaveBeenCalledTimes(1),
    );
    // FileReader.readAsDataURL даёт "data:...;base64,<payload>": Rust ждёт payload.
    expect(apiMock.qrDecodeImage.mock.calls[0][0]).toBe("iVBORw0KGgo=");
    await waitFor(() =>
      expect(onImported).toHaveBeenCalledWith(QR_URL),
    );
  });

  it("QR со ссылками на узлы показывает ошибку и не импортирует", async () => {
    const user = userEvent.setup();
    const onImported = vi.fn();
    apiMock.qrDecodeImage.mockResolvedValue({ kind: "node_links", url: null });
    render(<QrImportDialog open onClose={() => {}} onImported={onImported} />);

    await user.upload(fileInput(), makePngFile());

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        /Импорт узлов без подписки/,
      ),
    );
    expect(onImported).not.toHaveBeenCalled();
  });

  it("сбой декодирования показывает текст по коду QR_DECODE_FAILED", async () => {
    const user = userEvent.setup();
    const onImported = vi.fn();
    apiMock.qrDecodeImage.mockRejectedValue({
      code: "QR_DECODE_FAILED",
      message: "decode boom",
    });
    render(<QrImportDialog open onClose={() => {}} onImported={onImported} />);

    await user.upload(fileInput(), makePngFile());

    await waitFor(() =>
      expect(screen.getByRole("alert")).toHaveTextContent(
        "Не удалось прочитать QR-код",
      ),
    );
    expect(onImported).not.toHaveBeenCalled();
  });

  it("вставка скриншота из буфера обмена запускает декодирование", async () => {
    const onImported = vi.fn();
    apiMock.qrDecodeImage.mockResolvedValue({ kind: "happ", url: QR_URL });
    render(<QrImportDialog open onClose={() => {}} onImported={onImported} />);

    const file = makePngFile();
    // В jsdom у Event нет clipboardData: подставляем минимальный объект.
    const pasteEvent = new Event("paste", { bubbles: true, cancelable: true });
    Object.defineProperty(pasteEvent, "clipboardData", {
      value: {
        items: [{ kind: "file", type: "image/png", getAsFile: () => file }],
      },
    });
    document.dispatchEvent(pasteEvent);

    await waitFor(() =>
      expect(apiMock.qrDecodeImage).toHaveBeenCalledTimes(1),
    );
    expect(apiMock.qrDecodeImage.mock.calls[0][0]).toBe("iVBORw0KGgo=");
    await waitFor(() =>
      expect(onImported).toHaveBeenCalledWith(QR_URL),
    );
  });

  it("кнопка отмены закрывает диалог", async () => {
    const user = userEvent.setup();
    const onClose = vi.fn();
    render(<QrImportDialog open onClose={onClose} onImported={() => {}} />);

    await user.click(screen.getByRole("button", { name: "Отмена" }));

    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("отмена во время чтения файла отменяет и запоздавший результат", async () => {
    // Управляемый FileReader: чтение завершается только по сигналу теста.
    class FakeFileReader {
      static instances: FakeFileReader[] = [];
      onload: (() => void) | null = null;
      onerror: (() => void) | null = null;
      result: string | null = null;
      constructor() {
        FakeFileReader.instances.push(this);
      }
      readAsDataURL(): void {
        // Ничего не делаем: момент завершения тест задаёт сам.
      }
    }
    vi.stubGlobal("FileReader", FakeFileReader);
    const onImported = vi.fn();
    const onClose = vi.fn();
    const { rerender } = render(
      <QrImportDialog open onClose={onClose} onImported={onImported} />,
    );
    const user = userEvent.setup();
    await user.upload(fileInput(), makePngFile());
    expect(FakeFileReader.instances).toHaveLength(1);

    // Пользователь отменил диалог, пока файл читался: родитель закрыл его.
    await user.click(screen.getByRole("button", { name: "Отмена" }));
    rerender(<QrImportDialog open={false} onClose={onClose} onImported={onImported} />);

    // Чтение завершилось уже после отмены: декодирование не запускается.
    const reader = FakeFileReader.instances[0];
    reader.result = "data:image/png;base64,iVBORw0KGgo=";
    reader.onload?.();

    expect(apiMock.qrDecodeImage).not.toHaveBeenCalled();
    expect(onImported).not.toHaveBeenCalled();
  });
});
