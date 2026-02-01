// ===== STATE MANAGEMENT =====
const state = {
    currentTrack: null,
    isPlaying: false,
    isMusicMode: false,
    searchText: '',
    playbackSpeed: 1.0,
    currentPosition: 0,
    duration: 0,
    isExpanded: false,
    isInitializing: true,
    searchTimeout: null,
    // Position tracking (mirrors backend approach)
    positionBase: 0,        // Position when playback started/resumed
    positionTimestamp: 0,   // Timestamp when positionBase was set
    // Queue controls
    shuffleMode: false,
    repeatMode: 'Off',      // 'Off', 'All', 'One'
    // Playlists
    playlists: [],
    currentPlaylist: null,
    currentPlaylistTracks: [],
    trackToAdd: null  // Track being added to playlists via modal
};

// Calculate current position based on elapsed time (like backend)
function calculateCurrentPosition() {
    if (!state.isPlaying) {
        return state.positionBase;
    }
    const elapsed = (Date.now() - state.positionTimestamp) / 1000;
    const calculated = state.positionBase + (elapsed * state.playbackSpeed);
    return Math.min(calculated, state.duration); // Don't exceed duration
}

// Sync position from backend - this is the source of truth
function syncPosition(backendPosition) {
    console.log(`syncPosition: backend=${backendPosition.toFixed(1)}, old positionBase=${state.positionBase.toFixed(1)}`);
    state.positionBase = backendPosition;
    state.positionTimestamp = Date.now();
    state.currentPosition = backendPosition;
}

// ===== DOM ELEMENTS =====
const elements = {
    // Search
    searchInput: document.getElementById('searchInput'),
    clearSearch: document.getElementById('clearSearch'),
    musicModeToggle: document.getElementById('musicModeToggle'),
    modeText: document.getElementById('modeText'),
    modeIcon: document.getElementById('modeIcon'),
    emptySearch: document.getElementById('emptySearch'),
    searchResults: document.getElementById('searchResults'),

    // Tabs
    tabButtons: document.querySelectorAll('.tab-btn'),
    tabContents: document.querySelectorAll('.tab-content'),

    // Player
    currentTrack: document.getElementById('currentTrack'),
    trackTitle: document.getElementById('trackTitle'),
    trackArtist: document.getElementById('trackArtist'),
    speakerIcon: document.getElementById('speakerIcon'),
    playPauseBtn: document.getElementById('playPauseBtn'),
    playPauseIcon: document.getElementById('playPauseIcon'),
    prevBtn: document.getElementById('prevBtn'),
    nextBtn: document.getElementById('nextBtn'),
    expandBtn: document.getElementById('expandBtn'),
    collapseBtn: document.getElementById('collapseBtn'),
    playerExpanded: document.getElementById('playerExpanded'),

    // Expanded Player
    expandedTitle: document.getElementById('expandedTitle'),
    expandedArtist: document.getElementById('expandedArtist'),
    expandedPrevBtn: document.getElementById('expandedPrevBtn'),
    expandedPlayPauseBtn: document.getElementById('expandedPlayPauseBtn'),
    expandedPlayPauseIcon: document.getElementById('expandedPlayPauseIcon'),
    expandedNextBtn: document.getElementById('expandedNextBtn'),
    progressBar: document.getElementById('progressBar'),
    currentTime: document.getElementById('currentTime'),
    duration: document.getElementById('duration'),
    speedDown: document.getElementById('speedDown'),
    speedUp: document.getElementById('speedUp'),
    speedText: document.getElementById('speedText'),
    shuffleBtn: document.getElementById('shuffleBtn'),
    repeatBtn: document.getElementById('repeatBtn'),

    // Queue
    clearQueueBtn: document.getElementById('clearQueueBtn'),
    queueControlsRow: document.getElementById('queueControlsRow'),
    queueInfo: document.getElementById('queueInfo'),
    emptyQueue: document.getElementById('emptyQueue'),
    queueList: document.getElementById('queueList'),
    queueShuffleBtn: document.getElementById('queueShuffleBtn'),
    queueRepeatBtn: document.getElementById('queueRepeatBtn'),

    // Playlists
    playlistsContainer: document.getElementById('playlistsContainer'),
    playlistsListView: document.getElementById('playlistsListView'),
    playlistDetailView: document.getElementById('playlistDetailView'),
    createPlaylistBtn: document.getElementById('createPlaylistBtn'),
    backToPlaylistsBtn: document.getElementById('backToPlaylistsBtn'),
    playAllBtn: document.getElementById('playAllBtn'),
    playlistDetailName: document.getElementById('playlistDetailName'),
    playlistDetailCount: document.getElementById('playlistDetailCount'),
    playlistTracksContainer: document.getElementById('playlistTracksContainer'),

    // Modals
    playlistModal: document.getElementById('playlistModal'),
    closePlaylistModal: document.getElementById('closePlaylistModal'),
    modalTrackTitle: document.getElementById('modalTrackTitle'),
    modalPlaylistsList: document.getElementById('modalPlaylistsList'),
    modalCreatePlaylistBtn: document.getElementById('modalCreatePlaylistBtn'),
    createPlaylistModal: document.getElementById('createPlaylistModal'),
    closeCreateModal: document.getElementById('closeCreateModal'),
    newPlaylistNameInput: document.getElementById('newPlaylistNameInput'),
    cancelCreateBtn: document.getElementById('cancelCreateBtn'),
    confirmCreateBtn: document.getElementById('confirmCreateBtn')
};

// ===== INITIALIZATION =====
async function init() {
    setupEventListeners();
    updateMusicModeUI();
    await initializeYTDLP();
}

