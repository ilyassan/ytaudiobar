import { describe, it, expect, vi, beforeEach } from 'vitest'
import { renderHook } from '@testing-library/react'

// vi.mock's factory is hoisted above all imports/top-level code, so anything
// it needs to reference must be created via vi.hoisted() rather than a plain
// top-level const.
const {
    togglePlayPause,
    playNext,
    playPrevious,
    seekTo,
    capturedCallbacks,
    unlistenMocks,
    listenToMediaKeyToggle,
    listenToMediaKeyPlay,
    listenToMediaKeyPause,
    listenToMediaKeyNext,
    listenToMediaKeyPrevious,
    listenToMediaKeySeek,
    listenToMediaKeySeekTo
} = vi.hoisted(() => {
    const capturedCallbacks: Record<string, (...args: unknown[]) => void> = {}
    const unlistenMocks: Record<string, ReturnType<typeof vi.fn>> = {}

    const makeListenerMock = (key: string) => {
        unlistenMocks[key] = vi.fn()
        return vi.fn((cb: (...args: unknown[]) => void) => {
            capturedCallbacks[key] = cb
            return Promise.resolve(unlistenMocks[key])
        })
    }

    return {
        togglePlayPause: vi.fn().mockResolvedValue(undefined),
        playNext: vi.fn().mockResolvedValue(undefined),
        playPrevious: vi.fn().mockResolvedValue(undefined),
        seekTo: vi.fn().mockResolvedValue(undefined),
        capturedCallbacks,
        unlistenMocks,
        listenToMediaKeyToggle: makeListenerMock('toggle'),
        listenToMediaKeyPlay: makeListenerMock('play'),
        listenToMediaKeyPause: makeListenerMock('pause'),
        listenToMediaKeyNext: makeListenerMock('next'),
        listenToMediaKeyPrevious: makeListenerMock('previous'),
        listenToMediaKeySeek: makeListenerMock('seek'),
        listenToMediaKeySeekTo: makeListenerMock('seekTo')
    }
})

vi.mock('@/lib/tauri', () => ({
    togglePlayPause: (...args: unknown[]) => togglePlayPause(...args),
    playNext: (...args: unknown[]) => playNext(...args),
    playPrevious: (...args: unknown[]) => playPrevious(...args),
    seekTo: (...args: unknown[]) => seekTo(...args),
    listenToMediaKeyToggle,
    listenToMediaKeyPlay,
    listenToMediaKeyPause,
    listenToMediaKeyNext,
    listenToMediaKeyPrevious,
    listenToMediaKeySeek,
    listenToMediaKeySeekTo
}))

import { useMediaKeys } from './useMediaKeys'

const audioState = (overrides: Partial<Record<string, unknown>> = {}) => ({
    is_playing: false,
    current_position: 10,
    duration: 100,
    volume: 1,
    playback_rate: 1,
    current_track: null,
    is_loading: false,
    download_progress: 1,
    playback_error: null,
    ...overrides
})

describe('useMediaKeys', () => {
    beforeEach(() => {
        vi.clearAllMocks()
    })

    it('registers a listener for every media key event', async () => {
        renderHook(() => useMediaKeys(false, audioState()))
        await Promise.resolve() // let the effect's async registration settle

        expect(listenToMediaKeyToggle).toHaveBeenCalled()
        expect(listenToMediaKeyPlay).toHaveBeenCalled()
        expect(listenToMediaKeyPause).toHaveBeenCalled()
        expect(listenToMediaKeyNext).toHaveBeenCalled()
        expect(listenToMediaKeyPrevious).toHaveBeenCalled()
        expect(listenToMediaKeySeek).toHaveBeenCalled()
        expect(listenToMediaKeySeekTo).toHaveBeenCalled()
    })

    it('toggle key always calls togglePlayPause', () => {
        renderHook(() => useMediaKeys(false, audioState()))
        capturedCallbacks.toggle()
        expect(togglePlayPause).toHaveBeenCalledTimes(1)
    })

    it('play key only calls togglePlayPause when not already playing', () => {
        renderHook(() => useMediaKeys(false, audioState()))
        capturedCallbacks.play()
        expect(togglePlayPause).toHaveBeenCalledTimes(1)
    })

    it('play key is a no-op when already playing', () => {
        renderHook(() => useMediaKeys(true, audioState()))
        capturedCallbacks.play()
        expect(togglePlayPause).not.toHaveBeenCalled()
    })

    it('pause key only calls togglePlayPause when currently playing', () => {
        renderHook(() => useMediaKeys(true, audioState()))
        capturedCallbacks.pause()
        expect(togglePlayPause).toHaveBeenCalledTimes(1)
    })

    it('pause key is a no-op when already paused', () => {
        renderHook(() => useMediaKeys(false, audioState()))
        capturedCallbacks.pause()
        expect(togglePlayPause).not.toHaveBeenCalled()
    })

    it('next/previous keys call the corresponding playback command', () => {
        renderHook(() => useMediaKeys(false, audioState()))
        capturedCallbacks.next()
        capturedCallbacks.previous()
        expect(playNext).toHaveBeenCalledTimes(1)
        expect(playPrevious).toHaveBeenCalledTimes(1)
    })

    it('relative seek clamps to [0, duration]', () => {
        renderHook(() =>
            useMediaKeys(
                false,
                audioState({ current_position: 95, duration: 100 })
            )
        )
        capturedCallbacks.seek(20) // would overshoot past duration

        expect(seekTo).toHaveBeenCalledWith(100)
    })

    it('relative seek does not go below zero', () => {
        renderHook(() =>
            useMediaKeys(
                false,
                audioState({ current_position: 5, duration: 100 })
            )
        )
        capturedCallbacks.seek(-20)

        expect(seekTo).toHaveBeenCalledWith(0)
    })

    it('absolute seekTo passes the position straight through', () => {
        renderHook(() => useMediaKeys(false, audioState()))
        capturedCallbacks.seekTo(42)
        expect(seekTo).toHaveBeenCalledWith(42)
    })

    it('unmounting calls every unlisten function', async () => {
        const { unmount } = renderHook(() => useMediaKeys(false, audioState()))
        await Promise.resolve()

        unmount()
        // Promise.all(...).then(...) needs a couple of microtask hops to run.
        await Promise.resolve()
        await Promise.resolve()
        await Promise.resolve()

        expect(unlistenMocks.toggle).toHaveBeenCalled()
        expect(unlistenMocks.next).toHaveBeenCalled()
    })
})
