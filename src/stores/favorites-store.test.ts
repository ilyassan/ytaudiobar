import { describe, it, expect, vi, beforeEach } from 'vitest'

const getAllPlaylists = vi.fn()
const getPlaylistTracks = vi.fn()

vi.mock('@/lib/tauri', () => ({
    getAllPlaylists: (...args: unknown[]) => getAllPlaylists(...args),
    getPlaylistTracks: (...args: unknown[]) => getPlaylistTracks(...args)
}))

import { useFavoritesStore } from './favorites-store'

const playlist = (overrides: Partial<Record<string, unknown>> = {}) => ({
    id: 'favorites',
    name: 'All Favorites',
    created_date: 1700000000,
    is_system_playlist: true,
    ...overrides
})

describe('favorites-store', () => {
    beforeEach(() => {
        getAllPlaylists.mockReset()
        getPlaylistTracks.mockReset()
        useFavoritesStore.setState({
            favoriteTrackIds: new Set(),
            isLoaded: false
        })
    })

    it('starts unloaded with no favorite ids', () => {
        const state = useFavoritesStore.getState()
        expect(state.isLoaded).toBe(false)
        expect(state.favoriteTrackIds.size).toBe(0)
    })

    it('refresh() finds the system "All Favorites" playlist and loads its tracks', async () => {
        getAllPlaylists.mockResolvedValue([
            playlist(),
            playlist({ id: 'p2', name: 'Other', is_system_playlist: false })
        ])
        getPlaylistTracks.mockResolvedValue([
            {
                id: 't1',
                title: 'A',
                author: null,
                duration: 1,
                thumbnail_url: null,
                added_date: 0,
                file_path: null
            },
            {
                id: 't2',
                title: 'B',
                author: null,
                duration: 1,
                thumbnail_url: null,
                added_date: 0,
                file_path: null
            }
        ])

        await useFavoritesStore.getState().refresh()

        const state = useFavoritesStore.getState()
        expect(state.isLoaded).toBe(true)
        expect(state.favoriteTrackIds.has('t1')).toBe(true)
        expect(state.favoriteTrackIds.has('t2')).toBe(true)
        expect(getPlaylistTracks).toHaveBeenCalledWith('favorites')
    })

    it('refresh() only matches a playlist that is both system AND named "All Favorites"', async () => {
        getAllPlaylists.mockResolvedValue([
            playlist({
                id: 'sys-other',
                name: 'Something Else',
                is_system_playlist: true
            }),
            playlist({
                id: 'user-favorites',
                name: 'All Favorites',
                is_system_playlist: false
            })
        ])

        await useFavoritesStore.getState().refresh()

        expect(getPlaylistTracks).not.toHaveBeenCalled()
        expect(useFavoritesStore.getState().favoriteTrackIds.size).toBe(0)
        expect(useFavoritesStore.getState().isLoaded).toBe(true)
    })

    it('refresh() marks isLoaded true even when no favorites playlist exists', async () => {
        getAllPlaylists.mockResolvedValue([])

        await useFavoritesStore.getState().refresh()

        const state = useFavoritesStore.getState()
        expect(state.isLoaded).toBe(true)
        expect(state.favoriteTrackIds.size).toBe(0)
        expect(getPlaylistTracks).not.toHaveBeenCalled()
    })

    it('refresh() swallows errors without throwing', async () => {
        getAllPlaylists.mockRejectedValue(new Error('backend down'))
        await expect(
            useFavoritesStore.getState().refresh()
        ).resolves.toBeUndefined()
        expect(useFavoritesStore.getState().isLoaded).toBe(false)
    })

    it('refresh() replaces the previous favorite set rather than merging with it', async () => {
        getAllPlaylists.mockResolvedValue([playlist()])
        getPlaylistTracks.mockResolvedValueOnce([
            {
                id: 'old',
                title: 'Old',
                author: null,
                duration: 1,
                thumbnail_url: null,
                added_date: 0,
                file_path: null
            }
        ])
        await useFavoritesStore.getState().refresh()
        expect(useFavoritesStore.getState().favoriteTrackIds.has('old')).toBe(
            true
        )

        getPlaylistTracks.mockResolvedValueOnce([
            {
                id: 'new',
                title: 'New',
                author: null,
                duration: 1,
                thumbnail_url: null,
                added_date: 0,
                file_path: null
            }
        ])
        await useFavoritesStore.getState().refresh()

        const { favoriteTrackIds } = useFavoritesStore.getState()
        expect(favoriteTrackIds.has('new')).toBe(true)
        expect(favoriteTrackIds.has('old')).toBe(false)
    })
})
