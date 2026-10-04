import { useEffect } from "react";
import { useToastStore } from "../store/toast";
import { Icon } from "./Icon";

const VISIBLE_MS = 3500;

/** Live region for the app-wide toast. Errors stay a little longer and are announced assertively. */
export function ToastHost() {
  const toast = useToastStore((s) => s.toast);
  const dismiss = useToastStore((s) => s.dismiss);

  useEffect(() => {
    if (!toast) return;
    const id = window.setTimeout(
      () => dismiss(toast.id),
      toast.tone === "error" ? VISIBLE_MS * 1.6 : VISIBLE_MS,
    );
    return () => window.clearTimeout(id);
  }, [toast, dismiss]);

  return (
    <div
      role={toast?.tone === "error" ? "alert" : "status"}
      className="pointer-events-none fixed bottom-8 left-1/2 z-[200] -translate-x-1/2 max-[900px]:bottom-20"
    >
      {toast && (
        <div
          key={toast.id}
          className="toast-in flex items-center gap-2.5 rounded-lg bg-raised-hi px-[18px] py-3 text-[14.5px] font-semibold shadow-[0_12px_32px_rgba(0,0,0,.5)]"
        >
          <Icon
            name={toast.tone === "error" ? "alert" : "check"}
            size={18}
            className={`flex-none ${toast.tone === "error" ? "text-danger" : "text-green"}`}
          />
          {toast.text}
        </div>
      )}
    </div>
  );
}