async function initializeYTDLP() {
    console.log('Starting YTDLP initialization...');

    try {
        // Show initializing message
        showInitializingUI();
        console.log('Initializing UI shown');

        // Check if yt-dlp is installed
        console.log('Checking if yt-dlp is installed...');
        const isInstalled = await window.__TAURI_INVOKE__('check_ytdlp_installed');
        console.log('yt-dlp installed:', isInstalled);

        if (!isInstalled) {
            console.log('yt-dlp not found, downloading...');
            updateInitializingUI('Downloading YouTube engine...');

            // Download yt-dlp
            console.log('Starting download...');
            await window.__TAURI_INVOKE__('install_ytdlp');
            console.log('yt-dlp installed successfully');
        } else {
            console.log('yt-dlp already installed');
        }

        // Get version
        console.log('Getting yt-dlp version...');
        const version = await window.__TAURI_INVOKE__('get_ytdlp_version');
        console.log('yt-dlp version:', version);

        state.isInitializing = false;
        hideInitializingUI();
        console.log('Initialization complete!');
    } catch (error) {
        console.error('Failed to initialize yt-dlp:', error);
        console.error('Error details:', error.message, error.stack);
        state.isInitializing = false;
        showErrorUI(`Failed to initialize: ${error.message || error}`);
    }
}

function showInitializingUI() {
    // Create overlay
    const overlay = document.createElement('div');
    overlay.id = 'initOverlay';
    overlay.style.cssText = `
        position: fixed;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        background: rgba(0, 0, 0, 0.5);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 10000;
    `;

    const message = document.createElement('div');
    message.id = 'initMessage';
    message.style.cssText = `
        background: white;
        padding: 20px 30px;
        border-radius: 10px;
        text-align: center;
        font-size: 14px;
        color: #000;
    `;
    message.innerHTML = `
        <div style="margin-bottom: 10px;">⏳</div>
        <div id="initText">Initializing...</div>
    `;

    overlay.appendChild(message);
    document.body.appendChild(overlay);
}

function updateInitializingUI(text) {
    const initText = document.getElementById('initText');
    if (initText) {
        initText.textContent = text;
    }
}

function hideInitializingUI() {
    const overlay = document.getElementById('initOverlay');
    if (overlay) {
        overlay.remove();
    }
}

function showErrorUI(message) {
    updateInitializingUI(message);
    setTimeout(() => {
        hideInitializingUI();
    }, 3000);
}

// ===== EVENT LISTENERS =====
function setupEventListeners() {
    // Search
    elements.searchInput.addEventListener('input', handleSearchInput);
    elements.clearSearch.addEventListener('click', clearSearch);
    elements.musicModeToggle.addEventListener('change', handleMusicModeToggle);

    // Tabs
    elements.tabButtons.forEach(btn => {
        btn.addEventListener('click', () => switchTab(btn.dataset.tab));
    });

    // Player Controls (Mini)
    elements.playPauseBtn.addEventListener('click', togglePlayPause);
    elements.prevBtn.addEventListener('click', playPrevious);
    elements.nextBtn.addEventListener('click', playNext);
    elements.expandBtn.addEventListener('click', toggleExpanded);
    elements.collapseBtn.addEventListener('click', toggleExpanded);

    // Player Controls (Expanded)
    elements.expandedPlayPauseBtn.addEventListener('click', togglePlayPause);
    elements.expandedPrevBtn.addEventListener('click', playPrevious);
    elements.expandedNextBtn.addEventListener('click', playNext);

    // Playback Speed
    elements.speedDown.addEventListener('click', decreaseSpeed);
    elements.speedUp.addEventListener('click', increaseSpeed);

    // Shuffle and Repeat
    elements.shuffleBtn.addEventListener('click', toggleShuffle);
    elements.repeatBtn.addEventListener('click', cycleRepeat);

    // Queue
    elements.clearQueueBtn.addEventListener('click', clearQueueAndRefresh);
    elements.queueShuffleBtn.addEventListener('click', toggleShuffle);
    elements.queueRepeatBtn.addEventListener('click', cycleRepeat);

    // Playlists
    elements.createPlaylistBtn.addEventListener('click', showCreatePlaylistModal);
    elements.backToPlaylistsBtn.addEventListener('click', backToPlaylists);
    elements.playAllBtn.addEventListener('click', playAllTracksInPlaylist);

    // Modals
    elements.closePlaylistModal.addEventListener('click', closePlaylistSelectionModal);
    elements.modalCreatePlaylistBtn.addEventListener('click', showCreatePlaylistModalFromSelection);
    elements.closeCreateModal.addEventListener('click', closeCreatePlaylistModal);
    elements.cancelCreateBtn.addEventListener('click', closeCreatePlaylistModal);
    elements.confirmCreateBtn.addEventListener('click', confirmCreatePlaylist);
    elements.newPlaylistNameInput.addEventListener('keypress', (e) => {
        if (e.key === 'Enter') confirmCreatePlaylist();
    });

    // Close modals on overlay click
    elements.playlistModal.addEventListener('click', (e) => {
        if (e.target === elements.playlistModal) closePlaylistSelectionModal();
    });
    elements.createPlaylistModal.addEventListener('click', (e) => {
        if (e.target === elements.createPlaylistModal) closeCreatePlaylistModal();
    });

    // Prevent modal content clicks from propagating to overlay
    document.querySelectorAll('.modal-content').forEach(modal => {
        modal.addEventListener('click', (e) => {
            e.stopPropagation();
        });
    });

    // Progress Bar
    elements.progressBar.addEventListener('input', handleSeek);
}

// ===== SEARCH FUNCTIONALITY =====
function handleSearchInput(e) {
    const value = e.target.value;
    state.searchText = value;

    // Show/hide clear button
    elements.clearSearch.style.display = value ? 'flex' : 'none';

    // Show/hide empty state
    if (value) {
        elements.emptySearch.style.display = 'none';
        elements.searchResults.style.display = 'block';
        // TODO: Implement actual search with debouncing
        performSearch(value);
    } else {
        elements.emptySearch.style.display = 'flex';
        elements.searchResults.style.display = 'none';
    }
}

function clearSearch() {
    elements.searchInput.value = '';
    state.searchText = '';
    elements.clearSearch.style.display = 'none';
    elements.emptySearch.style.display = 'flex';
    elements.searchResults.style.display = 'none';
}

function handleMusicModeToggle(e) {
    state.isMusicMode = e.target.checked;
    updateMusicModeUI();

    // Re-search if there's a query
    if (state.searchText) {
        performSearch(state.searchText);
    }
}

