import {
    useEffect,
    useRef,
    type Dispatch,
    type SetStateAction,
    type RefObject
} from 'react'
import { type AudioState, togglePlayPause, seekTo } from '@/lib/tauri'

interface UseKeyboardShortcutsArgs {
    audioState: AudioState | null
    setAudioState: Dispatch<SetStateAction<AudioState | null>>
    // Shared with the playback-state-changed listener in home.tsx, which merges
    // backend state against whatever optimistic seek position is in flight --
    // owned by the caller rather than this hook so both places see the same value.
    positionRef: RefObject<number>
    targetSeekRef: RefObject<number | null>
}

// Space to toggle play/pause, Left/Right arrows to seek +/-5s. Registered once
// (reads latest state via a ref) instead of re-subscribing on every audioState
// tick (every 500ms during playback). Extracted from home.tsx.
export function useKeyboardShortcuts({
    audioState,
    setAudioState,
    positionRef,
    targetSeekRef
}: UseKeyboardShortcutsArgs) {
    const audioStateRef = useRef<AudioState | null>(null)
    const seekDebounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)

    useEffect(() => {
        audioStateRef.current = audioState
    }, [audioState])

    useEffect(() => {
        const applySeekStep = (delta: number) => {
            const state = audioStateRef.current
            if (!state) return null
            const newPosition = Math.max(
                0,
                Math.min(state.duration, positionRef.current + delta)
            )
            positionRef.current = newPosition
            targetSeekRef.current = newPosition
            setAudioState({ ...state, current_position: newPosition })
            return newPosition
        }

        const sendSeekNow = (position: number) => {
            if (seekDebounceRef.current) {
                clearTimeout(seekDebounceRef.current)
                seekDebounceRef.current = null
            }
            seekTo(position).catch(console.error)
        }

        const handleKeyDown = (e: KeyboardEvent) => {
            // Don't trigger if user is typing in an input/textarea
            const target = e.target as HTMLElement
            if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA') {
                return
            }

            switch (e.key) {
                case ' ': // Space bar - toggle play/pause
                    e.preventDefault()
                    togglePlayPause().catch(console.error)
                    break
                case 'ArrowLeft': // Left arrow - seek backward 5s
                case 'ArrowRight': {
                    // Right arrow - seek forward 5s
                    e.preventDefault()
                    const newPosition = applySeekStep(
                        e.key === 'ArrowLeft' ? -5 : 5
                    )
                    if (newPosition === null) break

                    if (!e.repeat) {
                        // A single tap seeks immediately -- no added latency.
                        sendSeekNow(newPosition)
                    } else {
                        // Holding the key: the visual position above already moved
                        // instantly on every repeat tick, but respawning ffmpeg on
                        // every ~30-50ms repeat while holding would be wasteful --
                        // debounce the actual backend seek so it fires once repeats
                        // settle (or immediately on keyup, see below).
                        if (seekDebounceRef.current) {
                            clearTimeout(seekDebounceRef.current)
                        }
                        seekDebounceRef.current = setTimeout(() => {
                            seekDebounceRef.current = null
                            seekTo(newPosition).catch(console.error)
                        }, 150)
                    }
                    break
                }
            }
        }

        const handleKeyUp = (e: KeyboardEvent) => {
            if (
                (e.key === 'ArrowLeft' || e.key === 'ArrowRight') &&
                seekDebounceRef.current &&
                targetSeekRef.current !== null
            ) {
                // Releasing the key should feel instant, not wait out the debounce.
                sendSeekNow(targetSeekRef.current)
            }
        }

        window.addEventListener('keydown', handleKeyDown)
        window.addEventListener('keyup', handleKeyUp)
        return () => {
            window.removeEventListener('keydown', handleKeyDown)
            window.removeEventListener('keyup', handleKeyUp)
            if (seekDebounceRef.current) {
                clearTimeout(seekDebounceRef.current)
            }
        }
    }, [])
}
