import { useState } from 'react'
import { Play, Pause, Heart, Trash, Loader2, Music } from 'lucide-react'
import { playTrack, type YTVideoInfo, type Track, formatDuration } from '@/lib/tauri'
import { PlaylistSelectionModal } from '@/features/playlists/playlist-selection-modal'

interface TrackItemProps {
    track: YTVideoInfo | Track
    context: 'search' | 'queue' | 'playlist'
    isCurrentTrack?: boolean
    isPlaying?: boolean
    queueIndex?: number
    isFavorite?: boolean
    onRemove?: () => void
    onToggleFavorite?: () => void
}

export function TrackItem({
    track,
    context,
    isCurrentTrack,
    isPlaying,
    queueIndex,
    isFavorite,
    onRemove,
    onToggleFavorite
}: TrackItemProps) {
    const [showPlaylistModal, setShowPlaylistModal] = useState(false)
    const [isLoading, setIsLoading] = useState(false)

    // Convert Track to YTVideoInfo format
    const videoInfo: YTVideoInfo = 'uploader' in track ? track : {
        ...track,
        uploader: track.author || 'Unknown',
        audio_url: null,
        description: null
    }

    const handlePlay = async () => {
        setIsLoading(true)
        try {
            // Play track directly WITHOUT adding to queue
            // Queue is only populated via "Play All" on playlists
            await playTrack(videoInfo)
        } catch (error) {
            console.error('Failed to play track:', error)
        } finally {
            setIsLoading(false)
        }
    }

    // Removed: handleAddToQueue - tracks are no longer manually added to queue
    // Queue is only populated by "Play All" playlist action

    const handleAddToPlaylist = (e: React.MouseEvent) => {
        e.stopPropagation()
        setShowPlaylistModal(true)
    }

    const handleToggleFavorite = (e: React.MouseEvent) => {
        e.stopPropagation()
        if (onToggleFavorite) {
            onToggleFavorite()
        } else {
            // Fallback to add to playlist modal
            setShowPlaylistModal(true)
        }
    }

    return (
        <>
            <div
                className={`flex items-center gap-3 px-3 py-2 hover-macos-button cursor-pointer transition-colors ${
                    isCurrentTrack
                        ? 'bg-[var(--macos-blue)]/10'
                        : ''
                }`}
                onClick={handlePlay}
            >
                {/* Leading Element - Queue Number */}
                {context === 'queue' && queueIndex !== undefined && (
                    <div className="w-6 flex-shrink-0 text-center">
                        <span className="text-[12px] text-muted-foreground">{queueIndex + 1}</span>
                    </div>
                )}

                {/* Thumbnail - 48x48px with 4px radius */}
                <div className="w-12 h-12 rounded flex-shrink-0 bg-secondary overflow-hidden">
                    {videoInfo.thumbnail_url ? (
                        <img
                            src={videoInfo.thumbnail_url}
                            alt={videoInfo.title}
                            className="w-full h-full object-cover"
                        />
                    ) : (
                        <div className="w-full h-full flex items-center justify-center">
                            <Music className="w-5 h-5 text-muted-foreground" />
                        </div>
                    )}
                </div>

                {/* Track Info */}
                <div className="flex-1 min-w-0 overflow-hidden">
                    <div
                        className={`text-[15px] font-semibold truncate ${
                            isCurrentTrack
                                ? 'text-[var(--macos-blue)]'
                                : 'text-foreground'
                        }`}
                    >
                        {videoInfo.title}
                    </div>
                    <div className="flex items-center gap-1.5 text-[12px] text-muted-foreground">
                        <span className="truncate">{videoInfo.uploader}</span>
                        {videoInfo.duration && (
                            <>
                                <span>•</span>
                                <span className="flex-shrink-0 text-[11px]">{formatDuration(videoInfo.duration)}</span>
                            </>
                        )}
                    </div>
                </div>

                {/* Action Buttons - Always visible, 20px icons */}
                <div className="flex items-center gap-2 flex-shrink-0">
                    {/* Play/Pause Button */}
                    <button
                        onClick={(e) => {
                            e.stopPropagation()
                            handlePlay()
                        }}
                        className="w-6 h-6 flex items-center justify-center hover-macos-button rounded"
                        title={isCurrentTrack && isPlaying ? 'Pause' : 'Play'}
                    >
                        {isLoading ? (
                            <Loader2 className="w-5 h-5 text-foreground animate-spin" />
                        ) : isCurrentTrack && isPlaying ? (
                            <Pause className="w-5 h-5 text-[var(--macos-blue)] fill-[var(--macos-blue)]" />
                        ) : (
                            <Play className="w-5 h-5 text-foreground fill-foreground" />
                        )}
                    </button>

                    {/* Note: "Add to Queue" button removed - queue is only populated via "Play All" in playlists */}

                    {/* Favorite Toggle - All contexts except playlist */}
                    {context !== 'playlist' && (
                        <button
                            onClick={handleToggleFavorite}
                            className="w-6 h-6 flex items-center justify-center hover-macos-button rounded group/heart"
                            title={isFavorite ? 'Remove from Favorites' : 'Add to Favorites'}
                        >
                            {isFavorite ? (
                                <Heart className="w-5 h-5 text-macos-red fill-[var(--macos-red)]" />
                            ) : (
                                <Heart className="w-5 h-5 text-foreground" />
                            )}
                        </button>
                    )}

                    {/* Remove Button - Queue and Playlist context */}
                    {(context === 'queue' || context === 'playlist') && onRemove && (
                        <button
                            onClick={(e) => {
                                e.stopPropagation()
                                onRemove()
                            }}
                            className="w-6 h-6 flex items-center justify-center hover-macos-button rounded"
                            title="Remove"
                        >
                            <Trash className="w-5 h-5 text-macos-red" />
                        </button>
                    )}
                </div>
            </div>

            {/* Playlist Selection Modal */}
            {showPlaylistModal && (
                <PlaylistSelectionModal
                    track={videoInfo}
                    onClose={() => setShowPlaylistModal(false)}
                />
            )}
        </>
    )
}
