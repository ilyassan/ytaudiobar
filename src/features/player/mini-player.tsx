import { Volume2, VolumeX, SkipBack, SkipForward, Play, Pause, ChevronDown } from 'lucide-react'
import { togglePlayPause, playPrevious, playNext, type YTVideoInfo } from '@/lib/tauri'
import { ScrollingText } from '@/components/scrolling-text'

interface MiniPlayerProps {
    track: YTVideoInfo
    isPlaying: boolean
    onExpand: () => void
}

export function MiniPlayer({ track, isPlaying, onExpand }: MiniPlayerProps) {
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

    return (
        <div className="h-[44px] border-t border-macos-separator bg-card px-3 flex items-center gap-2 flex-shrink-0">
            {/* Speaker Icon - 11px */}
            <div className="flex-shrink-0">
                {isPlaying ? (
                    <Volume2 className="w-[11px] h-[11px] text-muted-foreground" />
                ) : (
                    <VolumeX className="w-[11px] h-[11px] text-muted-foreground" />
                )}
            </div>

            {/* Track Info - Single line with separator */}
            <div className="flex-1 min-w-0 text-[13px] text-foreground">
                <ScrollingText
                    text={`${track.title} • ${track.uploader}`}
                    className="font-medium"
                    speed={50}
                />
            </div>

            {/* Divider */}
            <div className="w-[1px] h-4 bg-muted-foreground/30" />

            {/* Control Buttons */}
            <div className="flex items-center gap-1 flex-shrink-0">
                {/* Previous - 11px */}
                <button
                    onClick={handlePrevious}
                    className="w-6 h-6 flex items-center justify-center hover-macos-button rounded"
                    aria-label="Previous track"
                >
                    <SkipBack className="w-[11px] h-[11px] text-foreground fill-foreground" />
                </button>

                {/* Play/Pause - 18px */}
                <button
                    onClick={handleTogglePlayPause}
                    className="w-7 h-7 flex items-center justify-center hover-macos-button rounded"
                    aria-label={isPlaying ? 'Pause' : 'Play'}
                >
                    {isPlaying ? (
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
                >
                    <SkipForward className="w-[11px] h-[11px] text-foreground fill-foreground" />
                </button>
            </div>

            {/* Divider */}
            <div className="w-[1px] h-4 bg-muted-foreground/30" />

            {/* Expand Button - 11px */}
            <button
                onClick={onExpand}
                className="w-6 h-6 flex items-center justify-center hover-macos-button rounded flex-shrink-0"
                aria-label="Expand player"
            >
                <ChevronDown className="w-[11px] h-[11px] text-muted-foreground" />
            </button>
        </div>
    )
}
