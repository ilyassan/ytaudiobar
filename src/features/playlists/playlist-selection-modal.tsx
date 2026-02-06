import { useState, useEffect } from 'react'
import { X, Plus, Check, Heart, Music } from 'lucide-react'
import { getAllPlaylists, getPlaylistTracks, addTrackToPlaylist, createPlaylist, type Playlist, type YTVideoInfo } from '@/lib/tauri'

interface PlaylistWithData extends Playlist {
    trackCount: number
    hasTrack: boolean
}

interface PlaylistSelectionModalProps {
    track: YTVideoInfo
    onClose: () => void
}

export function PlaylistSelectionModal({ track, onClose }: PlaylistSelectionModalProps) {
    const [playlists, setPlaylists] = useState<PlaylistWithData[]>([])
    const [showCreateModal, setShowCreateModal] = useState(false)
    const [newPlaylistName, setNewPlaylistName] = useState('')

    const loadPlaylists = async () => {
        try {
            const allPlaylists = await getAllPlaylists()

            // Load track data for each playlist
            const playlistsWithData = await Promise.all(
                allPlaylists.map(async (playlist) => {
                    const tracks = await getPlaylistTracks(playlist.id)
                    return {
                        ...playlist,
                        trackCount: tracks.length,
                        hasTrack: tracks.some(t => t.id === track.id)
                    }
                })
            )

            setPlaylists(playlistsWithData)
        } catch (error) {
            console.error('Failed to load playlists:', error)
        }
    }

    useEffect(() => {
        loadPlaylists()
    }, [])

    const handleAddToPlaylist = async (playlistId: string) => {
        try {
            await addTrackToPlaylist(track, playlistId)
            await loadPlaylists()
        } catch (error) {
            console.error('Failed to add track to playlist:', error)
        }
    }

    const handleCreateAndAdd = async () => {
        if (!newPlaylistName.trim()) return

        try {
            const playlistId = await createPlaylist(newPlaylistName)
            await addTrackToPlaylist(track, playlistId)
            setNewPlaylistName('')
            setShowCreateModal(false)
            await loadPlaylists()
        } catch (error) {
            console.error('Failed to create playlist:', error)
        }
    }

    return (
        <div className="fixed inset-0 bg-black/50 flex items-center justify-center z-50" onClick={onClose}>
            <div className="bg-card rounded-xl w-[300px] max-h-[400px] flex flex-col" onClick={(e) => e.stopPropagation()}>
                {/* Header */}
                <div className="flex items-center justify-between px-4 py-3 border-b border-macos-separator">
                    <div className="flex-1 min-w-0 pr-2">
                        <h3 className="text-[13px] font-semibold text-foreground truncate">
                            {track.title}
                        </h3>
                        <p className="text-[11px] text-muted-foreground truncate">Add to playlist</p>
                    </div>
                    <button
                        onClick={onClose}
                        className="w-8 h-8 flex items-center justify-center rounded-full hover-macos-button transition-colors flex-shrink-0"
                    >
                        <X className="w-4 h-4 text-muted-foreground" />
                    </button>
                </div>

                {/* Playlists List */}
                <div className="flex-1 overflow-y-auto py-2">
                    {playlists.map((playlist) => (
                        <button
                            key={playlist.id}
                            onClick={() => !playlist.hasTrack && handleAddToPlaylist(playlist.id)}
                            disabled={playlist.hasTrack}
                            className={`w-full flex items-center gap-3 px-4 py-2.5 transition-colors ${
                                playlist.hasTrack
                                    ? 'opacity-60 cursor-default'
                                    : 'hover-macos-button cursor-pointer'
                            }`}
                        >
                            <div className={`w-8 h-8 rounded-lg flex items-center justify-center flex-shrink-0 ${
                                playlist.is_system_playlist
                                    ? 'bg-macos-red/10 text-macos-red'
                                    : 'bg-[var(--macos-blue)]/10 text-[var(--macos-blue)]'
                            }`}>
                                {playlist.is_system_playlist ? (
                                    <Heart className="w-4 h-4" />
                                ) : (
                                    <Music className="w-4 h-4" />
                                )}
                            </div>
                            <div className="flex-1 text-left min-w-0">
                                <div className="text-[13px] font-medium text-foreground truncate">
                                    {playlist.name}
                                </div>
                                <div className="text-[11px] text-muted-foreground">
                                    {playlist.trackCount} track{playlist.trackCount === 1 ? '' : 's'}
                                </div>
                            </div>
                            {playlist.hasTrack ? (
                                <div className="flex items-center gap-1 text-[var(--macos-blue)] flex-shrink-0">
                                    <Check className="w-4 h-4" />
                                    <span className="text-[11px] font-medium">Added</span>
                                </div>
                            ) : (
                                <Plus className="w-5 h-5 text-muted-foreground flex-shrink-0" />
                            )}
                        </button>
                    ))}
                </div>

                {/* Create Playlist Button */}
                <div className="px-4 py-3 border-t border-macos-separator">
                    <button
                        onClick={() => setShowCreateModal(true)}
                        className="w-full flex items-center justify-center gap-2 px-4 py-2 rounded-lg bg-[var(--macos-blue)] text-white hover:opacity-90 transition-opacity"
                    >
                        <Plus className="w-4 h-4" />
                        <span className="text-[13px] font-medium">Create New Playlist</span>
                    </button>
                </div>

                {/* Create Playlist Modal */}
                {showCreateModal && (
                    <div className="absolute inset-0 bg-black/50 rounded-xl flex items-center justify-center p-4" onClick={() => setShowCreateModal(false)}>
                        <div className="bg-card rounded-xl p-5 w-full" onClick={(e) => e.stopPropagation()}>
                            <h3 className="text-[15px] font-semibold text-foreground mb-4">
                                Create Playlist
                            </h3>
                            <input
                                type="text"
                                value={newPlaylistName}
                                onChange={(e) => setNewPlaylistName(e.target.value)}
                                onKeyPress={(e) => e.key === 'Enter' && handleCreateAndAdd()}
                                placeholder="Playlist name"
                                className="w-full px-3 py-2 bg-secondary border-none rounded-lg text-[13px] text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-[var(--macos-blue)] mb-4"
                                autoFocus
                            />
                            <div className="flex gap-2">
                                <button
                                    onClick={() => setShowCreateModal(false)}
                                    className="flex-1 px-4 py-2 rounded-lg text-[13px] font-medium text-[var(--macos-blue)] hover-macos-button transition-colors"
                                >
                                    Cancel
                                </button>
                                <button
                                    onClick={handleCreateAndAdd}
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
        </div>
    )
}
