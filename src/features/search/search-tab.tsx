import { Search, ListMusic } from 'lucide-react'
import { type YTVideoInfo, type YTPlaylistInfo } from '@/lib/tauri'
import { TrackItem } from '@/components/track-item'
import { useFavoritesStore } from '@/stores/favorites-store'

interface SearchTabProps {
    query: string
    isPlaylistMode: boolean
    results: YTVideoInfo[]
    playlistResults: YTPlaylistInfo[]
    isSearching: boolean
    onSelectPlaylist: (playlist: YTPlaylistInfo) => void
}

function PlaylistResultRow({
    playlist,
    onClick
}: {
    playlist: YTPlaylistInfo
    onClick: () => void
}) {
    return (
        <button
            onClick={onClick}
            className="w-full flex items-center gap-3 px-3 py-2 hover-macos-button transition-colors text-left"
        >
            <div className="relative w-12 h-12 rounded flex-shrink-0 bg-secondary overflow-hidden">
                {playlist.thumbnail_url ? (
                    <img
                        src={playlist.thumbnail_url}
                        alt={playlist.title}
                        className="w-full h-full object-cover"
                    />
                ) : (
                    <div className="w-full h-full flex items-center justify-center">
                        <ListMusic className="w-6 h-6 text-muted-foreground" />
                    </div>
                )}
                <div className="absolute bottom-0.5 right-0.5 w-5 h-5 rounded-full bg-black/70 flex items-center justify-center">
                    <ListMusic className="w-3 h-3 text-white" />
                </div>
            </div>
            <div className="flex-1 min-w-0 overflow-hidden">
                <div className="text-[15px] font-semibold text-foreground truncate">
                    {playlist.title}
                </div>
                <div className="text-[12px] text-muted-foreground">
                    Playlist
                </div>
            </div>
        </button>
    )
}

export function SearchTab({
    query,
    isPlaylistMode,
    results,
    playlistResults,
    isSearching,
    onSelectPlaylist
}: SearchTabProps) {
    const favoriteTrackIds = useFavoritesStore((s) => s.favoriteTrackIds)

    return (
        <div className="flex flex-col h-full overflow-y-auto bg-background">
            {!query ? (
                <div className="flex flex-col items-center justify-center h-full text-center px-6">
                    {isPlaylistMode ? (
                        <ListMusic className="w-12 h-12 text-muted-foreground mb-4 opacity-60" />
                    ) : (
                        <Search className="w-12 h-12 text-muted-foreground mb-4 opacity-60" />
                    )}
                    <h3 className="text-[15px] font-semibold text-foreground mb-2">
                        {isPlaylistMode
                            ? 'Search YouTube Playlists'
                            : 'Search YouTube'}
                    </h3>
                    <p className="text-[13px] text-muted-foreground max-w-[250px]">
                        {isPlaylistMode
                            ? 'Find playlists to preview, import, or download in full'
                            : 'Find your favorite songs and videos'}
                    </p>
                </div>
            ) : isSearching ? (
                <div className="flex items-center justify-center h-full">
                    <div className="text-[13px] text-muted-foreground">
                        {isPlaylistMode
                            ? `Searching playlists for "${query}"...`
                            : `Searching for "${query}"...`}
                    </div>
                </div>
            ) : isPlaylistMode ? (
                playlistResults.length === 0 ? (
                    <div className="flex items-center justify-center h-full">
                        <div className="text-[13px] text-muted-foreground">
                            No playlists found
                        </div>
                    </div>
                ) : (
                    <div className="py-2">
                        {playlistResults.map((playlist) => (
                            <PlaylistResultRow
                                key={playlist.id}
                                playlist={playlist}
                                onClick={() => onSelectPlaylist(playlist)}
                            />
                        ))}
                    </div>
                )
            ) : results.length === 0 ? (
                <div className="flex items-center justify-center h-full">
                    <div className="text-[13px] text-muted-foreground">
                        No results found
                    </div>
                </div>
            ) : (
                <div className="py-2">
                    {results.map((track) => (
                        <TrackItem
                            key={track.id}
                            track={track}
                            context="search"
                            isFavorite={favoriteTrackIds.has(track.id)}
                        />
                    ))}
                </div>
            )}
        </div>
    )
}