function updateMusicModeUI() {
    const modeToggle = document.querySelector('.mode-toggle');

    if (state.isMusicMode) {
        modeToggle.classList.add('music-mode');
        elements.modeText.textContent = 'Music Mode';
        elements.searchInput.placeholder = 'Search YouTube Music...';

        // Update icon to music note
        elements.modeIcon.innerHTML = '<path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>';
    } else {
        modeToggle.classList.remove('music-mode');
        elements.modeText.textContent = 'General';
        elements.searchInput.placeholder = 'Search YouTube...';

        // Update icon to general/list
        elements.modeIcon.innerHTML = '<path d="M4 6h16v2H4zm0 5h16v2H4zm0 5h16v2H4z"/>';
    }
}

function performSearch(query) {
    // Don't search while initializing
    if (state.isInitializing) {
        return;
    }

    // Clear previous timeout
    if (state.searchTimeout) {
        clearTimeout(state.searchTimeout);
    }

    // Debounce search - wait 500ms after user stops typing
    state.searchTimeout = setTimeout(async () => {
        try {
            console.log(`Searching for: ${query} (Music Mode: ${state.isMusicMode})`);

            // Show loading state
            elements.searchResults.innerHTML = '<div style="padding: 20px; text-align: center; color: #8e8e93;">Searching...</div>';

            // Call Tauri backend
            console.log('Calling search_youtube command...');
            const results = await window.__TAURI_INVOKE__('search_youtube', {
                query: query,
                musicMode: state.isMusicMode
            });

            console.log('Search results received:', results);
            console.log('Number of results:', results.length);

            if (!results || results.length === 0) {
                elements.searchResults.innerHTML = '<div style="padding: 20px; text-align: center; color: #8e8e93;">No results found</div>';
                return;
            }

            // Transform results to match our format
            const transformedResults = results.map(video => ({
                id: video.id,
                title: video.title,
                artist: video.uploader,
                duration: formatDuration(video.duration),
                thumbnail: video.thumbnail_url,
                videoInfo: video
            }));

            displaySearchResults(transformedResults);
        } catch (error) {
            console.error('Search failed:', error);
            console.error('Error message:', error.message);
            console.error('Error details:', error);
            elements.searchResults.innerHTML = `<div style="padding: 20px; text-align: center; color: #ff3b30;">Search failed: ${error.message || error}<br/><small>Check console for details</small></div>`;
        }
    }, 500);
}

function formatDuration(seconds) {
    if (!seconds || seconds <= 0) return '0:00';
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
}

function displaySearchResults(results) {
    elements.searchResults.innerHTML = '';

    results.forEach(result => {
        const resultItem = createResultItem(result);
        elements.searchResults.appendChild(resultItem);
    });
}

function createResultItem(result) {
    const div = document.createElement('div');
    div.className = 'result-item';

    const thumbnailStyle = result.thumbnail
        ? `background-image: url('${result.thumbnail}'); background-size: cover; background-position: center;`
        : '';

    div.innerHTML = `
        <div class="result-thumbnail" style="${thumbnailStyle}"></div>
        <div class="result-info">
            <div class="result-title">${escapeHtml(result.title)}</div>
            <div class="result-meta">
                <span class="result-artist">${escapeHtml(result.artist)}</span>
                <span class="result-duration">${result.duration}</span>
            </div>
        </div>
        <div class="track-actions">
            <button class="track-action-btn play-btn" title="Play">
                <svg viewBox="0 0 24 24" fill="currentColor">
                    <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-2 14.5v-9l6 4.5-6 4.5z"/>
                </svg>
            </button>
            <button class="track-action-btn queue-btn" title="Add to Queue">
                <svg viewBox="0 0 24 24" fill="currentColor">
                    <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm5 11h-4v4h-2v-4H7v-2h4V7h2v4h4v2z"/>
                </svg>
            </button>
            <button class="track-action-btn favorite-btn" title="Add to Playlist">
                <svg viewBox="0 0 24 24" fill="currentColor">
                    <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
                </svg>
            </button>
        </div>
    `;

    // Action buttons
    const playBtn = div.querySelector('.play-btn');
    const queueBtn = div.querySelector('.queue-btn');
    const favoriteBtn = div.querySelector('.favorite-btn');

    playBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        playTrack(result);
    });

    queueBtn.addEventListener('click', async (e) => {
        e.stopPropagation();
        try {
            await window.__TAURI_INVOKE__('add_to_queue', { track: result.videoInfo });
            console.log('Added to queue:', result.title);
        } catch (error) {
            console.error('Failed to add to queue:', error);
        }
    });

    favoriteBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        showPlaylistSelectionModal(result.videoInfo);
    });

    return div;
}

function escapeHtml(text) {
    const div = document.createElement('div');
    div.textContent = text;
    return div.innerHTML;
}

// ===== TAB NAVIGATION =====
function switchTab(tabName) {
    // Update tab buttons
    elements.tabButtons.forEach(btn => {
        btn.classList.toggle('active', btn.dataset.tab === tabName);
    });

    // Update tab contents
    elements.tabContents.forEach(content => {
        content.classList.toggle('active', content.id === `${tabName}Tab`);
    });

    // Load playlists when switching to playlists tab
    if (tabName === 'playlists') {
        loadPlaylists();
    }

    // Load queue when switching to queue tab
    if (tabName === 'queue') {
        loadQueue();
    }
}

