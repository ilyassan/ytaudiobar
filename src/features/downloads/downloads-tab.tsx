import { useState, useEffect } from 'react'
import { Download, X, CheckSquare, Square } from 'lucide-react'
import {
    getActiveDownloads,
    getDownloadedTracks,
    getStorageUsed,
    deleteDownload,
    cancelDownload,
    listenToDownloadsUpdate,
    type DownloadProgress,
    type DownloadedTrack
} from '@/lib/tauri'
import { TrackItem } from '@/components/track-item'
import { TabHeader } from '@/components/tab-header'

export function DownloadsTab() {
    const [activeDownloads, setActiveDownloads] = useState<DownloadProgress[]>([])
    const [downloadedTracks, setDownloadedTracks] = useState<DownloadedTrack[]>([])
    const [storageUsed, setStorageUsed] = useState<number>(0)
    const [isSelectionMode, setIsSelectionMode] = useState(false)
    const [selectedTracks, setSelectedTracks] = useState<Set<string>>(new Set())

    const loadDownloads = async () => {
        try {
            const [active, downloaded, storage] = await Promise.all([
                getActiveDownloads(),
                getDownloadedTracks(),
                getStorageUsed()
            ])
            setActiveDownloads(active)
            setDownloadedTracks(downloaded)
            setStorageUsed(storage)
        } catch (error) {
            console.error('Failed to load downloads:', error)
        }
    }

    useEffect(() => {
        loadDownloads()

        // Set up interval to refresh downloads
        const interval = setInterval(loadDownloads, 2000)

        // Listen for download updates
        const unlisten = listenToDownloadsUpdate(() => {
            loadDownloads()
        })

        return () => {
            clearInterval(interval)
            unlisten.then(fn => fn())
        }
    }, [])

    const handleCancelDownload = async (videoId: string) => {
        try {
            await cancelDownload(videoId)
            await loadDownloads()
        } catch (error) {
            console.error('Failed to cancel download:', error)
        }
    }

    const handleDeleteDownload = async (videoId: string) => {
        try {
            await deleteDownload(videoId)
            await loadDownloads()
        } catch (error) {
            console.error('Failed to delete download:', error)
        }
    }

    const handleToggleSelection = (videoId: string) => {
        const newSelected = new Set(selectedTracks)
        if (newSelected.has(videoId)) {
            newSelected.delete(videoId)
        } else {
            newSelected.add(videoId)
        }
        setSelectedTracks(newSelected)
    }

    const handleDeleteSelected = async () => {
        try {
            await Promise.all(
                Array.from(selectedTracks).map(id => deleteDownload(id))
            )
            setSelectedTracks(new Set())
            setIsSelectionMode(false)
            await loadDownloads()
        } catch (error) {
            console.error('Failed to delete selected tracks:', error)
        }
    }

    const formatBytes = (bytes: number): string => {
        if (bytes === 0) return '0 Bytes'
        const k = 1024
        const sizes = ['Bytes', 'KB', 'MB', 'GB']
        const i = Math.floor(Math.log(bytes) / Math.log(k))
        return Math.round((bytes / Math.pow(k, i)) * 100) / 100 + ' ' + sizes[i]
    }

    const calculateSelectedSize = (): number => {
        return downloadedTracks
            .filter(t => selectedTracks.has(t.video_info.id))
            .reduce((sum, t) => sum + t.file_size, 0)
    }

    const hasDownloads = activeDownloads.length > 0 || downloadedTracks.length > 0

    return (
        <div className="flex flex-col h-full bg-background">
            <TabHeader
                title="Downloads"
                subtitle={
                    isSelectionMode && selectedTracks.size > 0
                        ? `${selectedTracks.size} selected • ${formatBytes(calculateSelectedSize())}`
                        : hasDownloads
                        ? `Storage used: ${formatBytes(storageUsed)}`
                        : undefined
                }
                actions={
                    downloadedTracks.length > 0 ? (
                        <div className="flex items-center gap-2">
                            {isSelectionMode ? (
                                <>
                                    <button
                                        onClick={() => {
                                            setIsSelectionMode(false)
                                            setSelectedTracks(new Set())
                                        }}
                                        className="text-[13px] text-muted-foreground hover:text-foreground"
                                    >
                                        Cancel
                                    </button>
                                    <button
                                        onClick={handleDeleteSelected}
                                        disabled={selectedTracks.size === 0}
                                        className={`text-[13px] ${
                                            selectedTracks.size === 0
                                                ? 'text-muted-foreground cursor-not-allowed opacity-50'
                                                : 'text-macos-red hover:opacity-80'
                                        }`}
                                    >
                                        Delete Selected
                                    </button>
                                </>
                            ) : (
                                <button
                                    onClick={() => setIsSelectionMode(true)}
                                    className="text-[13px] text-[var(--macos-blue)] hover:opacity-80"
                                >
                                    Select
                                </button>
                            )}
                        </div>
                    ) : undefined
                }
            />

            {/* Downloads Content */}
            <div className="flex-1 overflow-y-auto">
                {!hasDownloads ? (
                    <div className="flex flex-col items-center justify-center h-full text-center px-6">
                        <Download className="w-12 h-12 text-muted-foreground mb-4 opacity-60" />
                        <h3 className="text-[15px] font-semibold text-foreground mb-2">
                            No Downloads
                        </h3>
                        <p className="text-[13px] text-muted-foreground max-w-[250px]">
                            Downloaded tracks will appear here. Use the download button on any track to start downloading.
                        </p>
                    </div>
                ) : (
                    <div className="py-2">
                        {/* Active Downloads Section */}
                        {activeDownloads.length > 0 && (
                            <>
                                <div className="px-3 py-2">
                                    <h3 className="text-[13px] font-semibold text-foreground">
                                        Downloading ({activeDownloads.length})
                                    </h3>
                                </div>
                                <div className="space-y-1">
                                    {activeDownloads.map((download) => (
                                        <ActiveDownloadRow
                                            key={download.video_id}
                                            download={download}
                                            onCancel={handleCancelDownload}
                                        />
                                    ))}
                                </div>
                            </>
                        )}

                        {/* Downloaded Tracks Section */}
                        {downloadedTracks.length > 0 && (
                            <>
                                <div className="px-3 py-2 mt-4">
                                    <h3 className="text-[13px] font-semibold text-foreground">
                                        Downloaded ({downloadedTracks.length})
                                    </h3>
                                </div>
                                <div>
                                    {downloadedTracks.map((track) => (
                                        <div
                                            key={track.video_info.id}
                                            className="relative group flex items-center gap-2"
                                        >
                                            {isSelectionMode && (
                                                <button
                                                    onClick={() => handleToggleSelection(track.video_info.id)}
                                                    className="pl-2 cursor-pointer"
                                                >
                                                    {selectedTracks.has(track.video_info.id) ? (
                                                        <CheckSquare className="w-5 h-5 text-[var(--macos-blue)]" />
                                                    ) : (
                                                        <Square className="w-5 h-5 text-muted-foreground" />
                                                    )}
                                                </button>
                                            )}
                                            <div className="flex-1 min-w-0">
                                                <TrackItem
                                                    track={track.video_info}
                                                    context="search"
                                                    onRemove={
                                                        !isSelectionMode
                                                            ? () => handleDeleteDownload(track.video_info.id)
                                                            : undefined
                                                    }
                                                />
                                            </div>
                                        </div>
                                    ))}
                                </div>
                            </>
                        )}
                    </div>
                )}
            </div>
        </div>
    )
}

