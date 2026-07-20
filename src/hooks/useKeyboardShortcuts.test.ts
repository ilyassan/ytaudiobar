import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest'
import { renderHook } from '@testing-library/react'
import { useKeyboardShortcuts } from './useKeyboardShortcuts'

const { togglePlayPause, seekTo } = vi.hoisted(() => ({
    togglePlayPause: vi.fn().mockResolvedValue(undefined),
    seekTo: vi.fn().mockResolvedValue(undefined)
}))

vi.mock('@/lib/tauri', () => ({ togglePlayPause, seekTo }))

const baseAudioState = (overrides: Partial<Record<string, unknown>> = {}) => ({
    is_playing: false,
    current_position: 50,
    duration: 100,
    volume: 1,
    playback_rate: 1,
    current_track: null,
    is_loading: false,
    download_progress: 1,
    playback_error: null,
    ...overrides
})

function dispatchKey(
    type: 'keydown' | 'keyup',
    key: string,
    opts: { repeat?: boolean; target?: HTMLElement } = {}
) {
    const event = new KeyboardEvent(type, {
        key,
        repeat: opts.repeat ?? false,
        cancelable: true,
        bubbles: true
    })
    if (opts.target) {
        opts.target.dispatchEvent(event)
    } else {
        window.dispatchEvent(event)
    }
    return event
}

// positionRef/targetSeekRef only need to behave like React refs ({ current }),
// not actually be created via useRef -- the hook just reads/writes .current.
function setup(
    initialAudioState: ReturnType<
        typeof baseAudioState
    > | null = baseAudioState()
) {
    const setAudioState = vi.fn()
    const positionRef = { current: initialAudioState?.current_position ?? 0 }
    const targetSeekRef: { current: number | null } = { current: null }

    const { rerender, unmount } = renderHook(
        (props: { audioState: ReturnType<typeof baseAudioState> | null }) =>
            useKeyboardShortcuts({
                audioState: props.audioState,
                setAudioState,
                positionRef,
                targetSeekRef
            }),
        { initialProps: { audioState: initialAudioState } }
    )

    return { setAudioState, positionRef, targetSeekRef, rerender, unmount }
}

describe('useKeyboardShortcuts', () => {
    beforeEach(() => {
        vi.clearAllMocks()
        vi.useFakeTimers()
    })

    afterEach(() => {
        // Global cleanup() in test-setup.ts unmounts any hook still rendered
        // so its window listeners don't leak into the next test.
        vi.useRealTimers()
    })

    it('space bar toggles play/pause', () => {
        setup()
        dispatchKey('keydown', ' ')
        expect(togglePlayPause).toHaveBeenCalledTimes(1)
    })

    it('ignores keydown when the event target is an input element', () => {
        const input = document.createElement('input')
        document.body.appendChild(input)
        setup()

        dispatchKey('keydown', ' ', { target: input })

        expect(togglePlayPause).not.toHaveBeenCalled()
        document.body.removeChild(input)
    })

    it('a single ArrowRight tap seeks immediately with no debounce', () => {
        setup()
        dispatchKey('keydown', 'ArrowRight', { repeat: false })

        // No timer advance needed -- a non-repeat press sends immediately.
        expect(seekTo).toHaveBeenCalledTimes(1)
        expect(seekTo).toHaveBeenCalledWith(55) // 50 + 5
    })

    it('a single ArrowLeft tap seeks backward and clamps at zero', () => {
        setup(baseAudioState({ current_position: 2 }))
        dispatchKey('keydown', 'ArrowLeft', { repeat: false })

        expect(seekTo).toHaveBeenCalledWith(0)
    })

    it('seeking past the duration clamps at duration', () => {
        setup(baseAudioState({ current_position: 98, duration: 100 }))
        dispatchKey('keydown', 'ArrowRight', { repeat: false })

        expect(seekTo).toHaveBeenCalledWith(100)
    })

    it('updates the visual position via setAudioState on every keypress', () => {
        const { setAudioState } = setup()
        dispatchKey('keydown', 'ArrowRight', { repeat: false })

        expect(setAudioState).toHaveBeenCalledWith(
            expect.objectContaining({ current_position: 55 })
        )
    })

    it('holding the key (repeat=true) does not call seekTo immediately', () => {
        setup()
        dispatchKey('keydown', 'ArrowRight', { repeat: true })

        expect(seekTo).not.toHaveBeenCalled()
    })

    it('holding the key fires exactly one debounced seekTo after repeats settle', () => {
        setup()
        dispatchKey('keydown', 'ArrowRight', { repeat: true })
        dispatchKey('keydown', 'ArrowRight', { repeat: true })
        dispatchKey('keydown', 'ArrowRight', { repeat: true })

        vi.advanceTimersByTime(150)

        // Three repeats accumulated positionRef by 5 each (50 -> 65), and only
        // the final debounced call should have gone through.
        expect(seekTo).toHaveBeenCalledTimes(1)
        expect(seekTo).toHaveBeenCalledWith(65)
    })

    it('releasing the key flushes the pending debounce immediately', () => {
        setup()
        dispatchKey('keydown', 'ArrowRight', { repeat: true })
        expect(seekTo).not.toHaveBeenCalled()

        dispatchKey('keyup', 'ArrowRight')

        // Flushed on keyup, without needing to advance the fake timer at all.
        expect(seekTo).toHaveBeenCalledTimes(1)
        expect(seekTo).toHaveBeenCalledWith(55)
    })

    it('keyup with nothing pending is a harmless no-op', () => {
        setup()
        dispatchKey('keyup', 'ArrowRight')
        expect(seekTo).not.toHaveBeenCalled()
    })

    it('does nothing for arrow keys when audioState is null', () => {
        setup(null)
        dispatchKey('keydown', 'ArrowRight', { repeat: false })
        expect(seekTo).not.toHaveBeenCalled()
    })

    it('unmounting removes the listeners and clears any pending debounce', () => {
        const { unmount } = setup()
        dispatchKey('keydown', 'ArrowRight', { repeat: true })

        unmount()
        vi.advanceTimersByTime(200)

        // The debounced seekTo must not fire after unmount -- both because the
        // timer is cleared in the cleanup, and to be sure removeEventListener
        // actually took the listeners off `window`.
        expect(seekTo).not.toHaveBeenCalled()
        dispatchKey('keydown', 'ArrowRight', { repeat: false })
        expect(togglePlayPause).not.toHaveBeenCalled()
    })
})
