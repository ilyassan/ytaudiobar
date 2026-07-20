import { useState, useEffect, useRef } from 'react'
import {
    Play,
    Pause,
    SkipBack,
    SkipForward,
    MinusCircle,
    PlusCircle,
    Loader2,
    RotateCcw,
    RotateCw
} from 'lucide-react'
import {
    togglePlayPause,
    playPrevious,
    playNext,
    seekTo,
    setPlaybackSpeed,
    formatTime,
    type AudioState
} from '@/lib/tauri'
import { ScrollingText } from '@/components/scrolling-text'
import { useToastStore } from '@/stores/toast-store'

interface ExpandedPlayerProps {
    audioState: AudioState
    onCollapse: () => void
}

export function ExpandedPlayer({
    audioState,
    onCollapse
}: ExpandedPlayerProps) {
    const [position, setPosition] = useState(audioState.current_position)
    const [playbackRate, setPlaybackRate] = useState(audioState.playback_rate)
    const [isSeeking, setIsSeeking] = useState(false)
    const [isSeekingBackend, setIsSeekingBackend] = useState(false)
    const [targetSeekPosition, setTargetSeekPosition] = useState<number | null>(
        null
    )
    const [hoverTime, setHoverTime] = useState<number | null>(null)
    const [hoverX, setHoverX] = useState(0)
    const progressBarRef = useRef<HTMLDivElement>(null)
    useEffect(() => {
        // If we're waiting for backend to catch up to our target position
        if (targetSeekPosition !== null) {
            // Check if backend has caught up (within 0.5 seconds)
            if (
                Math.abs(audioState.current_position - targetSeekPosition) < 0.5
            ) {
                setTargetSeekPosition(null)
                setPosition(audioState.current_position)
            }
            // Otherwise keep showing target position
            return
        }

        // Only update position from audioState if we're not currently seeking
        if (!isSeeking) {
            setPosition(audioState.current_position)
        }
        setPlaybackRate(audioState.playback_rate)
    }, [audioState, isSeeking, targetSeekPosition])

    const handleTogglePlayPause = async () => {
        try {
            if (
                !audioState.is_playing &&
                audioState.duration > 0 &&
                audioState.current_position >= audioState.duration - 0.5
            ) {
                await seekTo(0) // backend seek auto-resumes playback
                return
            }
            await togglePlayPause()
        } catch (error) {
            console.error('Failed to toggle play/pause:', error)
            useToastStore.getState().show('Failed to play/pause')
        }
    }

    const handlePrevious = async () => {
        try {
            await playPrevious()
        } catch (error) {
            console.error('Failed to play previous:', error)
            useToastStore.getState().show('Failed to play previous track')
        }
    }

    const handleNext = async () => {
        try {
            await playNext()
        } catch (error) {
            console.error('Failed to play next:', error)
            useToastStore.getState().show('Failed to play next track')
        }
    }

    const handleSeekBackward = async () => {
        const newPosition = Math.max(0, position - 5)
        setPosition(newPosition)
        setTargetSeekPosition(newPosition)
        setIsSeekingBackend(true)
        try {
            await seekTo(newPosition)
        } catch (error) {
            console.error('Failed to seek backward:', error)
            setTargetSeekPosition(null)
        } finally {
            setIsSeekingBackend(false)
        }
    }

    const handleSeekForward = async () => {
        const newPosition = Math.min(audioState.duration, position + 5)
        setPosition(newPosition)
        setTargetSeekPosition(newPosition)
        setIsSeekingBackend(true)
        try {
            await seekTo(newPosition)
        } catch (error) {
            console.error('Failed to seek forward:', error)
            setTargetSeekPosition(null)
        } finally {
            setIsSeekingBackend(false)
        }
    }

    // Update visual position while dragging (no seek yet)
    const handleSliderChange = (e: React.ChangeEvent<HTMLInputElement>) => {
        const newPosition = parseFloat(e.target.value)
        setPosition(newPosition)
    }

    // Start seeking (mouse down)
    const handleSeekStart = () => {
        setIsSeeking(true)
    }

    // Commit the seek (mouse up / touch end)
    const handleSeekEnd = async (
        e:
            | React.MouseEvent<HTMLInputElement>
            | React.TouchEvent<HTMLInputElement>
    ) => {
        const target = e.currentTarget as HTMLInputElement
        const newPosition = parseFloat(target.value)
        target.blur() // Return focus to document so arrow key shortcuts work immediately

        // Keep slider at target position
        setPosition(newPosition)
        setTargetSeekPosition(newPosition)
        setIsSeeking(false)
        setIsSeekingBackend(true)

        try {
            await seekTo(newPosition)
        } catch (error) {
            console.error('Failed to seek:', error)
            setTargetSeekPosition(null)
        } finally {
            setIsSeekingBackend(false)
        }
    }

    const handleProgressHover = (e: React.MouseEvent<HTMLDivElement>) => {
        if (!progressBarRef.current || audioState.duration === 0) return

        const rect = progressBarRef.current.getBoundingClientRect()
        const offsetX = Math.min(Math.max(e.clientX - rect.left, 0), rect.width)
        const ratio = offsetX / rect.width

        setHoverX(offsetX)
        setHoverTime(ratio * audioState.duration)
    }

    const handleProgressHoverEnd = () => {
        setHoverTime(null)
    }

    const handleSpeedChange = async (delta: number) => {
        const newRate = Math.max(0.25, Math.min(2.0, playbackRate + delta))
        setPlaybackRate(newRate)
        try {
            await setPlaybackSpeed(newRate)
        } catch (error) {
            console.error('Failed to set playback speed:', error)
        }
    }

    const progress =
        audioState.duration > 0 ? (position / audioState.duration) * 100 : 0

    return (
        <div className="border-t border-macos-separator bg-card flex-shrink-0 px-4 py-4">
            {/* Content */}
            <div className="flex flex-col gap-3">
                {/* Header with track info - clickable to collapse */}
                {audioState.current_track && (
                    <button
                        onClick={onCollapse}
                        className="flex flex-col min-w-0 w-full text-left mb-3 px-1 py-1 rounded hover-macos-button cursor-pointer"
                        aria-label="Collapse player"
                    >
                        <div className="mb-0.5 w-full">
                            <ScrollingText
                                text={audioState.current_track.title}
                                className="text-[15px] font-semibold text-foreground"
                                speed={30}
                            />
                        </div>
                        <p className="text-[13px] text-muted-foreground truncate w-full">
                            {audioState.current_track.uploader}
                        </p>
                    </button>
                )}

                {/* Control Buttons */}
                <div className="flex items-center justify-center gap-4 mb-4">
                    {/* Previous */}
                    <button
                        onClick={handlePrevious}
                        className="w-10 h-10 flex items-center justify-center hover-macos-button rounded-full"
                        aria-label="Previous track"
                        title="Previous track"
                    >
                        <SkipBack className="w-5 h-5 text-foreground fill-foreground" />
                    </button>

                    {/* Seek Backward 5s */}
                    <button
                        onClick={handleSeekBackward}
                        className="w-9 h-9 flex items-center justify-center hover-macos-button rounded-full relative disabled:opacity-50"
                        aria-label="Rewind 5 seconds"
                        title="Rewind 5 seconds (←)"
                        disabled={
                            audioState.is_loading ||
                            isSeekingBackend ||
                            audioState.duration === 0
                        }
                    >
                        <RotateCcw className="w-5 h-5 text-foreground" />
                        <span className="absolute text-[7px] font-bold text-foreground">
                            5
                        </span>
                    </button>

                    {/* Play/Pause */}
                    <button
                        onClick={handleTogglePlayPause}
                        className="w-12 h-12 flex items-center justify-center rounded-full bg-[var(--macos-blue)] hover:opacity-90 transition-opacity"
                        aria-label={
                            audioState.is_loading || isSeekingBackend
                                ? 'Loading...'
                                : audioState.is_playing
                                  ? 'Pause'
                                  : 'Play'
                        }
                        title={
                            audioState.is_loading || isSeekingBackend
                                ? 'Loading...'
                                : audioState.is_playing
                                  ? 'Pause (Space)'
                                  : 'Play (Space)'
                        }
                        disabled={audioState.is_loading || isSeekingBackend}
                    >
                        {audioState.is_loading || isSeekingBackend ? (
                            <Loader2 className="w-6 h-6 text-white animate-spin" />
                        ) : audioState.is_playing ? (
                            <Pause className="w-6 h-6 text-white fill-white" />
                        ) : (
                            <Play className="w-6 h-6 text-white fill-white ml-0.5" />
                        )}
                    </button>

                    {/* Seek Forward 5s */}
                    <button
                        onClick={handleSeekForward}
                        className="w-9 h-9 flex items-center justify-center hover-macos-button rounded-full relative disabled:opacity-50"
                        aria-label="Fast forward 5 seconds"
                        title="Fast forward 5 seconds (→)"
                        disabled={
                            audioState.is_loading ||
                            isSeekingBackend ||
                            audioState.duration === 0
                        }
                    >
                        <RotateCw className="w-5 h-5 text-foreground" />
                        <span className="absolute text-[7px] font-bold text-foreground">
                            5
                        </span>
                    </button>

                    {/* Next */}
                    <button
                        onClick={handleNext}
                        className="w-10 h-10 flex items-center justify-center hover-macos-button rounded-full"
                        aria-label="Next track"
                        title="Next track"
                    >
                        <SkipForward className="w-5 h-5 text-foreground fill-foreground" />
                    </button>
                </div>

                {/* Progress Slider */}
                <div
                    ref={progressBarRef}
                    className="relative mb-1 h-3 flex items-center"
                    onMouseMove={handleProgressHover}
                    onMouseLeave={handleProgressHoverEnd}
                >
                    {/* Hover time tooltip */}
                    {hoverTime !== null && (
                        <div
                            className="absolute bottom-full mb-2 -translate-x-1/2 px-2 py-1 rounded-md bg-black/80 text-white text-[11px] font-medium tabular-nums pointer-events-none whitespace-nowrap"
                            style={{ left: `${hoverX}px` }}
                        >
                            {formatTime(hoverTime)}
                        </div>
                    )}

                    {/* Gray base track (unplayed) */}
                    <div className="absolute inset-x-0 h-[6px] rounded-full bg-white/10" />

                    {/* Blue fill (played portion) — same progress% as the dot */}
                    <div
                        className="absolute left-0 h-[6px] rounded-full bg-[var(--macos-blue)]"
                        style={{ width: `${progress}%` }}
                    />

                    {/* Dot — always at the exact right edge of the blue fill */}
                    <div
                        className="absolute w-3 h-3 rounded-full bg-white shadow-md -translate-x-1/2"
                        style={{ left: `${progress}%` }}
                    />

                    {/* Invisible range input on top for interaction */}
                    <input
                        type="range"
                        min="0"
                        max={audioState.duration || 100}
                        value={position}
                        onChange={handleSliderChange}
                        onMouseDown={handleSeekStart}
                        onMouseUp={handleSeekEnd}
                        onTouchStart={handleSeekStart}
                        onTouchEnd={handleSeekEnd}
                        disabled={audioState.duration === 0}
                        className="absolute inset-x-0 w-full opacity-0 cursor-pointer disabled:cursor-not-allowed h-3"
                    />
                </div>

                {/* Time Display and Playback Speed */}
                <div className="flex justify-between items-center text-[11px] text-muted-foreground">
                    <span>{formatTime(position)}</span>

                    {/* Playback Speed Controls */}
                    <div className="flex items-center gap-2">
                        <button
                            onClick={() => handleSpeedChange(-0.25)}
                            disabled={playbackRate <= 0.25}
                            className="w-6 h-6 flex items-center justify-center hover-macos-button rounded disabled:opacity-30 disabled:cursor-not-allowed"
                            aria-label="Decrease playback speed"
                            title="Decrease playback speed"
                        >
                            <MinusCircle className="w-4 h-4 text-foreground" />
                        </button>
                        <span className="text-[13px] font-medium text-foreground min-w-[45px] text-center tabular-nums">
                            {playbackRate.toFixed(2)}x
                        </span>
                        <button
                            onClick={() => handleSpeedChange(0.25)}
                            disabled={playbackRate >= 2.0}
                            className="w-6 h-6 flex items-center justify-center hover-macos-button rounded disabled:opacity-30 disabled:cursor-not-allowed"
                            aria-label="Increase playback speed"
                            title="Increase playback speed"
                        >
                            <PlusCircle className="w-4 h-4 text-foreground" />
                        </button>
                    </div>

                    <span>{formatTime(audioState.duration)}</span>
                </div>
            </div>
        </div>
    )
}
