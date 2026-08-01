import { useEffect, useRef } from 'react'
import {
    type AudioState,
    togglePlayPause,
    playNext as playNextTrack,
    playPrevious as playPreviousTrack,
    seekTo,
    listenToMediaKeyToggle,
    listenToMediaKeyNext,
    listenToMediaKeyPrevious,
    listenToMediaKeyPlay,
    listenToMediaKeyPause,
    listenToMediaKeySeek,
    listenToMediaKeySeekTo
} from '@/lib/tauri'

// Wires OS media-key events (play/pause/next/previous/seek) to the backend
// playback commands. Extracted from home.tsx so that page component isn't also
// responsible for this OS-integration concern.
export function useMediaKeys(
    isPlaying: boolean,
    audioState: AudioState | null
) {
    // Read through refs rather than closing over the values directly, so the
    // effect below can register its listeners exactly once.
    //
    // `audioState` is a fresh object on every backend tick (playback state is
    // emitted twice a second while playing), so depending on it here meant
    // tearing down and re-registering all seven listeners 2x/second for the
    // whole length of every track -- 28 IPC round-trips a second of pure
    // churn, and Tauri's unlisten leaves a dead bookkeeping entry behind each
    // time, so the registry grew without bound.
    const isPlayingRef = useRef(isPlaying)
    const audioStateRef = useRef(audioState)
    isPlayingRef.current = isPlaying
    audioStateRef.current = audioState

    useEffect(() => {
        const unlisteners: Promise<() => void>[] = []

        // Play/Pause/Toggle
        unlisteners.push(
            listenToMediaKeyToggle(() => {
                togglePlayPause().catch(console.error)
            })
        )

        unlisteners.push(
            listenToMediaKeyPlay(() => {
                if (!isPlayingRef.current) {
                    togglePlayPause().catch(console.error)
                }
            })
        )

        unlisteners.push(
            listenToMediaKeyPause(() => {
                if (isPlayingRef.current) {
                    togglePlayPause().catch(console.error)
                }
            })
        )

        // Next/Previous
        unlisteners.push(
            listenToMediaKeyNext(() => {
                playNextTrack().catch(console.error)
            })
        )

        unlisteners.push(
            listenToMediaKeyPrevious(() => {
                playPreviousTrack().catch(console.error)
            })
        )

        // Seeking
        unlisteners.push(
            listenToMediaKeySeek((offset) => {
                const state = audioStateRef.current
                if (state) {
                    const newPosition = Math.max(
                        0,
                        Math.min(
                            state.current_position + offset,
                            state.duration
                        )
                    )
                    seekTo(newPosition).catch(console.error)
                }
            })
        )

        unlisteners.push(
            listenToMediaKeySeekTo((position) => {
                seekTo(position).catch(console.error)
            })
        )

        return () => {
            Promise.all(unlisteners).then((fns) => fns.forEach((fn) => fn()))
        }
        // Registered once for the lifetime of the hook; the handlers read
        // current values from the refs above.
    }, [])
}