// ===== PLAYER FUNCTIONALITY =====
async function playTrack(track) {
    try {
        console.log('Playing track:', track);

        // Store current track
        state.currentTrack = track;
        state.currentPosition = 0;

        // Parse duration from the track
        const durationSeconds = track.videoInfo?.duration || 0;
        state.duration = durationSeconds;

        // Show current track display immediately
        elements.currentTrack.style.display = 'block';

        // Update track info
        elements.trackTitle.textContent = 'Loading...';
        elements.trackArtist.textContent = track.artist;

        // Setup scrolling animation for expanded title
        setupScrollingTitle(elements.expandedTitle, track.title);
        elements.expandedArtist.textContent = track.artist;

        // Update progress bar
        elements.progressBar.max = durationSeconds || 100;
        elements.progressBar.value = 0;
        elements.progressBar.style.setProperty('--progress', '0%');
        elements.duration.textContent = formatTime(durationSeconds);
        elements.currentTime.textContent = '0:00';

        // Show loading state
        state.isPlaying = false;
        updatePlayPauseIcon();
        elements.speakerIcon.classList.remove('playing');

        // Call backend to play track
        await window.__TAURI_INVOKE__('play_track', {
            track: track.videoInfo || track
        });

        // Update title after loading
        elements.trackTitle.textContent = track.title;

        console.log('Playback started successfully');
    } catch (error) {
        console.error('Failed to play track:', error);
        elements.trackTitle.textContent = 'Error loading track';
    }
}

async function togglePlayPause() {
    if (!state.currentTrack) return;

    try {
        await window.__TAURI_INVOKE__('toggle_play_pause');
        // Backend will emit state change event which updates UI
    } catch (error) {
        console.error('Failed to toggle playback:', error);
    }
}

function updatePlayPauseIcon() {
    if (state.isPlaying) {
        // Pause icon
        elements.playPauseIcon.innerHTML = '<path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z"/>';
        elements.expandedPlayPauseIcon.innerHTML = '<path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z"/>';
    } else {
        // Play icon
        elements.playPauseIcon.innerHTML = '<path d="M8 5v14l11-7z"/>';
        elements.expandedPlayPauseIcon.innerHTML = '<path d="M8 5v14l11-7z"/>';
    }
}

async function playPrevious() {
    try {
        const track = await window.__TAURI_INVOKE__('play_previous');
        if (track) {
            console.log('Playing previous track:', track.title);
            // Reset position for the new track
            state.currentPosition = 0;
        } else {
            console.log('No previous track available');
        }
    } catch (error) {
        console.error('Failed to play previous:', error);
    }
}

async function playNext() {
    try {
        const track = await window.__TAURI_INVOKE__('play_next');
        if (track) {
            console.log('Playing next track:', track.title);
            // Reset position for the new track
            state.currentPosition = 0;
        } else {
            console.log('No next track available - end of queue');
            // Stop playback state
            state.isPlaying = false;
            updatePlayPauseIcon();
            elements.speakerIcon.classList.remove('playing');
        }
    } catch (error) {
        console.error('Failed to play next:', error);
    }
}

function toggleExpanded() {
    state.isExpanded = !state.isExpanded;

    if (state.isExpanded) {
        elements.playerExpanded.style.display = 'block';
    } else {
        elements.playerExpanded.style.display = 'none';
    }
}

// ===== PLAYBACK CONTROLS =====
async function decreaseSpeed() {
    // Update position base before changing speed
    if (state.isPlaying) {
        state.positionBase = calculateCurrentPosition();
        state.positionTimestamp = Date.now();
    }
    state.playbackSpeed = Math.max(0.25, state.playbackSpeed - 0.25);
    updateSpeedDisplay();
    try {
        await window.__TAURI_INVOKE__('set_playback_speed', { rate: state.playbackSpeed });
    } catch (error) {
        console.error('Failed to set playback speed:', error);
    }
}

async function increaseSpeed() {
    // Update position base before changing speed
    if (state.isPlaying) {
        state.positionBase = calculateCurrentPosition();
        state.positionTimestamp = Date.now();
    }
    state.playbackSpeed = Math.min(2.0, state.playbackSpeed + 0.25);
    updateSpeedDisplay();
    try {
        await window.__TAURI_INVOKE__('set_playback_speed', { rate: state.playbackSpeed });
    } catch (error) {
        console.error('Failed to set playback speed:', error);
    }
}

function updateSpeedDisplay() {
    elements.speedText.textContent = `${state.playbackSpeed.toFixed(2)}x`;
}

async function handleSeek(e) {
    const value = parseFloat(e.target.value);

    // Update local position tracking
    syncPosition(value);
    elements.currentTime.textContent = formatTime(value);
    updateProgressBarFill();

    try {
        console.log('Seeking to:', value);
        await window.__TAURI_INVOKE__('seek_to', { position: value });
    } catch (error) {
        console.error('Failed to seek:', error);
    }
}

function formatTime(seconds) {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
}

async function toggleShuffle() {
    try {
        const shuffleEnabled = await window.__TAURI_INVOKE__('toggle_shuffle');
        state.shuffleMode = shuffleEnabled;
        updateShuffleUI();
        console.log(`Shuffle: ${shuffleEnabled ? 'ON' : 'OFF'}`);

        // Reload queue if on queue tab to show new order
        const activeTab = document.querySelector('.tab-btn.active');
        if (activeTab && activeTab.dataset.tab === 'queue') {
            await loadQueue();
        }
    } catch (error) {
        console.error('Failed to toggle shuffle:', error);
    }
}

async function cycleRepeat() {
    try {
        const repeatMode = await window.__TAURI_INVOKE__('cycle_repeat_mode');
        state.repeatMode = repeatMode;
        updateRepeatUI();
        console.log(`Repeat mode: ${repeatMode}`);
    } catch (error) {
        console.error('Failed to cycle repeat mode:', error);
    }
}

function updateShuffleUI() {
    const isActive = state.shuffleMode;
    const title = isActive ? 'Shuffle On' : 'Shuffle Off';

    // Update expanded player shuffle button
    if (elements.shuffleBtn) {
        elements.shuffleBtn.classList.toggle('active', isActive);
        elements.shuffleBtn.title = title;
    }

    // Update queue shuffle button
    if (elements.queueShuffleBtn) {
        elements.queueShuffleBtn.classList.toggle('active', isActive);
        elements.queueShuffleBtn.title = title;
    }
}

