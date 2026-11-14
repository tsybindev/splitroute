// Общий чекбокс с подписью: используется настройками и карточкой обновлений.
export function Checkbox({
  checked,
  onChange,
  label,
  disabled,
}: {
  checked: boolean;
  onChange: (value: boolean) => void;
  label: string;
  disabled?: boolean;
}) {
  return (
    <label className="flex items-center gap-2 text-sm text-neutral-200">
      <input
        type="checkbox"
        checked={checked}
        disabled={disabled}
        onChange={(event) => onChange(event.target.checked)}
        className="size-4 accent-emerald-600"
      />
      {label}
    </label>
  );
}
