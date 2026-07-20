import { useEffect } from 'react'
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
                if (!isPlaying) {
                    togglePlayPause().catch(console.error)
                }
            })
        )

        unlisteners.push(
            listenToMediaKeyPause(() => {
                if (isPlaying) {
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
                if (audioState) {
                    const newPosition = Math.max(
                        0,
                        Math.min(
                            audioState.current_position + offset,
                            audioState.duration
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
    }, [isPlaying, audioState])
}
