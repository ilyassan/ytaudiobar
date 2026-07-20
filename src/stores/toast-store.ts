import { create } from 'zustand'

export interface Toast {
    id: number
    message: string
    variant: 'error' | 'success'
}

interface ToastState {
    toasts: Toast[]
    show: (message: string, variant?: Toast['variant']) => void
    dismiss: (id: number) => void
}

const AUTO_DISMISS_MS = 4000

let nextId = 0

// A single shared toast queue so any async failure -- search, download, play,
// queue/playlist actions -- can surface to the user instead of only going to
// console.error, without every call site needing its own UI state.
export const useToastStore = create<ToastState>((set, get) => ({
    toasts: [],

    show: (message, variant = 'error') => {
        const id = nextId++
        set((s) => ({ toasts: [...s.toasts, { id, message, variant }] }))
        setTimeout(() => get().dismiss(id), AUTO_DISMISS_MS)
    },

    dismiss: (id) => {
        set((s) => ({ toasts: s.toasts.filter((t) => t.id !== id) }))
    }
}))
