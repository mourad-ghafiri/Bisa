/**
 * A list where arrivals and departures are visible.
 *
 * This is the second thing motion is for here: in a feed that updates while
 * you are reading it, a row that simply appears is indistinguishable from a
 * row you had not noticed, and a row that vanishes takes its context with it.
 * A short fade and a four-pixel slide answer "did something just change?"
 * without asking you to look away from what you were reading.
 *
 * It is for lists that change *while on screen* — an inbox, a feed, a set of
 * filter chips. A list that is rebuilt on navigation should not use it: every
 * row would animate in on every screen change, which reads as the app being
 * slow rather than as anything having happened. {@link VirtualList} does not
 * use it either, because it positions rows absolutely and would animate
 * scrolling as insertion.
 *
 * Under reduced motion the durations collapse to zero, so rows appear and
 * disappear instantly and `AnimatePresence` still unmounts them cleanly.
 */

import { AnimatePresence, motion } from "motion/react";
import type { ReactNode } from "react";
import { LIST_ITEM_MOTION, useMotionTiming } from "./motion";

export function AnimatedList<T>({
  items,
  keyOf,
  render,
  className,
}: {
  items: readonly T[];
  /** Stable across re-renders, or every update reads as a full replacement. */
  keyOf: (item: T, index: number) => string;
  render: (item: T, index: number) => ReactNode;
  className?: string;
}) {
  const timing = useMotionTiming("fast");
  return (
    <div className={className}>
      <AnimatePresence initial={false}>
        {items.map((item, i) => (
          <motion.div key={keyOf(item, i)} layout transition={timing} {...LIST_ITEM_MOTION}>
            {render(item, i)}
          </motion.div>
        ))}
      </AnimatePresence>
    </div>
  );
}