interface ActiveDownloadRowProps {
    download: DownloadProgress
    onCancel: (videoId: string) => void
}

function ActiveDownloadRow({ download, onCancel }: ActiveDownloadRowProps) {
    const percentage = Math.round(download.progress * 100)

    return (
        <div className="flex items-center gap-3 px-3 py-2 hover-macos-button">
            {/* Progress Circle */}
            <div className="relative w-8 h-8 flex-shrink-0">
                <svg className="w-8 h-8 -rotate-90">
                    <circle
                        cx="16"
                        cy="16"
                        r="14"
                        stroke="currentColor"
                        strokeWidth="3"
                        fill="none"
                        className="text-muted-foreground/30"
                    />
                    <circle
                        cx="16"
                        cy="16"
                        r="14"
                        stroke="currentColor"
                        strokeWidth="3"
                        fill="none"
                        strokeDasharray={`${2 * Math.PI * 14}`}
                        strokeDashoffset={`${2 * Math.PI * 14 * (1 - download.progress)}`}
                        className="text-[var(--macos-blue)] transition-all duration-300"
                        strokeLinecap="round"
                    />
                </svg>
                <div className="absolute inset-0 flex items-center justify-center">
                    <span className="text-[11px] font-semibold text-[var(--macos-blue)]">
                        {percentage}
                    </span>
                </div>
            </div>

            {/* Download Info */}
            <div className="flex-1 min-w-0">
                <div className="text-[13px] font-semibold text-foreground truncate">
                    Downloading...
                </div>
                <div className="flex items-center gap-1.5 text-[11px] text-muted-foreground">
                    {download.speed && <span>{download.speed}</span>}
                    {download.file_size && (
                        <>
                            <span>•</span>
                            <span>{download.file_size}</span>
                        </>
                    )}
                    {download.eta && (
                        <>
                            <span>•</span>
                            <span>ETA: {download.eta}</span>
                        </>
                    )}
                </div>
                {download.error && (
                    <div className="text-[11px] text-macos-red mt-1">
                        Error: {download.error}
                    </div>
                )}
            </div>

            {/* Cancel Button */}
            <button
                onClick={() => onCancel(download.video_id)}
                className="w-6 h-6 flex items-center justify-center hover-macos-button rounded"
                title="Cancel download"
            >
                <X className="w-5 h-5 text-macos-red" />
            </button>
        </div>
    )
}