function updateRepeatUI() {
    const iconPaths = {
        'Off': 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z',
        'All': 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4z',
        'One': 'M7 7h10v3l4-4-4-4v3H5v6h2V7zm10 10H7v-3l-4 4 4 4v-3h12v-6h-2v4zm-5-6h2v4h-2z'
    };

    const isActive = state.repeatMode !== 'Off';
    const title = `Repeat ${state.repeatMode}`;

    // Update expanded player repeat button
    if (elements.repeatBtn) {
        const icon = elements.repeatBtn.querySelector('svg path');
        if (icon) {
            icon.setAttribute('d', iconPaths[state.repeatMode]);
        }
        elements.repeatBtn.classList.toggle('active', isActive);
        elements.repeatBtn.title = title;
    }

    // Update queue repeat button
    if (elements.queueRepeatBtn) {
        const icon = elements.queueRepeatBtn.querySelector('svg path');
        if (icon) {
            icon.setAttribute('d', iconPaths[state.repeatMode]);
        }
        elements.queueRepeatBtn.classList.toggle('active', isActive);
        elements.queueRepeatBtn.title = title;
    }
}

// Scrolling title animation
function setupScrollingTitle(element, text) {
    // Create inner span for animation
    const containerWidth = element.parentElement?.offsetWidth || element.offsetWidth;

    // Create a temporary span to measure text width
    const measureSpan = document.createElement('span');
    measureSpan.style.cssText = 'position: absolute; visibility: hidden; white-space: nowrap; font: inherit;';
    measureSpan.textContent = text;
    document.body.appendChild(measureSpan);
    const textWidth = measureSpan.offsetWidth;
    document.body.removeChild(measureSpan);

    // If text is longer than container, enable scrolling
    if (textWidth > containerWidth - 20) {
        element.classList.add('scrolling');
        // Create duplicate text for seamless loop
        element.innerHTML = `<span class="expanded-title-inner">${escapeHtml(text)}  •  ${escapeHtml(text)}  •  </span>`;

        // Calculate scroll duration based on text length (25px per second)
        const scrollDuration = (textWidth + 30) / 25;
        element.style.setProperty('--scroll-duration', `${scrollDuration}s`);
    } else {
        element.classList.remove('scrolling');
        element.textContent = text;
    }
}

function updateProgress() {
    if (state.isPlaying && state.duration > 0) {
        // Calculate position based on elapsed time (mirrors backend)
        const calculatedPosition = calculateCurrentPosition();

        // Clamp to duration - don't exceed it
        state.currentPosition = Math.min(calculatedPosition, state.duration);

        // Update UI with calculated position (backend handles track ending)
        elements.progressBar.value = state.currentPosition;
        elements.currentTime.textContent = formatTime(state.currentPosition);
        updateProgressBarFill();
    } else if (!state.isPlaying && state.currentTrack) {
        // When paused, use the positionBase which was synced from backend on pause
        elements.progressBar.value = state.positionBase;
        elements.currentTime.textContent = formatTime(state.positionBase);
        state.currentPosition = state.positionBase;
        updateProgressBarFill();
    }
}

function updateProgressBarFill() {
    if (state.duration > 0) {
        const progress = (state.currentPosition / state.duration) * 100;
        elements.progressBar.style.setProperty('--progress', `${progress}%`);
    }
}

// ===== KEYBOARD SHORTCUTS =====
document.addEventListener('keydown', (e) => {
    // Space - Play/Pause
    if (e.code === 'Space' && !e.target.matches('input')) {
        e.preventDefault();
        togglePlayPause();
    }

    // Arrow Left - Previous
    if (e.code === 'ArrowLeft' && e.ctrlKey) {
        e.preventDefault();
        playPrevious();
    }

    // Arrow Right - Next
    if (e.code === 'ArrowRight' && e.ctrlKey) {
        e.preventDefault();
        playNext();
    }
});

// ===== WAIT FOR TAURI API =====
function waitForTauri() {
    return new Promise((resolve) => {
        if (window.__TAURI_INVOKE__) {
            console.log('Tauri API already available');
            resolve();
        } else {
            console.log('Waiting for Tauri API...');
            const checkInterval = setInterval(() => {
                if (window.__TAURI_INVOKE__) {
                    console.log('Tauri API now available');
                    clearInterval(checkInterval);
                    resolve();
                }
            }, 100);
        }
    });
}

// ===== TAURI EVENT LISTENERS =====
async function setupTauriListeners() {
    const { listen } = await import('https://unpkg.com/@tauri-apps/api@2/event');

    // Listen for playback state changes
    let trackEndHandled = false; // Prevent multiple track-end triggers

    await listen('playback-state-changed', (event) => {
        const audioState = event.payload;
        const backendPosition = audioState.current_position || 0;

        console.log(`[Backend] playing=${audioState.is_playing}, position=${backendPosition.toFixed(1)}s, rate=${audioState.playback_rate}`);

        // Update state - sync from backend (source of truth)
        const wasPlaying = state.isPlaying;
        state.isPlaying = audioState.is_playing;
        state.duration = audioState.duration || 0;
        state.playbackSpeed = audioState.playback_rate || 1.0;

        // Sync position from backend - always trust backend position
        syncPosition(backendPosition);

        // Detect track ended: was playing, now not playing, position at duration
        const trackEnded = wasPlaying && !audioState.is_playing &&
                          state.duration > 0 &&
                          backendPosition >= state.duration - 0.5; // Allow small margin

        if (trackEnded && !trackEndHandled) {
            trackEndHandled = true;
            console.log('🏁 Track ended, playing next...');
            setTimeout(() => {
                playNext();
                trackEndHandled = false;
            }, 500);
        }

        // Reset flag when a new track starts playing
        if (audioState.is_playing && backendPosition < 1) {
            trackEndHandled = false;
        }

        if (audioState.current_track) {
            const isNewTrack = !state.currentTrack || state.currentTrack.id !== audioState.current_track.id;

            state.currentTrack = {
                id: audioState.current_track.id,
                title: audioState.current_track.title,
                artist: audioState.current_track.uploader,
                durationSeconds: audioState.current_track.duration,
                videoInfo: audioState.current_track
            };

            // Update track info in player
            elements.currentTrack.style.display = 'block';
            elements.trackTitle.textContent = audioState.current_track.title;
            elements.trackArtist.textContent = audioState.current_track.uploader;

            // Only setup scrolling animation when track changes (not on seek/pause)
            if (isNewTrack) {
                setupScrollingTitle(elements.expandedTitle, audioState.current_track.title);
            }
            elements.expandedArtist.textContent = audioState.current_track.uploader;
        }

        // Update duration display
        elements.duration.textContent = formatTime(state.duration);

        // Update progress bar max value
        elements.progressBar.max = state.duration || 100;
        elements.progressBar.value = state.currentPosition;

        // Update current time display
        elements.currentTime.textContent = formatTime(state.currentPosition);

        // Update progress bar fill
        updateProgressBarFill();

        // Update play/pause icon
        updatePlayPauseIcon();

        // Update speaker animation
        if (state.isPlaying) {
            elements.speakerIcon.classList.add('playing');
        } else {
            elements.speakerIcon.classList.remove('playing');
        }

        // Show loading state
        if (audioState.is_loading) {
            elements.trackTitle.textContent = 'Loading...';
            elements.speakerIcon.classList.remove('playing');
        }
    });
}

