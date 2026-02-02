import { useState, useEffect } from 'react'
import { Play, Pause, SkipBack, SkipForward, ChevronDown, MinusCircle, PlusCircle, Loader2 } from 'lucide-react'
import { togglePlayPause, playPrevious, playNext, seekTo, setPlaybackSpeed, formatTime, type AudioState } from '@/lib/tauri'
import { ScrollingText } from '@/components/scrolling-text'

interface ExpandedPlayerProps {
    audioState: AudioState
    onCollapse: () => void
}

export function ExpandedPlayer({ audioState, onCollapse }: ExpandedPlayerProps) {
    const [position, setPosition] = useState(audioState.current_position)
    const [playbackRate, setPlaybackRate] = useState(audioState.playback_rate)

    useEffect(() => {
        setPosition(audioState.current_position)
        setPlaybackRate(audioState.playback_rate)
    }, [audioState])

    const handleTogglePlayPause = async () => {
        try {
            await togglePlayPause()
        } catch (error) {
            console.error('Failed to toggle play/pause:', error)
        }
    }

    const handlePrevious = async () => {
        try {
            await playPrevious()
        } catch (error) {
            console.error('Failed to play previous:', error)
        }
    }

    const handleNext = async () => {
        try {
            await playNext()
        } catch (error) {
            console.error('Failed to play next:', error)
        }
    }

    const handleSeek = async (e: React.ChangeEvent<HTMLInputElement>) => {
        const newPosition = parseFloat(e.target.value)
        setPosition(newPosition)
        try {
            await seekTo(newPosition)
        } catch (error) {
            console.error('Failed to seek:', error)
        }
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

    const progress = audioState.duration > 0 ? (position / audioState.duration) * 100 : 0

    return (
        <div className="border-t border-macos-separator bg-card flex-shrink-0 px-4 py-4">
            {/* Content */}
            <div className="flex flex-col gap-3">
                {/* Header with track info and collapse button */}
                {audioState.current_track && (
                    <div className="flex items-start justify-between gap-3 mb-3">
                        <div className="flex-1 min-w-0">
                            <div className="mb-0.5">
                                <ScrollingText
                                    text={audioState.current_track.title}
                                    className="text-[15px] font-semibold text-foreground"
                                    speed={50}
                                />
                            </div>
                            <div className="flex items-center gap-2">
                                <p className="text-[13px] text-muted-foreground truncate">
                                    {audioState.current_track.uploader}
                                </p>
                                {audioState.is_loading && (
                                    <Loader2 className="w-3.5 h-3.5 text-muted-foreground animate-spin" />
                                )}
                            </div>
                        </div>
                        <button
                            onClick={onCollapse}
                            className="w-8 h-8 flex items-center justify-center hover-macos-button rounded-full flex-shrink-0"
                            aria-label="Collapse player"
                        >
                            <ChevronDown className="w-5 h-5 text-muted-foreground" />
                        </button>
                    </div>
                )}

                {/* Control Buttons */}
                <div className="flex items-center justify-center gap-6 mb-4">
                    {/* Previous */}
                    <button
                        onClick={handlePrevious}
                        className="w-10 h-10 flex items-center justify-center hover-macos-button rounded-full"
                        aria-label="Previous track"
                    >
                        <SkipBack className="w-5 h-5 text-foreground fill-foreground" />
                    </button>

                    {/* Play/Pause */}
                    <button
                        onClick={handleTogglePlayPause}
                        className="w-12 h-12 flex items-center justify-center rounded-full bg-[var(--macos-blue)] hover:opacity-90 transition-opacity"
                        aria-label={audioState.is_playing ? 'Pause' : 'Play'}
                    >
                        {audioState.is_playing ? (
                            <Pause className="w-6 h-6 text-white fill-white" />
                        ) : (
                            <Play className="w-6 h-6 text-white fill-white ml-0.5" />
                        )}
                    </button>

                    {/* Next */}
                    <button
                        onClick={handleNext}
                        className="w-10 h-10 flex items-center justify-center hover-macos-button rounded-full"
                        aria-label="Next track"
                    >
                        <SkipForward className="w-5 h-5 text-foreground fill-foreground" />
                    </button>
                </div>

                {/* Progress Slider */}
                <div className="mb-1">
                    <input
                        type="range"
                        min="0"
                        max={audioState.duration || 100}
                        value={position}
                        onChange={handleSeek}
                        disabled={audioState.duration === 0}
                        className="w-full h-[6px] bg-muted/30 rounded-full appearance-none cursor-pointer
                                 [&::-webkit-slider-thumb]:appearance-none
                                 [&::-webkit-slider-thumb]:w-3
                                 [&::-webkit-slider-thumb]:h-3
                                 [&::-webkit-slider-thumb]:rounded-full
                                 [&::-webkit-slider-thumb]:bg-white
                                 [&::-webkit-slider-thumb]:shadow-md
                                 disabled:opacity-50 disabled:cursor-not-allowed"
                        style={{
                            background: `linear-gradient(to right, var(--macos-blue) ${progress}%, rgba(255,255,255,0.1) ${progress}%)`
                        }}
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
