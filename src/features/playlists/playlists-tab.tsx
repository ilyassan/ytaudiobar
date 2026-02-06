import { useState, useEffect } from 'react'
import { Plus, Heart, Music, ArrowLeft, Play, ChevronRight } from 'lucide-react'
import { getAllPlaylists, getPlaylistTracks, createPlaylist, removeTrackFromPlaylist, playPlaylist, type Playlist, type Track } from '@/lib/tauri'
import { TrackItem } from '@/components/track-item'
import { TabHeader } from '@/components/tab-header'

export function PlaylistsTab() {
    const [playlists, setPlaylists] = useState<Playlist[]>([])
    const [selectedPlaylist, setSelectedPlaylist] = useState<Playlist | null>(null)
    const [playlistTracks, setPlaylistTracks] = useState<Track[]>([])
    const [showCreateModal, setShowCreateModal] = useState(false)
    const [newPlaylistName, setNewPlaylistName] = useState('')
    const [isLoading, setIsLoading] = useState(true)
    const [isLoadingTracks, setIsLoadingTracks] = useState(false)

    const loadPlaylists = async () => {
        try {
            setIsLoading(true)
            const data = await getAllPlaylists()
            // Load track counts
            const playlistsWithCounts = await Promise.all(
                data.map(async (playlist) => {
                    const tracks = await getPlaylistTracks(playlist.id)
                    return { ...playlist, trackCount: tracks.length }
                })
            )
            setPlaylists(playlistsWithCounts as any)
        } catch (error) {
            console.error('Failed to load playlists:', error)
        } finally {
            setIsLoading(false)
        }
    }

    useEffect(() => {
        loadPlaylists()
    }, [])

    const handleSelectPlaylist = async (playlist: Playlist) => {
        setSelectedPlaylist(playlist)
        setIsLoadingTracks(true)
        try {
            const tracks = await getPlaylistTracks(playlist.id)
            setPlaylistTracks(tracks)
        } catch (error) {
            console.error('Failed to load playlist tracks:', error)
        } finally {
            setIsLoadingTracks(false)
        }
    }

    const handleBackToPlaylists = () => {
        setSelectedPlaylist(null)
        setPlaylistTracks([])
        loadPlaylists()
    }

    const handleCreatePlaylist = async () => {
        if (!newPlaylistName.trim()) return

        try {
            await createPlaylist(newPlaylistName)
            setNewPlaylistName('')
            setShowCreateModal(false)
            await loadPlaylists()
        } catch (error) {
            console.error('Failed to create playlist:', error)
        }
    }

    const handleRemoveTrack = async (trackId: string) => {
        if (!selectedPlaylist) return

        try {
            await removeTrackFromPlaylist(trackId, selectedPlaylist.id)
            const tracks = await getPlaylistTracks(selectedPlaylist.id)
            setPlaylistTracks(tracks)
        } catch (error) {
            console.error('Failed to remove track:', error)
        }
    }

    const handlePlayPlaylist = async () => {
        if (!selectedPlaylist) return

        try {
            await playPlaylist(selectedPlaylist.id)
        } catch (error) {
            console.error('Failed to play playlist:', error)
        }
    }

    // List View
    if (!selectedPlaylist) {
        return (
            <div className="flex flex-col h-full bg-background">
                <TabHeader
                    title="Playlists"
                    actions={
                        <button
                            onClick={() => setShowCreateModal(true)}
                            className="w-8 h-8 flex items-center justify-center rounded-full hover-macos-button"
                            title="Create Playlist"
                        >
                            <Plus className="w-5 h-5 text-[var(--macos-blue)]" />
                        </button>
                    }
                />

                {/* Playlists List */}
                <div className="flex-1 overflow-y-auto py-2">
                    {isLoading ? null : playlists.length === 0 ? (
                        <div className="flex flex-col items-center justify-center h-full text-center px-6">
                            <Heart className="w-12 h-12 text-muted-foreground mb-4 opacity-60" />
                            <h3 className="text-[15px] font-semibold text-foreground mb-2">
                                No playlists yet
                            </h3>
                            <p className="text-[13px] text-muted-foreground max-w-[250px]">
                                Create a playlist to organize your favorite tracks
                            </p>
                        </div>
                    ) : (
                        playlists.map((playlist) => (
                            <button
                                key={playlist.id}
                                onClick={() => handleSelectPlaylist(playlist)}
                                className="w-full flex items-center gap-3 px-4 py-3 hover-macos-button transition-colors"
                            >
                                <div className={`w-10 h-10 rounded-lg flex items-center justify-center flex-shrink-0 ${
                                    playlist.is_system_playlist
                                        ? 'bg-[var(--macos-red)]/10'
                                        : 'bg-[var(--macos-blue)]/10'
                                }`}>
                                    {playlist.is_system_playlist ? (
                                        <Heart className="w-5 h-5 text-macos-red fill-[var(--macos-red)]" />
                                    ) : (
                                        <Music className="w-5 h-5 text-[var(--macos-blue)]" />
                                    )}
                                </div>
                                <div className="flex-1 text-left min-w-0">
                                    <div className="text-[15px] font-semibold text-foreground truncate">
                                        {playlist.name}
                                    </div>
                                    <div className="text-[12px] text-muted-foreground">
                                        {(playlist as any).trackCount || 0} track{(playlist as any).trackCount === 1 ? '' : 's'}
                                    </div>
                                </div>
                                <ChevronRight className="w-5 h-5 text-muted-foreground flex-shrink-0" />
                            </button>
                        ))
                    )}
                </div>

                {/* Create Playlist Modal */}
                {showCreateModal && (
                    <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={() => setShowCreateModal(false)}>
                        <div className="bg-card rounded-xl p-5 w-[280px] shadow-xl" onClick={(e) => e.stopPropagation()}>
                            <h3 className="text-[17px] font-semibold text-foreground mb-4">
                                Create Playlist
                            </h3>
                            <input
                                type="text"
                                value={newPlaylistName}
                                onChange={(e) => setNewPlaylistName(e.target.value)}
                                onKeyPress={(e) => e.key === 'Enter' && handleCreatePlaylist()}
                                placeholder="Playlist name"
                                className="w-full px-3 py-2 bg-secondary border-none rounded-lg text-[14px] text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-[var(--macos-blue)] mb-4"
                                autoFocus
                            />
                            <div className="flex gap-2">
                                <button
                                    onClick={() => setShowCreateModal(false)}
                                    className="flex-1 px-4 py-2 rounded-lg text-[13px] font-medium text-foreground hover-macos-button"
                                >
                                    Cancel
                                </button>
                                <button
                                    onClick={handleCreatePlaylist}
                                    disabled={!newPlaylistName.trim()}
                                    className="flex-1 px-4 py-2 rounded-lg text-[13px] font-medium bg-[var(--macos-blue)] text-white hover:opacity-90 transition-opacity disabled:opacity-50"
                                >
                                    Create
                                </button>
                            </div>
                        </div>
                    </div>
                )}
            </div>
        )
    }

    // Detail View
    return (
        <div className="flex flex-col h-full bg-background">
            {/* Header */}
            <div className="px-4 py-3 border-b border-macos-separator bg-card">
                <div className="flex items-center gap-3">
                    <button
                        onClick={handleBackToPlaylists}
                        className="w-8 h-8 flex items-center justify-center rounded-full hover-macos-button"
                        aria-label="Back to playlists"
                    >
                        <ArrowLeft className="w-5 h-5 text-foreground" />
                    </button>
                    <div className="flex-1 min-w-0">
                        <h2 className="text-[17px] font-semibold text-foreground truncate">
                            {selectedPlaylist.name}
                        </h2>
                        <div className="text-[12px] text-muted-foreground">
                            {playlistTracks.length} track{playlistTracks.length === 1 ? '' : 's'}
                        </div>
                    </div>
                    {playlistTracks.length > 0 && (
                        <button
                            onClick={handlePlayPlaylist}
                            className="flex items-center gap-2 px-3 py-1.5 rounded-full bg-[var(--macos-blue)] text-white hover:opacity-90 transition-opacity"
                        >
                            <Play className="w-3.5 h-3.5 fill-white" />
                            <span className="text-[12px] font-medium">Play All</span>
                        </button>
                    )}
                </div>
            </div>

            {/* Tracks */}
            <div className="flex-1 overflow-y-auto">
                {isLoadingTracks ? null : playlistTracks.length === 0 ? (
                    <div className="flex flex-col items-center justify-center h-full text-center px-6">
                        <Music className="w-12 h-12 text-muted-foreground mb-4 opacity-60" />
                        <h3 className="text-[15px] font-semibold text-foreground mb-2">
                            Empty Playlist
                        </h3>
                        <p className="text-[13px] text-muted-foreground max-w-[250px]">
                            Add tracks by using the heart button when searching
                        </p>
                    </div>
                ) : (
                    <div className="py-2">
                        {playlistTracks.map((track) => (
                            <TrackItem
                                key={track.id}
                                track={track}
                                context="playlist"
                                onRemove={() => handleRemoveTrack(track.id)}
                            />
                        ))}
                    </div>
                )}
            </div>
        </div>
    )
}