// ===== PLAYLIST FUNCTIONS =====

async function loadPlaylists() {
    try {
        const playlists = await window.__TAURI_INVOKE__('get_all_playlists');

        // Load track counts for all playlists before displaying
        const playlistsWithCounts = await Promise.all(
            playlists.map(async (playlist) => {
                const tracks = await window.__TAURI_INVOKE__('get_playlist_tracks', {
                    playlistId: playlist.id
                });
                return {
                    ...playlist,
                    trackCount: tracks.length
                };
            })
        );

        state.playlists = playlistsWithCounts;
        displayPlaylists(playlistsWithCounts);
    } catch (error) {
        console.error('Failed to load playlists:', error);
    }
}

function displayPlaylists(playlists) {
    elements.playlistsContainer.innerHTML = '';

    if (playlists.length === 0) {
        elements.playlistsContainer.innerHTML = `
            <div class="empty-playlists">
                <svg viewBox="0 0 24 24" fill="currentColor">
                    <path d="M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z"/>
                </svg>
                <h3>No Playlists Yet</h3>
                <p>Create your first playlist to organize your favorite songs</p>
            </div>
        `;
        return;
    }

    playlists.forEach(playlist => {
        const card = createPlaylistCard(playlist);
        elements.playlistsContainer.appendChild(card);
    });
}

function createPlaylistCard(playlist) {
    const div = document.createElement('div');
    div.className = 'playlist-card';

    const iconClass = playlist.is_system_playlist ? 'system' : '';
    const iconPath = playlist.is_system_playlist
        ? 'M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z'
        : 'M15 6H3v2h12V6zm0 4H3v2h12v-2zM3 16h8v-2H3v2zM17 6v8.18c-.31-.11-.65-.18-1-.18-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3V8h3V6h-5z';

    const trackCount = playlist.trackCount || 0;

    div.innerHTML = `
        <div class="playlist-icon ${iconClass}">
            <svg viewBox="0 0 24 24" fill="currentColor">
                <path d="${iconPath}"/>
            </svg>
        </div>
        <div class="playlist-info">
            <div class="playlist-name">${escapeHtml(playlist.name)}</div>
            <div class="playlist-count">${trackCount} track${trackCount === 1 ? '' : 's'}</div>
        </div>
        <svg class="playlist-arrow" viewBox="0 0 24 24" fill="currentColor" width="20" height="20">
            <path d="M8.59 16.59L13.17 12 8.59 7.41 10 6l6 6-6 6-1.41-1.41z"/>
        </svg>
    `;

    div.addEventListener('click', () => showPlaylistDetail(playlist));

    return div;
}

async function showPlaylistDetail(playlist) {
    state.currentPlaylist = playlist;

    // Show detail view, hide list view
    elements.playlistsListView.style.display = 'none';
    elements.playlistDetailView.style.display = 'block';

    // Update header
    elements.playlistDetailName.textContent = playlist.name;

    // Load tracks
    try {
        const tracks = await window.__TAURI_INVOKE__('get_playlist_tracks', {
            playlistId: playlist.id
        });
        state.currentPlaylistTracks = tracks;
        displayPlaylistTracks(tracks);
        elements.playlistDetailCount.textContent = `${tracks.length} track${tracks.length === 1 ? '' : 's'}`;
    } catch (error) {
        console.error('Failed to load playlist tracks:', error);
        elements.playlistTracksContainer.innerHTML = '<p style="text-align: center; color: var(--text-secondary); padding: 40px;">Failed to load tracks</p>';
    }
}

function displayPlaylistTracks(tracks) {
    elements.playlistTracksContainer.innerHTML = '';

    if (tracks.length === 0) {
        elements.playlistTracksContainer.innerHTML = `
            <div class="empty-playlists">
                <svg viewBox="0 0 24 24" fill="currentColor">
                    <path d="M12 3v10.55c-.59-.34-1.27-.55-2-.55-2.21 0-4 1.79-4 4s1.79 4 4 4 4-1.79 4-4V7h4V3h-6z"/>
                </svg>
                <h3>Empty Playlist</h3>
                <p>Add tracks by using the heart button when searching</p>
            </div>
        `;
        return;
    }

    tracks.forEach(track => {
        const item = createPlaylistTrackItem(track);
        elements.playlistTracksContainer.appendChild(item);
    });
}

function createPlaylistTrackItem(track) {
    const div = document.createElement('div');
    div.className = 'playlist-track-item';

    div.innerHTML = `
        <div class="playlist-track-info">
            <div class="playlist-track-title">${escapeHtml(track.title)}</div>
            <div class="playlist-track-artist">${escapeHtml(track.author || 'Unknown')}</div>
        </div>
        <div class="playlist-track-duration">${formatDuration(track.duration)}</div>
        <button class="icon-btn remove-track-btn" title="Remove from playlist">
            <svg class="icon" viewBox="0 0 24 24" fill="currentColor">
                <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
            </svg>
        </button>
    `;

    // Play track on click (not on remove button)
    div.addEventListener('click', (e) => {
        if (!e.target.closest('.remove-track-btn')) {
            playTrackFromPlaylist(track);
        }
    });

    // Remove button
    const removeBtn = div.querySelector('.remove-track-btn');
    removeBtn.addEventListener('click', (e) => {
        e.stopPropagation();
        removeTrackFromPlaylist(track.id);
    });

    return div;
}

