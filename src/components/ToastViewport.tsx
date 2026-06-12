// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { clsx } from "clsx";
import type { Toast } from "../hooks/useToasts";

interface ToastViewportProps {
  toasts: Toast[];
  onDismiss: (id: string) => void;
}

const tones = {
  success: "toast--success",
  error: "toast--error",
  info: "toast--info",
};

const labels = {
  success: "OK",
  error: "ER",
  info: "IN",
};

export function ToastViewport({ toasts, onDismiss }: ToastViewportProps) {
  return (
    <div className="pointer-events-none fixed right-5 top-5 z-50 flex w-[360px] max-w-[calc(100vw-2rem)] flex-col gap-3">
      {toasts.map((toast) => (
        <div
          key={toast.id}
          className={clsx(
            "toast pointer-events-auto",
            toast.fading ? "animate-toast-out" : "animate-panel-in",
            tones[toast.kind],
          )}
        >
          <div className="flex items-start gap-3">
            <span className="toast-mark">{labels[toast.kind]}</span>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-bold">{toast.title}</p>
              {toast.message ? <p className="toast-message mt-1 truncate text-xs">{toast.message}</p> : null}
            </div>
            <button type="button" className="toast-dismiss" onClick={() => onDismiss(toast.id)}>
              Dismiss
            </button>
          </div>
        </div>
      ))}
    </div>
  );
}
