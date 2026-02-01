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
    speedText: document.getElementById('speedText')
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
            <div class="result-channel">${escapeHtml(result.artist)} • ${result.duration}</div>
        </div>
    `;

    div.addEventListener('click', () => playTrack(result));

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