async function playTrackFromPlaylist(track) {
    const videoInfo = {
        id: track.id,
        title: track.title,
        uploader: track.author || 'Unknown',
        duration: track.duration,
        thumbnail_url: track.thumbnail_url,
        audio_url: null,
        description: null
    };

    await playTrack({ ...videoInfo, videoInfo });
}

async function removeTrackFromPlaylist(trackId) {
    if (!state.currentPlaylist) return;

    try {
        await window.__TAURI_INVOKE__('remove_track_from_playlist', {
            trackId,
            playlistId: state.currentPlaylist.id
        });

        // Reload the playlist
        showPlaylistDetail(state.currentPlaylist);
    } catch (error) {
        console.error('Failed to remove track:', error);
    }
}

async function playAllTracksInPlaylist() {
    if (!state.currentPlaylist || state.currentPlaylistTracks.length === 0) {
        return;
    }

    try {
        await window.__TAURI_INVOKE__('play_playlist', {
            playlistId: state.currentPlaylist.id
        });

        // Switch to queue tab
        switchTab('queue');
    } catch (error) {
        console.error('Failed to play playlist:', error);
    }
}

function backToPlaylists() {
    elements.playlistsListView.style.display = 'block';
    elements.playlistDetailView.style.display = 'none';
    state.currentPlaylist = null;
    state.currentPlaylistTracks = [];
}

// ===== MODAL FUNCTIONS =====

async function showPlaylistSelectionModal(track) {
    state.trackToAdd = track;
    elements.modalTrackTitle.textContent = track.title;

    // Load ALL data before showing modal (no loading states)
    try {
        const playlists = await window.__TAURI_INVOKE__('get_all_playlists');

        // Load track counts and check if track is in each playlist
        const playlistsWithData = await Promise.all(
            playlists.map(async (playlist) => {
                const tracks = await window.__TAURI_INVOKE__('get_playlist_tracks', {
                    playlistId: playlist.id
                });
                return {
                    ...playlist,
                    trackCount: tracks.length,
                    hasTrack: tracks.some(t => t.id === track.id)
                };
            })
        );

        displayModalPlaylists(playlistsWithData);
    } catch (error) {
        console.error('Failed to load playlists:', error);
    }

    // Show modal after everything is loaded
    elements.playlistModal.style.display = 'flex';
}

function displayModalPlaylists(playlists) {
    elements.modalPlaylistsList.innerHTML = '';

    playlists.forEach(playlist => {
        const item = createModalPlaylistItem(playlist);
        elements.modalPlaylistsList.appendChild(item);
    });
}

function createModalPlaylistItem(playlist) {
    const div = document.createElement('div');
    div.className = 'modal-playlist-item';

    const isSystem = playlist.is_system_playlist;
    const isAdded = playlist.hasTrack || false;
    const trackCount = playlist.trackCount || 0;

    if (isAdded) {
        div.classList.add('added');
    }

    const iconPath = isSystem
        ? 'M12 21.35l-1.45-1.32C5.4 15.36 2 12.28 2 8.5 2 5.42 4.42 3 7.5 3c1.74 0 3.41.81 4.5 2.09C13.09 3.81 14.76 3 16.5 3 19.58 3 22 5.42 22 8.5c0 3.78-3.4 6.86-8.55 11.54L12 21.35z'
        : 'M15 6H3v2h12V6zm0 4H3v2h12v-2zM3 16h8v-2H3v2zM17 6v8.18c-.31-.11-.65-.18-1-.18-1.66 0-3 1.34-3 3s1.34 3 3 3 3-1.34 3-3V8h3V6h-5z';

    const statusHtml = isAdded
        ? `<svg viewBox="0 0 24 24" fill="currentColor">
                <path d="M9 16.2L4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4L9 16.2z"/>
            </svg>
            <span>Added</span>`
        : `<svg viewBox="0 0 24 24" fill="currentColor">
                <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm5 11h-4v4h-2v-4H7v-2h4V7h2v4h4v2z"/>
            </svg>`;

    div.innerHTML = `
        <div class="modal-playlist-icon ${isSystem ? 'system' : ''}">
            <svg viewBox="0 0 24 24" fill="currentColor">
                <path d="${iconPath}"/>
            </svg>
        </div>
        <div class="modal-playlist-info">
            <div class="modal-playlist-name">${escapeHtml(playlist.name)}</div>
            <div class="modal-playlist-count">${trackCount} track${trackCount === 1 ? '' : 's'}</div>
        </div>
        <div class="modal-playlist-status ${isAdded ? 'added' : ''}">
            ${statusHtml}
        </div>
    `;

    // Only allow adding if not already added
    if (!isAdded) {
        div.addEventListener('click', (e) => {
            e.stopPropagation();
            addTrackToPlaylistFromModal(playlist);
        });
    }

    return div;
}

async function addTrackToPlaylistFromModal(playlist) {
    if (!state.trackToAdd) return;

    try {
        await window.__TAURI_INVOKE__('add_track_to_playlist', {
            track: state.trackToAdd,
            playlistId: playlist.id
        });
        console.log(`✓ Added "${state.trackToAdd.title}" to "${playlist.name}"`);

        // Refresh modal data to show checkmark (modal stays open for multiple selections)
        const playlists = await window.__TAURI_INVOKE__('get_all_playlists');
        const playlistsWithData = await Promise.all(
            playlists.map(async (p) => {
                const tracks = await window.__TAURI_INVOKE__('get_playlist_tracks', {
                    playlistId: p.id
                });
                return {
                    ...p,
                    trackCount: tracks.length,
                    hasTrack: tracks.some(t => t.id === state.trackToAdd.id)
                };
            })
        );

        displayModalPlaylists(playlistsWithData);
    } catch (error) {
        console.error('Failed to add track to playlist:', error);
    }
}

