import { useState, useEffect } from 'react'
import { Play, Pause, Heart, Trash, Loader2, Music, Download } from 'lucide-react'
import { playTrack, togglePlayPause, downloadTrack, isTrackDownloaded, getActiveDownloads, type YTVideoInfo, type Track, formatDuration } from '@/lib/tauri'
import { PlaylistSelectionModal } from '@/features/playlists/playlist-selection-modal'
import { usePlayerStore } from '@/stores/player-store'

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
    queueIndex,
    isFavorite,
    onRemove,
    onToggleFavorite
}: TrackItemProps) {
    const [showPlaylistModal, setShowPlaylistModal] = useState(false)
    const [isDownloaded, setIsDownloaded] = useState(false)
    const [isDownloading, setIsDownloading] = useState(false)
    const [downloadProgress, setDownloadProgress] = useState<number>(0)

    // Convert Track to YTVideoInfo format
    const videoInfo: YTVideoInfo = 'uploader' in track ? track : {
        ...track,
        uploader: track.author || 'Unknown',
        audio_url: null,
        description: null
    }

    // Use Zustand store for player state
    const { loadingTrackId, currentTrack, isPlaying: globalIsPlaying } = usePlayerStore()
    const isThisTrackLoading = loadingTrackId === videoInfo.id
    const isThisTrackPlaying = currentTrack?.id === videoInfo.id && globalIsPlaying

    // Check if track is downloaded or downloading on mount
    useEffect(() => {
        const checkStatus = async () => {
            try {
                // Check if downloaded
                const downloaded = await isTrackDownloaded(videoInfo.id)
                setIsDownloaded(downloaded)

                // Check if currently downloading
                if (!downloaded) {
                    const activeDownloads = await getActiveDownloads()
                    const thisDownload = activeDownloads.find(d => d.video_id === videoInfo.id)
                    if (thisDownload) {
                        setIsDownloading(true)
                        setDownloadProgress(thisDownload.progress)
                    }
                }
            } catch (error) {
                console.error('Failed to check download status:', error)
            }
        }
        checkStatus()

        // Periodically check download status
        const interval = setInterval(checkStatus, 3000)
        return () => clearInterval(interval)
    }, [videoInfo.id])

    // Poll for download progress when downloading
    useEffect(() => {
        if (!isDownloading) return

        const checkProgress = async () => {
            try {
                const activeDownloads = await getActiveDownloads()
                const thisDownload = activeDownloads.find(d => d.video_id === videoInfo.id)
                if (thisDownload) {
                    setDownloadProgress(thisDownload.progress)
                    // If download completed, update state
                    if (thisDownload.is_completed) {
                        setIsDownloading(false)
                        setIsDownloaded(true)
                    }
                } else {
                    // Download not found in active downloads, check if completed
                    const downloaded = await isTrackDownloaded(videoInfo.id)
                    if (downloaded) {
                        setIsDownloading(false)
                        setIsDownloaded(true)
                    }
                }
            } catch (error) {
                console.error('Failed to check download progress:', error)
            }
        }

        checkProgress()
        const interval = setInterval(checkProgress, 500) // Check every 500ms for smooth progress
        return () => clearInterval(interval)
    }, [isDownloading, videoInfo.id])

    const handlePlay = async () => {
        // Don't allow clicking play if track is currently loading
        if (isThisTrackLoading) return

        try {
            // If this is the current track (playing or paused), toggle play/pause
            // Otherwise, play the new track
            if (currentTrack?.id === videoInfo.id) {
                await togglePlayPause()
            } else {
                // Play track directly WITHOUT adding to queue
                // Queue is only populated via "Play All" on playlists
                await playTrack(videoInfo)
            }
        } catch (error) {
            console.error('Failed to play track:', error)
        }
    }

    // Removed: handleAddToQueue - tracks are no longer manually added to queue
    // Queue is only populated by "Play All" playlist action

    const handleToggleFavorite = (e: React.MouseEvent) => {
        e.stopPropagation()
        if (onToggleFavorite) {
            onToggleFavorite()
        } else {
            // Fallback to add to playlist modal
            setShowPlaylistModal(true)
        }
    }

    const handleDownload = async (e: React.MouseEvent) => {
        e.stopPropagation()
        if (isDownloaded || isDownloading) return

        setIsDownloading(true)
        try {
            await downloadTrack(videoInfo)
            // Don't set isDownloading to false here - let the progress polling handle it
        } catch (error) {
            console.error('Failed to download track:', error)
            setIsDownloading(false)
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
                            <Music className="w-6 h-6 text-muted-foreground" />
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

                {/* Action Buttons - Always visible, 16px icons */}
                <div className="flex items-center gap-2 flex-shrink-0">
                    {/* Play/Pause Button */}
                    <button
                        onClick={(e) => {
                            e.stopPropagation()
                            handlePlay()
                        }}
                        disabled={isThisTrackLoading}
                        className="w-6 h-6 flex items-center justify-center hover-macos-button rounded disabled:cursor-not-allowed disabled:opacity-70"
                        title={isThisTrackLoading ? 'Loading...' : isThisTrackPlaying ? 'Playing' : 'Play'}
                    >
                        {isThisTrackLoading ? (
                            <Loader2 className="w-4 h-4 text-foreground animate-spin" />
                        ) : isThisTrackPlaying ? (
                            <Pause className="w-4 h-4 text-[var(--macos-blue)] fill-[var(--macos-blue)]" />
                        ) : (
                            <Play className="w-4 h-4 text-foreground fill-foreground" />
                        )}
                    </button>

                    {/* Note: "Add to Queue" button removed - queue is only populated via "Play All" in playlists */}

                    {/* Download Button - Only show if not already downloaded */}
                    {context !== 'queue' && !isDownloaded && (
                        <button
                            onClick={handleDownload}
                            className="w-6 h-6 flex items-center justify-center hover-macos-button rounded relative"
                            title={isDownloading ? `Downloading ${Math.round(downloadProgress * 100)}%` : "Download"}
                            disabled={isDownloading}
                        >
                            {isDownloading ? (
                                <div className="relative w-6 h-6 flex items-center justify-center">
                                    {/* Background circle */}
                                    <svg className="absolute w-6 h-6 -rotate-90">
                                        <circle
                                            cx="12"
                                            cy="12"
                                            r="10"
                                            stroke="currentColor"
                                            strokeWidth="2"
                                            fill="none"
                                            className="text-muted-foreground/30"
                                        />
                                        <circle
                                            cx="12"
                                            cy="12"
                                            r="10"
                                            stroke="currentColor"
                                            strokeWidth="2"
                                            fill="none"
                                            strokeDasharray={`${2 * Math.PI * 10}`}
                                            strokeDashoffset={`${2 * Math.PI * 10 * (1 - downloadProgress)}`}
                                            className="text-[var(--macos-blue)] transition-all duration-300"
                                            strokeLinecap="round"
                                        />
                                    </svg>
                                    {/* Percentage text */}
                                    <span className="text-[8px] font-bold text-[var(--macos-blue)]">
                                        {Math.round(downloadProgress * 100)}
                                    </span>
                                </div>
                            ) : (
                                <Download className="w-4 h-4 text-foreground" />
                            )}
                        </button>
                    )}

                    {/* Favorite Toggle - All contexts except playlist */}
                    {context !== 'playlist' && (
                        <button
                            onClick={handleToggleFavorite}
                            className="w-6 h-6 flex items-center justify-center hover-macos-button rounded group/heart"
                            title={isFavorite ? 'Remove from Favorites' : 'Add to Favorites'}
                        >
                            {isFavorite ? (
                                <Heart className="w-4 h-4 text-macos-red fill-[var(--macos-red)]" />
                            ) : (
                                <Heart className="w-4 h-4 text-foreground" />
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
                            <Trash className="w-4 h-4 text-macos-red" />
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
