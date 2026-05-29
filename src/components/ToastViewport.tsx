import { clsx } from "clsx";
import type { Toast } from "../hooks/useToasts";

interface ToastViewportProps {
  toasts: Toast[];
  onDismiss: (id: string) => void;
}

const tones = {
  success: "border-emerald-900 bg-emerald-950 text-emerald-200",
  error: "border-red-900 bg-red-950 text-red-200",
  info: "border-orange-900 bg-stone-950 text-orange-200",
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
          className={clsx("pointer-events-auto animate-panel-in rounded-xl border px-4 py-3", tones[toast.kind])}
        >
          <div className="flex items-start gap-3">
            <span className="toast-mark">{labels[toast.kind]}</span>
            <div className="min-w-0 flex-1">
              <p className="truncate text-sm font-bold">{toast.title}</p>
              {toast.message ? <p className="mt-1 truncate text-xs text-stone-400">{toast.message}</p> : null}
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
