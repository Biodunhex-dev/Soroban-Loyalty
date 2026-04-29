"use client";

import { useEffect, useRef } from "react";
import { FocusTrap } from "./FocusTrap";

interface Props {
  open: boolean;
  title: string;
  description: string;
  confirmLabel?: string;
  cancelLabel?: string;
  loading?: boolean;
  onConfirm: () => void;
  onCancel: () => void;
}

/**
 * Accessible confirmation dialog for irreversible actions.
 * - Cancel button is focused by default (safe default).
 * - Confirm button uses destructive red styling.
 * - Focus is trapped while open (WCAG 2.1 AA).
 * - Closes on Escape key.
 */
export function ConfirmDialog({
  open,
  title,
  description,
  confirmLabel = "Confirm",
  cancelLabel = "Cancel",
  loading = false,
  onConfirm,
  onCancel,
}: Props) {
  const cancelRef = useRef<HTMLButtonElement>(null);

  useEffect(() => {
    if (!open) return;
    // Focus cancel button when dialog opens
    cancelRef.current?.focus();

    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") onCancel();
    };
    document.addEventListener("keydown", handleKeyDown);
    return () => document.removeEventListener("keydown", handleKeyDown);
  }, [open, onCancel]);

  if (!open) return null;

  return (
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby="confirm-dialog-title"
      aria-describedby="confirm-dialog-desc"
      style={{
        position: "fixed",
        inset: 0,
        zIndex: 50,
        display: "flex",
        alignItems: "center",
        justifyContent: "center",
        background: "rgba(0,0,0,0.6)",
      }}
    >
      <FocusTrap active={open}>
        <div
          style={{
            background: "#1a1d27",
            border: "1px solid #2d3148",
            borderRadius: 12,
            padding: "24px 28px",
            maxWidth: 420,
            width: "90vw",
            boxShadow: "0 8px 32px rgba(0,0,0,0.5)",
          }}
        >
          <h2
            id="confirm-dialog-title"
            style={{ margin: "0 0 8px", fontSize: "1.1rem", fontWeight: 700, color: "#e2e8f0" }}
          >
            {title}
          </h2>
          <p
            id="confirm-dialog-desc"
            style={{ margin: "0 0 20px", color: "#94a3b8", fontSize: "0.9rem", lineHeight: 1.5 }}
          >
            {description}
          </p>
          <div style={{ display: "flex", gap: 10, justifyContent: "flex-end" }}>
            <button
              ref={cancelRef}
              className="btn btn-outline"
              onClick={onCancel}
              disabled={loading}
              style={{ minWidth: 80 }}
            >
              {cancelLabel}
            </button>
            <button
              className="btn btn-primary"
              onClick={onConfirm}
              disabled={loading}
              style={{ minWidth: 80, background: "#dc2626", borderColor: "#dc2626" }}
            >
              {loading ? "Processing…" : confirmLabel}
            </button>
          </div>
        </div>
      </FocusTrap>
    </div>
  );
}
