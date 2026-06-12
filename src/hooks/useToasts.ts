// SPDX-FileCopyrightText: 2025 MisplacedOrange
// SPDX-License-Identifier: GPL-3.0-only

import { useCallback, useState } from "react";

export type ToastKind = "success" | "error" | "info";

export interface Toast {
  id: string;
  title: string;
  message?: string;
  kind: ToastKind;
  fading?: boolean;
}

export function useToasts() {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const removeToast = useCallback((id: string) => {
    setToasts((current) => current.filter((toast) => toast.id !== id));
  }, []);

  const pushToast = useCallback(
    (toast: Omit<Toast, "id" | "fading">) => {
      const id = crypto.randomUUID();
      setToasts((current) => [...current.slice(-4), { ...toast, id }]);
      window.setTimeout(() => {
        setToasts((current) =>
          current.map((t) => (t.id === id ? { ...t, fading: true } : t)),
        );
      }, 4200);
      window.setTimeout(() => removeToast(id), 4400);
    },
    [removeToast],
  );

  return { toasts, pushToast, removeToast };
}
