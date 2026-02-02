import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

// ===== TYPES =====
export interface YTVideoInfo {
    id: string
    title: string
    uploader: string
    duration: number
    thumbnail_url: string | null
    audio_url: string | null
    description: string | null
}

export interface Track {
    id: string
    title: string
    author: string | null
    duration: number
    thumbnail_url: string | null
    added_date: number
    file_path: string | null
}

export interface Playlist {
    id: string
    name: string
    created_date: number
    is_system_playlist: boolean
}

export interface AudioState {
    is_playing: boolean
    is_loading: boolean
    current_position: number
    duration: number
    playback_rate: number
    current_track: YTVideoInfo | null
}

export type RepeatMode = 'Off' | 'All' | 'One'

// ===== TAURI COMMANDS =====

// Search
export const searchYoutube = (query: string, musicMode: boolean) =>
    invoke<YTVideoInfo[]>('search_youtube', { query, musicMode })

// YTDLP
export const checkYtdlpInstalled = () => invoke<boolean>('check_ytdlp_installed')
export const installYtdlp = () => invoke<void>('install_ytdlp')
export const getYtdlpVersion = () => invoke<string>('get_ytdlp_version')

// Playback
export const playTrack = (track: YTVideoInfo) => invoke<void>('play_track', { track })
export const togglePlayPause = () => invoke<void>('toggle_play_pause')
export const pausePlayback = () => invoke<void>('pause_playback')
export const stopPlayback = () => invoke<void>('stop_playback')
export const seekTo = (position: number) => invoke<void>('seek_to', { position })
export const setVolume = (volume: number) => invoke<void>('set_volume', { volume })
export const setPlaybackSpeed = (rate: number) => invoke<void>('set_playback_speed', { rate })
export const playNext = () => invoke<YTVideoInfo | null>('play_next')
export const playPrevious = () => invoke<YTVideoInfo | null>('play_previous')
export const getAudioState = () => invoke<AudioState>('get_audio_state')

// Queue
export const addToQueue = (track: YTVideoInfo) => invoke<void>('add_to_queue', { track })
export const getQueue = () => invoke<YTVideoInfo[]>('get_queue')
export const clearQueue = () => invoke<void>('clear_queue')
export const toggleShuffle = () => invoke<boolean>('toggle_shuffle')
export const cycleRepeatMode = () => invoke<RepeatMode>('cycle_repeat_mode')
export const getQueueInfo = () => invoke<string>('get_queue_info')
export const reorderQueue = (newQueue: YTVideoInfo[]) => invoke<void>('reorder_queue', { newQueue })

// Playlists
export const getAllPlaylists = () => invoke<Playlist[]>('get_all_playlists')
export const createPlaylist = (name: string) => invoke<string>('create_playlist', { name })
export const deletePlaylist = (id: string) => invoke<void>('delete_playlist', { id })
export const getPlaylistTracks = (playlistId: string) =>
    invoke<Track[]>('get_playlist_tracks', { playlistId })
export const addTrackToPlaylist = (track: YTVideoInfo, playlistId: string) =>
    invoke<void>('add_track_to_playlist', { track, playlistId })
export const removeTrackFromPlaylist = (trackId: string, playlistId: string) =>
    invoke<void>('remove_track_from_playlist', { trackId, playlistId })
export const addToFavorites = (track: YTVideoInfo) =>
    invoke<void>('add_to_favorites', { track })
export const removeFromFavorites = (trackId: string) =>
    invoke<void>('remove_from_favorites', { trackId })
export const playPlaylist = (playlistId: string) =>
    invoke<void>('play_playlist', { playlistId })

// ===== EVENTS =====
export const listenToPlaybackState = (
    callback: (state: AudioState) => void
) => {
    return listen<AudioState>('playback-state-changed', (event) => {
        callback(event.payload)
    })
}

// ===== UTILITIES =====
export const formatDuration = (seconds: number | null): string => {
    if (!seconds || seconds <= 0) return '0:00'
    const mins = Math.floor(seconds / 60)
    const secs = Math.floor(seconds % 60)
    return `${mins}:${secs.toString().padStart(2, '0')}`
}

export const formatTime = (seconds: number): string => {
    const mins = Math.floor(seconds / 60)
    const secs = Math.floor(seconds % 60)
    return `${mins}:${secs.toString().padStart(2, '0')}`
}
