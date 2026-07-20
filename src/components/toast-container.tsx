import { AlertCircle, CheckCircle2, X } from 'lucide-react'
import { useToastStore } from '@/stores/toast-store'

// Mounted once at the app root (home.tsx). Renders whatever's currently in the
// shared toast queue -- see stores/toast-store.ts for how entries get added.
export function ToastContainer() {
    const toasts = useToastStore((s) => s.toasts)
    const dismiss = useToastStore((s) => s.dismiss)

    if (toasts.length === 0) return null

    return (
        <div className="fixed bottom-4 left-1/2 -translate-x-1/2 z-[100] flex flex-col gap-2 items-center pointer-events-none">
            {toasts.map((toast) => (
                <div
                    key={toast.id}
                    className={`pointer-events-auto flex items-center gap-2 pl-3 pr-2 py-2 rounded-lg shadow-lg text-[13px] text-white max-w-[320px] ${
                        toast.variant === 'error'
                            ? 'bg-macos-red'
                            : 'bg-[var(--macos-blue)]'
                    }`}
                >
                    {toast.variant === 'error' ? (
                        <AlertCircle className="w-4 h-4 flex-shrink-0" />
                    ) : (
                        <CheckCircle2 className="w-4 h-4 flex-shrink-0" />
                    )}
                    <span className="truncate">{toast.message}</span>
                    <button
                        onClick={() => dismiss(toast.id)}
                        className="w-5 h-5 flex items-center justify-center rounded-full hover:bg-white/20 flex-shrink-0"
                        aria-label="Dismiss"
                    >
                        <X className="w-3.5 h-3.5" />
                    </button>
                </div>
            ))}
        </div>
    )
}
