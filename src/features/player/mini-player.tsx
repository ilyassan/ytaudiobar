import {
    Volume2,
    VolumeX,
    SkipBack,
    SkipForward,
    Play,
    Pause,
    Loader2
} from 'lucide-react'
import { playPrevious, playNext, type YTVideoInfo } from '@/lib/tauri'

interface MiniPlayerProps {
    track: YTVideoInfo
    isPlaying: boolean
    isLoading: boolean
    onExpand: () => void
    onTogglePlayPause: () => void
}

export function MiniPlayer({
    track,
    isPlaying,
    isLoading,
    onExpand,
    onTogglePlayPause
}: MiniPlayerProps) {
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

    return (
        <div className="h-[44px] border-t border-macos-separator bg-card px-3 flex items-center gap-2 flex-shrink-0">
            {/* Speaker Icon - 11px */}
            <div className="flex-shrink-0">
                {isPlaying ? (
                    <Volume2 className="w-[11px] h-[11px] text-[var(--macos-blue)]" />
                ) : (
                    <VolumeX className="w-[11px] h-[11px] text-muted-foreground" />
                )}
            </div>

            {/* Track Info - clickable to expand */}
            <button
                onClick={onExpand}
                className="flex-1 min-w-0 text-[13px] text-foreground font-medium truncate text-left px-1 py-1 rounded hover-macos-button cursor-pointer"
                aria-label="Expand player"
            >
                {track.title} • {track.uploader}
            </button>

            {/* Divider */}
            <div className="w-[1px] h-4 bg-muted-foreground/30" />

            {/* Control Buttons */}
            <div className="flex items-center gap-1 flex-shrink-0">
                {/* Previous - 11px */}
                <button
                    onClick={handlePrevious}
                    className="w-6 h-6 flex items-center justify-center hover-macos-button rounded"
                    aria-label="Previous track"
                    title="Previous track"
                >
                    <SkipBack className="w-[11px] h-[11px] text-foreground fill-foreground" />
                </button>

                {/* Play/Pause - 18px */}
                <button
                    onClick={onTogglePlayPause}
                    className="w-7 h-7 flex items-center justify-center hover-macos-button rounded"
                    aria-label={
                        isLoading ? 'Loading...' : isPlaying ? 'Pause' : 'Play'
                    }
                    title={
                        isLoading
                            ? 'Loading...'
                            : isPlaying
                              ? 'Pause (Space)'
                              : 'Play (Space)'
                    }
                    disabled={isLoading}
                >
                    {isLoading ? (
                        <Loader2 className="w-[18px] h-[18px] text-[var(--macos-blue)] animate-spin" />
                    ) : isPlaying ? (
                        <Pause className="w-[18px] h-[18px] text-[var(--macos-blue)] fill-[var(--macos-blue)]" />
                    ) : (
                        <Play className="w-[18px] h-[18px] text-[var(--macos-blue)] fill-[var(--macos-blue)]" />
                    )}
                </button>

                {/* Next - 11px */}
                <button
                    onClick={handleNext}
                    className="w-6 h-6 flex items-center justify-center hover-macos-button rounded"
                    aria-label="Next track"
                    title="Next track"
                >
                    <SkipForward className="w-[11px] h-[11px] text-foreground fill-foreground" />
                </button>
            </div>
        </div>
    )
}
