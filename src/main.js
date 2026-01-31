// ===== STATE MANAGEMENT =====
const state = {
    currentTrack: null,
    isPlaying: false,
    isMusicMode: false,
    searchText: '',
    playbackSpeed: 1.0,
    currentPosition: 0,
    duration: 0,
    isExpanded: false
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
function init() {
    setupEventListeners();
    updateMusicModeUI();
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
    // TODO: Implement actual search with Tauri backend
    console.log(`Searching for: ${query} (Music Mode: ${state.isMusicMode})`);

    // Mock search results for now
    const mockResults = [
        {
            id: '1',
            title: 'Sample Song - Artist Name',
            artist: 'Artist Name',
            duration: '3:45',
            thumbnail: ''
        },
        {
            id: '2',
            title: 'Another Track - Different Artist',
            artist: 'Different Artist',
            duration: '4:20',
            thumbnail: ''
        }
    ];

    displaySearchResults(mockResults);
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
    div.innerHTML = `
        <div class="result-thumbnail"></div>
        <div class="result-info">
            <div class="result-title">${result.title}</div>
            <div class="result-channel">${result.artist} • ${result.duration}</div>
        </div>
    `;

    div.addEventListener('click', () => playTrack(result));

    return div;
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

// ===== START APP =====
init();

// Update progress every second (when playing)
setInterval(updateProgress, 1000);

console.log('YTAudioBar initialized!');