function closePlaylistSelectionModal() {
    elements.playlistModal.style.display = 'none';
    state.trackToAdd = null;
}

function showCreatePlaylistModal() {
    elements.newPlaylistNameInput.value = '';
    elements.createPlaylistModal.style.display = 'flex';
    setTimeout(() => elements.newPlaylistNameInput.focus(), 100);
}

function showCreatePlaylistModalFromSelection() {
    showCreatePlaylistModal();
}

function closeCreatePlaylistModal() {
    elements.createPlaylistModal.style.display = 'none';
    elements.newPlaylistNameInput.value = '';
}

async function confirmCreatePlaylist() {
    const name = elements.newPlaylistNameInput.value.trim();
    if (!name) return;

    try {
        const playlistId = await window.__TAURI_INVOKE__('create_playlist', { name });
        console.log('✓ Created playlist:', name);

        // If adding from track modal, add track to new playlist
        if (state.trackToAdd) {
            await window.__TAURI_INVOKE__('add_track_to_playlist', {
                track: state.trackToAdd,
                playlistId
            });
            console.log(`✓ Added "${state.trackToAdd.title}" to new playlist "${name}"`);
        }

        closeCreatePlaylistModal();

        // Refresh data
        if (state.trackToAdd) {
            // Refresh modal with all data preloaded
            const playlists = await window.__TAURI_INVOKE__('get_all_playlists');
            const playlistsWithData = await Promise.all(
                playlists.map(async (p) => {
                    const tracks = await window.__TAURI_INVOKE__('get_playlist_tracks', {
                        playlistId: p.id
                    });
                    return {
                        ...p,
                        trackCount: tracks.length,
                        hasTrack: tracks.some(t => t.id === state.trackToAdd.id)
                    };
                })
            );
            displayModalPlaylists(playlistsWithData);
        } else {
            // Refresh playlists tab
            await loadPlaylists();
        }
    } catch (error) {
        console.error('Failed to create playlist:', error);
    }
}

// ===== QUEUE FUNCTIONS =====

async function loadQueue() {
    try {
        const queue = await window.__TAURI_INVOKE__('get_queue');
        const queueInfo = await window.__TAURI_INVOKE__('get_queue_info');

        displayQueue(queue, queueInfo);
    } catch (error) {
        console.error('Failed to load queue:', error);
    }
}

function displayQueue(queue, queueInfo) {
    if (queue.length === 0) {
        // Show empty state
        elements.emptyQueue.style.display = 'flex';
        elements.queueList.style.display = 'none';
        elements.clearQueueBtn.style.display = 'none';
        elements.queueControlsRow.style.display = 'none';
    } else {
        // Show queue list
        elements.emptyQueue.style.display = 'none';
        elements.queueList.style.display = 'block';
        elements.clearQueueBtn.style.display = 'flex';
        elements.queueControlsRow.style.display = 'flex';

        // Update queue info
        elements.queueInfo.textContent = queueInfo;

        // Display queue items
        elements.queueList.innerHTML = '';
        queue.forEach((track, index) => {
            const item = createQueueItem(track, index);
            elements.queueList.appendChild(item);
        });
    }

    // Update shuffle/repeat button states
    updateShuffleUI();
    updateRepeatUI();
}

function createQueueItem(track, index) {
    const div = document.createElement('div');
    div.className = 'queue-item';

    // Check if this is the current track
    if (state.currentTrack && state.currentTrack.id === track.id) {
        div.classList.add('current-track');
    }

    const thumbnailStyle = track.thumbnail_url
        ? `background-image: url('${track.thumbnail_url}'); background-size: cover; background-position: center;`
        : '';

    div.innerHTML = `
        <div class="queue-item-thumbnail" style="${thumbnailStyle}"></div>
        <div class="queue-item-info">
            <div class="queue-item-title">${escapeHtml(track.title)}</div>
            <div class="queue-item-meta">
                <span class="queue-item-artist">${escapeHtml(track.uploader)}</span>
                <span class="queue-item-duration">${formatDuration(track.duration)}</span>
            </div>
        </div>
        <div class="queue-item-actions">
            <button class="queue-item-btn play-btn" title="Play">
                <svg viewBox="0 0 24 24" fill="currentColor">
                    <path d="M8 5v14l11-7z"/>
                </svg>
            </button>
            <button class="queue-item-btn remove-btn" title="Remove">
                <svg viewBox="0 0 24 24" fill="currentColor">
                    <path d="M19 6.41L17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z"/>
                </svg>
            </button>
        </div>
    `;

    // Play button
    const playBtn = div.querySelector('.play-btn');
    playBtn.addEventListener('click', async (e) => {
        e.stopPropagation();
        await playTrack({ ...track, videoInfo: track });
    });

    // Remove button
    const removeBtn = div.querySelector('.remove-btn');
    removeBtn.addEventListener('click', async (e) => {
        e.stopPropagation();
        await removeFromQueue(index);
    });

    // Click on item to play
    div.addEventListener('click', async () => {
        await playTrack({ ...track, videoInfo: track });
    });

    return div;
}

async function removeFromQueue(index) {
    try {
        // Note: Backend doesn't have removeFromQueue yet, we'll need to add it
        // For now, just reload the queue
        console.log('Remove from queue at index:', index);
        await loadQueue();
    } catch (error) {
        console.error('Failed to remove from queue:', error);
    }
}

async function clearQueueAndRefresh() {
    try {
        await window.__TAURI_INVOKE__('clear_queue');
        await loadQueue();
    } catch (error) {
        console.error('Failed to clear queue:', error);
    }
}

// ===== START APP =====
window.addEventListener('DOMContentLoaded', async () => {
    console.log('DOM loaded, waiting for Tauri...');
    await waitForTauri();
    console.log('Setting up event listeners...');
    await setupTauriListeners();
    console.log('Starting initialization...');
    init().catch(error => {
        console.error('Initialization error:', error);
    });
});

// Update progress every second (when playing)
setInterval(updateProgress, 1000);
