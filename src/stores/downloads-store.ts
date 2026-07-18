import { create } from 'zustand'
import {
    getActiveDownloads,
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

    refresh: () => Promise<void>
    scheduleRefresh: () => void
}

// The backend's "downloads-updated" event fires on every yt-dlp progress line —
// potentially several times a second during an active download. Debouncing here
// keeps IPC traffic bounded to roughly this interval regardless of how fast the
// backend emits, instead of refetching on every single tick.
const REFRESH_DEBOUNCE_MS = 400
let debounceTimer: ReturnType<typeof setTimeout> | null = null

export const useDownloadsStore = create<DownloadsState>((set, get) => ({
    activeDownloads: [],
    downloadedTracks: [],
    downloadedIds: new Set(),
    storageUsed: 0,
    isLoaded: false,

    refresh: async () => {
        try {
            const [activeDownloads, downloadedTracks, storageUsed] =
                await Promise.all([
                    getActiveDownloads(),
                    getDownloadedTracks(),
                    getStorageUsed()
                ])
            set({
                activeDownloads,
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

    scheduleRefresh: () => {
        if (debounceTimer) clearTimeout(debounceTimer)
        debounceTimer = setTimeout(() => {
            debounceTimer = null
            void get().refresh()
        }, REFRESH_DEBOUNCE_MS)
    }
}))
