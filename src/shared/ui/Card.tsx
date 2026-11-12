import type { ReactNode } from "react";

interface CardProps {
  title: string;
  children: ReactNode;
}

export function Card({ title, children }: CardProps) {
  return (
    <section className="rounded-lg border border-neutral-800 bg-neutral-900/60 p-5">
      <h2 className="mb-3 text-base font-medium">{title}</h2>
      {children}
    </section>
  );
}
