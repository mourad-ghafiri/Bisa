/**
 * A one-shot wash that says *this changed*: remounted by `flashKey`, it
 * fades from the tone's soft colour to nothing and never repeats on its own.
 * A row that flashes on every render is noise; one that flashes when its
 * key changes — a state, not a token — is a signal. Reduced motion collapses
 * the fade to nothing, so the change is still there to read, only not drawn.
 */

import { motion } from "motion/react";
import type { ReactNode } from "react";
import { useMotionTiming } from "./motion";

const WASH: Record<"accent" | "danger" | "ok" | "neutral", string> = {
  accent: "var(--color-accent-soft)",
  danger: "var(--color-danger-soft)",
  ok: "var(--color-ok-soft, var(--color-accent-soft))",
  neutral: "var(--color-surface-2)",
};

export function Flash({
  flashKey,
  tone = "accent",
  className,
  children,
}: {
  /** Changes exactly when the thing inside changed in a way worth seeing. */
  flashKey: string;
  tone?: keyof typeof WASH;
  className?: string;
  children: ReactNode;
}) {
  const { duration, ease } = useMotionTiming("slow");
  return (
    <motion.div
      key={flashKey}
      className={className}
      initial={duration === 0 ? false : { backgroundColor: WASH[tone] }}
      animate={{ backgroundColor: "rgba(0, 0, 0, 0)" }}
      transition={{ duration: duration * 3, ease: [...ease] }}
    >
      {children}
    </motion.div>
  );
}
