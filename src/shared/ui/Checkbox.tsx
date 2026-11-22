// Общий чекбокс: с подписью (настройки, карточка обновлений) или без неё
// как переключатель строки — тогда доступность обеспечивает aria-label.
export function Checkbox({
  checked,
  onChange,
  label,
  ariaLabel,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label?: string;
  ariaLabel?: string;
  disabled?: boolean;
}) {
  return (
    <label className="flex items-center gap-2 text-sm text-neutral-200">
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        aria-label={ariaLabel}
        onChange={(event) => onChange(event.target.checked)}
        className="size-4 accent-emerald-600"
      />
      {label}
    </label>
  );
}
