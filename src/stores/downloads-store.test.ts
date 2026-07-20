import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'

const getActiveDownloads = vi.fn()
const getDownloadedTracks = vi.fn()
const getStorageUsed = vi.fn()

vi.mock('@/lib/tauri', () => ({
    getActiveDownloads: (...args: unknown[]) => getActiveDownloads(...args),
    getDownloadedTracks: (...args: unknown[]) => getDownloadedTracks(...args),
    getStorageUsed: (...args: unknown[]) => getStorageUsed(...args)
}))

import { useDownloadsStore } from './downloads-store'

const downloadedTrack = (id: string) => ({
    video_info: {
        id,
        title: `Track ${id}`,
        uploader: 'Uploader',
        duration: 100,
        thumbnail_url: null,
        audio_url: null,
        description: null
    },
    file_path: `/downloads/${id}.mp3`,
    file_size: 1000,
    download_date: 1700000000
})

describe('downloads-store', () => {
    beforeEach(() => {
        vi.useFakeTimers()
        getActiveDownloads.mockReset().mockResolvedValue([])
        getDownloadedTracks.mockReset().mockResolvedValue([])
        getStorageUsed.mockReset().mockResolvedValue(0)
        useDownloadsStore.setState({
            activeDownloads: [],
            downloadedTracks: [],
            downloadedIds: new Set(),
            storageUsed: 0,
            isLoaded: false
        })
    })

    afterEach(() => {
        vi.useRealTimers()
    })

    it('starts unloaded with empty collections', () => {
        const state = useDownloadsStore.getState()
        expect(state.isLoaded).toBe(false)
        expect(state.activeDownloads).toEqual([])
        expect(state.downloadedTracks).toEqual([])
    })

    it('refresh() populates state from all three backend calls', async () => {
        getActiveDownloads.mockResolvedValue([
            {
                video_id: 'a',
                progress: 0.5,
                speed: '',
                eta: '',
                file_size: '',
                is_completed: false,
                error: null
            }
        ])
        getDownloadedTracks.mockResolvedValue([downloadedTrack('b')])
        getStorageUsed.mockResolvedValue(12345)

        await useDownloadsStore.getState().refresh()

        const state = useDownloadsStore.getState()
        expect(state.isLoaded).toBe(true)
        expect(state.activeDownloads).toHaveLength(1)
        expect(state.downloadedTracks).toHaveLength(1)
        expect(state.storageUsed).toBe(12345)
    })

    it('refresh() derives downloadedIds from downloadedTracks', async () => {
        getDownloadedTracks.mockResolvedValue([
            downloadedTrack('x'),
            downloadedTrack('y')
        ])

        await useDownloadsStore.getState().refresh()

        const { downloadedIds } = useDownloadsStore.getState()
        expect(downloadedIds.has('x')).toBe(true)
        expect(downloadedIds.has('y')).toBe(true)
        expect(downloadedIds.has('z')).toBe(false)
    })

    it('refresh() swallows errors and leaves isLoaded false on first failure', async () => {
        getActiveDownloads.mockRejectedValue(new Error('backend down'))

        await expect(
            useDownloadsStore.getState().refresh()
        ).resolves.toBeUndefined()
        expect(useDownloadsStore.getState().isLoaded).toBe(false)
    })

    it('refresh() does not clobber previously-loaded state on a later failure', async () => {
        getDownloadedTracks.mockResolvedValue([downloadedTrack('a')])
        await useDownloadsStore.getState().refresh()
        expect(useDownloadsStore.getState().isLoaded).toBe(true)

        getActiveDownloads.mockRejectedValue(new Error('transient failure'))
        await useDownloadsStore.getState().refresh()

        // The failed refresh's catch block never calls set(), so the last
        // successful snapshot survives instead of being wiped out.
        expect(useDownloadsStore.getState().isLoaded).toBe(true)
        expect(useDownloadsStore.getState().downloadedTracks).toHaveLength(1)
    })

    it('scheduleRefresh() debounces multiple calls into a single refresh', () => {
        useDownloadsStore.getState().scheduleRefresh()
        useDownloadsStore.getState().scheduleRefresh()
        useDownloadsStore.getState().scheduleRefresh()

        vi.advanceTimersByTime(400)

        expect(getActiveDownloads).toHaveBeenCalledTimes(1)
    })

    it('scheduleRefresh() does not fire before the debounce window elapses', () => {
        useDownloadsStore.getState().scheduleRefresh()
        vi.advanceTimersByTime(399)
        expect(getActiveDownloads).not.toHaveBeenCalled()
    })
})
