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
    searchTimeout: null
};

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
function playTrack(track) {
    state.currentTrack = track;
    state.isPlaying = true;

    // Show current track display
    elements.currentTrack.style.display = 'block';

    // Update track info
    elements.trackTitle.textContent = track.title;
    elements.trackArtist.textContent = track.artist;
    elements.expandedTitle.textContent = track.title;
    elements.expandedArtist.textContent = track.artist;

    // Update speaker icon
    elements.speakerIcon.classList.add('playing');

    // Update play/pause icon
    updatePlayPauseIcon();

    // TODO: Implement actual playback with Tauri backend
    console.log('Playing track:', track);
}

function togglePlayPause() {
    if (!state.currentTrack) return;

    state.isPlaying = !state.isPlaying;
    updatePlayPauseIcon();

    if (state.isPlaying) {
        elements.speakerIcon.classList.add('playing');
    } else {
        elements.speakerIcon.classList.remove('playing');
    }

    // TODO: Implement actual play/pause with Tauri backend
    console.log('Play/Pause:', state.isPlaying);
}

function updatePlayPauseIcon() {
    if (state.isPlaying) {
        // Pause icon
        elements.playPauseIcon.innerHTML = '<path d="M6 4h4v16H6V4zm8 0h4v16h-4V4z"/>';
    } else {
        // Play icon
        elements.playPauseIcon.innerHTML = '<path d="M8 5v14l11-7z"/>';
    }
}

function playPrevious() {
    // TODO: Implement previous track
    console.log('Previous track');
}

function playNext() {
    // TODO: Implement next track
    console.log('Next track');
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
function decreaseSpeed() {
    state.playbackSpeed = Math.max(0.25, state.playbackSpeed - 0.25);
    updateSpeedDisplay();
    // TODO: Implement actual speed change with Tauri backend
}

function increaseSpeed() {
    state.playbackSpeed = Math.min(2.0, state.playbackSpeed + 0.25);
    updateSpeedDisplay();
    // TODO: Implement actual speed change with Tauri backend
}

function updateSpeedDisplay() {
    elements.speedText.textContent = `${state.playbackSpeed.toFixed(2)}x`;
}

function handleSeek(e) {
    const value = parseFloat(e.target.value);
    state.currentPosition = value;
    // TODO: Implement actual seek with Tauri backend
    console.log('Seeking to:', value);
}

function formatTime(seconds) {
    const mins = Math.floor(seconds / 60);
    const secs = Math.floor(seconds % 60);
    return `${mins}:${secs.toString().padStart(2, '0')}`;
}

function updateProgress() {
    if (state.isPlaying) {
        state.currentPosition += 1;
        if (state.currentPosition >= state.duration) {
            state.currentPosition = state.duration;
            state.isPlaying = false;
            updatePlayPauseIcon();
        }

        elements.progressBar.value = state.currentPosition;
        elements.currentTime.textContent = formatTime(state.currentPosition);
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

// ===== START APP =====
window.addEventListener('DOMContentLoaded', async () => {
    console.log('DOM loaded, waiting for Tauri...');
    await waitForTauri();
    console.log('Starting initialization...');
    init().catch(error => {
        console.error('Initialization error:', error);
    });
});

// Update progress every second (when playing)
setInterval(updateProgress, 1000);
