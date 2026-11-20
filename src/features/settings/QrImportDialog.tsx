import { useCallback, useEffect, useRef, useState } from "react";
import { Loader2, QrCode } from "lucide-react";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { UnlistenFn } from "@tauri-apps/api/event";
import { api, toCommandError } from "../../shared/api/client";
import type { CommandError, QrDecodeResult } from "../../shared/api/types";
import { t } from "../../shared/i18n";
import { Button } from "../../shared/ui/Button";
import { ErrorBox } from "../../shared/ui/ErrorBox";

interface QrImportDialogProps {
  open: boolean;
  onClose: () => void;
  onImported: (url: string) => void;
}

/** Диалог импорта подписки по QR: файл, вставка скриншота или drag-drop.
 *
 * Декодирование выполняет Rust (qr_decode_image / qr_decode_file): в UI попадает
 * только итоговая ссылка, расшифрованное содержимое не логируется.
 */
export function QrImportDialog({
  open,
  onClose,
  onImported,
}: QrImportDialogProps) {
  const [busy, setBusy] = useState(false);
  const [dragging, setDragging] = useState(false);
  const [error, setError] = useState<CommandError | null>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  // Пока декодирование не завершилось, новые файлы не принимаются: результаты
  // не должны перебивать друг друга.
  const busyRef = useRef(false);
  // Метка активного декодирования: отмена диалога обнуляет её, и запоздавший
  // результат не открывает форму добавления после «Отмена».
  const decodeTokenRef = useRef(0);

  const resetState = useCallback(() => {
    busyRef.current = false;
    decodeTokenRef.current += 1;
    setBusy(false);
    setDragging(false);
    setError(null);
  }, []);

  const handleClose = useCallback(() => {
    resetState();
    onClose();
  }, [resetState, onClose]);

  const decode = useCallback(
    (run: () => Promise<QrDecodeResult>) => {
      if (busyRef.current) return;
      busyRef.current = true;
      const token = decodeTokenRef.current;
      setBusy(true);
      setError(null);
      void (async () => {
        try {
          const result = await run();
          if (token !== decodeTokenRef.current) return;
          if (
            (result.kind === "url" || result.kind === "happ") &&
            result.url
          ) {
            onImported(result.url);
          } else if (result.kind === "node_links") {
            setError({ code: "QR_NODE_LINKS", message: "" });
          } else {
            setError({ code: "QR_NO_SUBSCRIPTION", message: "" });
          }
        } catch (cause) {
          if (token !== decodeTokenRef.current) return;
          setError(toCommandError(cause));
        } finally {
          // Сбрасываем занятость только если это ещё актуальный запуск.
          if (token === decodeTokenRef.current) {
            busyRef.current = false;
            setBusy(false);
          }
        }
      })();
    },
    [onImported],
  );

  const decodeImageFile = useCallback(
    (file: File) => {
      const reader = new FileReader();
      const token = decodeTokenRef.current;
      reader.onload = () => {
        // Пока файл читался, диалог могли закрыть: запоздавший результат не
        // должен открывать форму добавления.
        if (token !== decodeTokenRef.current) return;
        const dataUrl = String(reader.result ?? "");
        // readAsDataURL даёт "data:image/png;base64,<payload>": Rust ждёт payload.
        const separator = dataUrl.indexOf(",");
        decode(() =>
          api.qrDecodeImage(separator >= 0 ? dataUrl.slice(separator + 1) : dataUrl),
        );
      };
      // До декодирования дело не дошло: файл не прочитался как поток данных.
      // Если диалог уже закрыли, запоздалую ошибку не показываем.
      reader.onerror = () => {
        if (token === decodeTokenRef.current) {
          setError({ code: "QR_FILE_UNREADABLE", message: "" });
        }
      };
      reader.readAsDataURL(file);
    },
    [decode],
  );

  const decodePath = useCallback(
    (path: string) => {
      decode(() => api.qrDecodeFile(path));
    },
    [decode],
  );

  // Вставка скриншота: слушаем document, пока диалог открыт.
  useEffect(() => {
    if (!open) return;
    const onPaste = (event: ClipboardEvent) => {
      const items = event.clipboardData?.items;
      if (!items) return;
      for (let index = 0; index < items.length; index += 1) {
        const item = items[index];
        if (
          item.kind === "file" &&
          (item.type === "image/png" || item.type === "image/jpeg")
        ) {
          const file = item.getAsFile();
          if (file) {
            event.preventDefault();
            decodeImageFile(file);
          }
          break;
        }
      }
    };
    document.addEventListener("paste", onPaste);
    return () => document.removeEventListener("paste", onPaste);
  }, [open, decodeImageFile]);

  // Оконный drag-drop Tauri: HTML5-события drop при включённом dragDropEnabled
  // не доходят до страницы, поэтому подписка именно на onDragDropEvent.
  useEffect(() => {
    if (!open) return;
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    void getCurrentWebviewWindow()
      .onDragDropEvent((event) => {
        const payload = event.payload;
        if (payload.type === "leave") {
          setDragging(false);
        } else if (payload.type === "drop") {
          setDragging(false);
          const path = payload.paths[0];
          if (path) decodePath(path);
        } else {
          // enter и over: курсор с файлом над окном.
          setDragging(true);
        }
      })
      .then((fn) => {
        if (cancelled) fn();
        else unlisten = fn;
      });
    return () => {
      cancelled = true;
      unlisten?.();
    };
  }, [open, decodePath]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/60 p-4">
      <div
        role="dialog"
        aria-label={t("subscriptions.qrTitle")}
        className="w-full max-w-md rounded-lg border border-neutral-800 bg-neutral-900 p-5 shadow-xl"
      >
        <p className="text-base font-medium text-neutral-200">
          {t("subscriptions.qrTitle")}
        </p>
        <div
          className={`mt-4 rounded-lg border-2 border-dashed p-6 text-center text-sm transition-colors ${
            dragging
              ? "border-emerald-600 bg-emerald-950/30 text-emerald-200"
              : "border-neutral-700 bg-neutral-950 text-neutral-400"
          }`}
        >
          {t("subscriptions.qrHint")}
        </div>
        {busy ? (
          <p className="mt-2 flex items-center justify-center gap-2 text-xs text-neutral-400">
            <Loader2 className="size-3.5 animate-spin" />
            {t("subscriptions.qrDecoding")}
          </p>
        ) : null}
        {error ? (
          <div className="mt-3">
            <ErrorBox error={error} />
          </div>
        ) : null}
        <input
          ref={fileInputRef}
          type="file"
          accept="image/png,image/jpeg"
          className="sr-only"
          onChange={(event) => {
            const file = event.target.files?.[0];
            if (file) decodeImageFile(file);
            // Сброс value: выбор того же файла второй раз должен работать.
            event.target.value = "";
          }}
        />
        <div className="mt-4 flex gap-2">
          <Button
            variant="primary"
            disabled={busy}
            onClick={() => fileInputRef.current?.click()}
          >
            <QrCode className="size-4" />
            {t("subscriptions.qrPickFile")}
          </Button>
          <Button onClick={handleClose}>{t("subscriptions.cancel")}</Button>
        </div>
      </div>
    </div>
  );
}
