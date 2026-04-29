"use client";

import { useEffect, useId, useRef, useState } from "react";

interface Props {
  content: string;
  children: React.ReactElement<React.HTMLAttributes<HTMLElement>>;
  delay?: number;
}

/**
 * Wraps a single child element and shows a tooltip on hover/focus.
 * Positions itself to avoid viewport edges.
 */
export function Tooltip({ content, children, delay = 300 }: Props) {
  const id = useId();
  const [visible, setVisible] = useState(false);
  const [pos, setPos] = useState<{ top: number; left: number }>({ top: 0, left: 0 });
  const triggerRef = useRef<HTMLElement>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const show = () => {
    timerRef.current = setTimeout(() => {
      if (!triggerRef.current) return;
      const rect = triggerRef.current.getBoundingClientRect();
      const TOOLTIP_HEIGHT = 36;
      const TOOLTIP_WIDTH = 220;
      const GAP = 8;

      let top = rect.top - TOOLTIP_HEIGHT - GAP + window.scrollY;
      let left = rect.left + rect.width / 2 - TOOLTIP_WIDTH / 2 + window.scrollX;

      // Flip below if not enough space above
      if (rect.top < TOOLTIP_HEIGHT + GAP) {
        top = rect.bottom + GAP + window.scrollY;
      }
      // Clamp horizontally
      left = Math.max(8, Math.min(left, window.innerWidth - TOOLTIP_WIDTH - 8));

      setPos({ top, left });
      setVisible(true);
    }, delay);
  };

  const hide = () => {
    if (timerRef.current) clearTimeout(timerRef.current);
    setVisible(false);
  };

  useEffect(() => () => { if (timerRef.current) clearTimeout(timerRef.current); }, []);

  // Clone child to inject ref + aria + event handlers
  const child = children as React.ReactElement<React.HTMLAttributes<HTMLElement> & { ref?: React.Ref<HTMLElement> }>;
  const trigger = {
    ...child,
    props: {
      ...child.props,
      ref: triggerRef,
      "aria-describedby": visible ? id : undefined,
      onMouseEnter: (e: React.MouseEvent<HTMLElement>) => { show(); child.props.onMouseEnter?.(e); },
      onMouseLeave: (e: React.MouseEvent<HTMLElement>) => { hide(); child.props.onMouseLeave?.(e); },
      onFocus:      (e: React.FocusEvent<HTMLElement>)  => { show(); child.props.onFocus?.(e); },
      onBlur:       (e: React.FocusEvent<HTMLElement>)  => { hide(); child.props.onBlur?.(e); },
    },
  };

  return (
    <>
      {trigger}
      {visible && (
        <div
          id={id}
          role="tooltip"
          className="tooltip"
          style={{ top: pos.top, left: pos.left }}
        >
          {content}
        </div>
      )}
    </>
  );
}
