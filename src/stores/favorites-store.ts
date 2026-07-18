import { create } from 'zustand'
import { getAllPlaylists, getPlaylistTracks } from '@/lib/tauri'

interface FavoritesState {
    favoriteTrackIds: Set<string>
    isLoaded: boolean

    refresh: () => Promise<void>
}

export const useFavoritesStore = create<FavoritesState>((set) => ({
    favoriteTrackIds: new Set(),
    isLoaded: false,

    refresh: async () => {
        try {
            const playlists = await getAllPlaylists()
            const favoritesPlaylist = playlists.find(
                (p) => p.is_system_playlist && p.name === 'All Favorites'
            )
            const tracks = favoritesPlaylist
                ? await getPlaylistTracks(favoritesPlaylist.id)
                : []
            set({
                favoriteTrackIds: new Set(tracks.map((t) => t.id)),
                isLoaded: true
            })
        } catch (error) {
            console.error('Failed to refresh favorites:', error)
        }
    }
}))
