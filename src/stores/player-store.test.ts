import { describe, it, expect, beforeEach } from 'vitest'
import { usePlayerStore } from './player-store'

const track = (id: string) => ({
    id,
    title: `Track ${id}`,
    uploader: 'Uploader',
    duration: 100,
    thumbnail_url: null,
    audio_url: null,
    description: null
})

describe('player-store', () => {
    beforeEach(() => {
        usePlayerStore.setState({
            currentTrack: null,
            isPlaying: false,
            loadingTrackId: null
        })
    })

    it('starts with no track, not playing, nothing loading', () => {
        const state = usePlayerStore.getState()
        expect(state.currentTrack).toBeNull()
        expect(state.isPlaying).toBe(false)
        expect(state.loadingTrackId).toBeNull()
    })

    it('setCurrentTrack updates currentTrack', () => {
        usePlayerStore.getState().setCurrentTrack(track('a'))
        expect(usePlayerStore.getState().currentTrack?.id).toBe('a')
    })

    it('setCurrentTrack(null) clears the current track', () => {
        usePlayerStore.getState().setCurrentTrack(track('a'))
        usePlayerStore.getState().setCurrentTrack(null)
        expect(usePlayerStore.getState().currentTrack).toBeNull()
    })

    it('setIsPlaying toggles isPlaying independently of the other fields', () => {
        usePlayerStore.getState().setCurrentTrack(track('a'))
        usePlayerStore.getState().setIsPlaying(true)

        const state = usePlayerStore.getState()
        expect(state.isPlaying).toBe(true)
        expect(state.currentTrack?.id).toBe('a')
    })

    it('setLoadingTrack sets and clears the loading id', () => {
        usePlayerStore.getState().setLoadingTrack('a')
        expect(usePlayerStore.getState().loadingTrackId).toBe('a')

        usePlayerStore.getState().setLoadingTrack(null)
        expect(usePlayerStore.getState().loadingTrackId).toBeNull()
    })
})
