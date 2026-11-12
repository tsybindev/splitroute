import type { ReactNode } from "react";

interface EmptyStateProps {
  title: string;
  hint?: string;
  children?: ReactNode;
}

export function EmptyState({ title, hint, children }: EmptyStateProps) {
  return (
    <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-neutral-800 px-6 py-10 text-center">
      <p className="text-sm font-medium text-neutral-200">{title}</p>
      {hint ? <p className="max-w-md text-sm text-neutral-400">{hint}</p> : null}
      {children}
    </div>
  );
}
