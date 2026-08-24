import { create } from 'zustand'
import {
    getDownloadedTracks,
    getStorageUsed,
    type DownloadProgress,
    type DownloadedTrack
} from '@/lib/tauri'

interface DownloadsState {
    activeDownloads: DownloadProgress[]
    downloadedTracks: DownloadedTrack[]
    downloadedIds: Set<string>
    storageUsed: number
    isLoaded: boolean

    applyActivePush: (activeDownloads: DownloadProgress[] | null) => void
    refresh: () => Promise<void>
    scheduleCompletionRefresh: () => void
}

// Debounce only the heavy side: refetching downloadedTracks + storageUsed.
// Active download progress comes directly in the event payload now, so it
// never needs a round-trip and has no debounce.
const COMPLETION_DEBOUNCE_MS = 400
let debounceTimer: ReturnType<typeof setTimeout> | null = null

export const useDownloadsStore = create<DownloadsState>((set, get) => ({
    activeDownloads: [],
    downloadedTracks: [],
    downloadedIds: new Set(),
    storageUsed: 0,
    isLoaded: false,

    // Called on every "downloads-updated" event — applies the pushed active
    // downloads list immediately (no IPC round-trip, no debounce).
    // When the list is empty a download just completed, so also schedule a
    // refresh to pick up the new downloadedTracks + updated storageUsed.
    applyActivePush: (activeDownloads: DownloadProgress[] | null) => {
        const downloads = activeDownloads ?? []
        set({ activeDownloads: downloads, isLoaded: true })
        if (downloads.length === 0) {
            get().scheduleCompletionRefresh()
        }
    },

    refresh: async () => {
        try {
            const [downloadedTracks, storageUsed] = await Promise.all([
                getDownloadedTracks(),
                getStorageUsed()
            ])
            set({
                downloadedTracks,
                downloadedIds: new Set(
                    downloadedTracks.map((t) => t.video_info.id)
                ),
                storageUsed,
                isLoaded: true
            })
        } catch (error) {
            console.error('Failed to refresh downloads state:', error)
        }
    },

    scheduleCompletionRefresh: () => {
        if (debounceTimer) clearTimeout(debounceTimer)
        debounceTimer = setTimeout(() => {
            debounceTimer = null
            void get().refresh()
        }, COMPLETION_DEBOUNCE_MS)
    }
}))
