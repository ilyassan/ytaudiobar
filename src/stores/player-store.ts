import { create } from 'zustand'
import type { YTVideoInfo } from '@/lib/tauri'

interface PlayerState {
    currentTrack: YTVideoInfo | null
    isPlaying: boolean
    loadingTrackId: string | null

    setCurrentTrack: (track: YTVideoInfo | null) => void
    setIsPlaying: (playing: boolean) => void
    setLoadingTrack: (trackId: string | null) => void
}

export const usePlayerStore = create<PlayerState>((set) => ({
    currentTrack: null,
    isPlaying: false,
    loadingTrackId: null,

    setCurrentTrack: (track) => set({ currentTrack: track }),
    setIsPlaying: (playing) => set({ isPlaying: playing }),
    setLoadingTrack: (trackId) => set({ loadingTrackId: trackId }),
}))
