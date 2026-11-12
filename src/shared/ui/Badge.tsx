import type { ReactNode } from "react";

type Tone = "neutral" | "success" | "warn" | "danger" | "info";

const tones: Record<Tone, string> = {
  neutral: "border-neutral-700 bg-neutral-800 text-neutral-300",
  success: "border-emerald-800 bg-emerald-950/50 text-emerald-300",
  warn: "border-amber-800 bg-amber-950/40 text-amber-200",
  danger: "border-red-900 bg-red-950/40 text-red-200",
  info: "border-sky-900 bg-sky-950/40 text-sky-200",
};

export function Badge({
  tone = "neutral",
  children,
}: {
  tone?: Tone;
  children: ReactNode;
}) {
  return (
    <span
      className={`inline-flex items-center rounded-full border px-2 py-0.5 text-xs font-medium ${tones[tone]}`}
    >
      {children}
    </span>
  );
}

export function Dot({ tone = "neutral" }: { tone?: Tone }) {
  const colors: Record<Tone, string> = {
    neutral: "bg-neutral-500",
    success: "bg-emerald-400",
    warn: "bg-amber-400",
    danger: "bg-red-400",
    info: "bg-sky-400",
  };
  return <span className={`inline-block size-2 rounded-full ${colors[tone]}`} />;
}
