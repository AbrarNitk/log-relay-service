const inputPage = document.getElementById('input-page');
const logsPage = document.getElementById('logs-page');
const streamForm = document.getElementById('stream-form');
const runIdInput = document.getElementById('run-id');
const filterInput = document.getElementById('filter-pattern');
const displayRunId = document.getElementById('display-run-id');
const stopBtn = document.getElementById('stop-btn');
const forceStopBtn = document.getElementById('force-stop-btn');
const clearBtn = document.getElementById('clear-btn');
const logsOutput = document.getElementById('logs-output');
const statusMsg = document.getElementById('status-message');
const logsContainer = document.getElementById('logs-container');
const statusDot = document.querySelector('.status-dot');
const logCountEl = document.getElementById('log-count');

let eventSource = null;
let currentRunId = null;
let logCount = 0;
let autoScroll = true;

// Initialize
runIdInput.focus();

// ----- Event Listeners -----

streamForm.addEventListener('submit', (e) => {
    e.preventDefault();
    const runId = runIdInput.value.trim();
    if (!runId) return;
    const filter = filterInput.value.trim();
    startStreaming(runId, filter);
});

stopBtn.addEventListener('click', stopStreaming);
forceStopBtn.addEventListener('click', forceStopStream);
clearBtn.addEventListener('click', clearLogs);

// Detect if user has scrolled up (pause auto-scroll)
logsContainer.addEventListener('scroll', () => {
    const { scrollTop, scrollHeight, clientHeight } = logsContainer;
    autoScroll = scrollHeight - scrollTop - clientHeight < 50;
});

// ----- Functions -----

function startStreaming(runId, filter = '') {
    // Switch view
    inputPage.classList.remove('active');
    setTimeout(() => {
        logsPage.classList.add('active');
    }, 500);

    currentRunId = runId;
    logCount = 0;
    updateLogCount();
    displayRunId.textContent = runId;
    logsOutput.innerHTML = '';
    statusMsg.textContent = 'Connecting to stream...';
    statusMsg.style.display = 'block';
    setStreamActive(false);

    // Close any existing connection
    if (eventSource) {
        eventSource.close();
        eventSource = null;
    }

    const url = `/stream-logs/${encodeURIComponent(runId)}`;
    console.log(`Connecting to ${url}`);
    eventSource = new EventSource(url);

    eventSource.onopen = () => {
        console.log('SSE connection opened');
        statusMsg.textContent = 'Stream connected.';
        setTimeout(() => {
            statusMsg.style.display = 'none';
        }, 2000);
        setStreamActive(true);
        addSystemLog('Connection established successfully.');
    };

    eventSource.onmessage = (event) => {
        if (filter && !event.data.toLowerCase().includes(filter.toLowerCase())) {
            return;
        }
        addLogEntry(event.data);
        logCount++;
        updateLogCount();
        if (autoScroll) scrollToBottom();
    };

    // Handle "warning" events (e.g. lagged messages)
    eventSource.addEventListener('warning', (event) => {
        addSystemLog(event.data, 'warning');
        if (autoScroll) scrollToBottom();
    });

    // Handle "error" events from the server
    eventSource.addEventListener('error', (event) => {
        if (event.data) {
            addSystemLog(event.data, 'error');
        }
    });

    eventSource.onerror = (err) => {
        console.error('SSE Error:', err);
        statusMsg.style.display = 'block';
        statusMsg.textContent = 'Connection interrupted or complete.';
        setStreamActive(false);
        addSystemLog('Stream ended or connection lost.', 'error');
        if (autoScroll) scrollToBottom();
        eventSource.close();
        eventSource = null;
    };
}

async function stopStreaming() {
    // Only close the local SSE connection.
    // The server-side Arc<SharedStream> refcount drops by 1.
    // Other clients sharing this stream are NOT affected.
    if (eventSource) {
        eventSource.close();
        eventSource = null;
    }
    setStreamActive(false);
    addSystemLog('Disconnected from stream.', 'info');

    // Switch back to input page
    currentRunId = null;
    logsPage.classList.remove('active');
    setTimeout(() => {
        inputPage.classList.add('active');
        runIdInput.focus();
    }, 500);
}

// Force-stop: kills the stream for ALL connected clients.
// Called by the "Force Stop" button.
async function forceStopStream() {
    const runId = currentRunId;

    if (eventSource) {
        eventSource.close();
        eventSource = null;
    }
    setStreamActive(false);

    if (runId) {
        try {
            const resp = await fetch(`/streams/${encodeURIComponent(runId)}`, {
                method: 'DELETE',
            });
            const data = await resp.json();
            console.log('Force stop response:', data);
            if (data.stopped) {
                addSystemLog(`Stream "${runId}" force-stopped on server (all clients disconnected).`, 'warning');
            } else {
                addSystemLog(`Stream "${runId}" was already stopped.`, 'info');
            }
        } catch (e) {
            console.warn('Failed to force-stop stream on server:', e);
            addSystemLog('Could not reach server to force-stop stream.', 'error');
        }
    }

    currentRunId = null;
    logsPage.classList.remove('active');
    setTimeout(() => {
        inputPage.classList.add('active');
        runIdInput.focus();
    }, 500);
}

function clearLogs() {
    logsOutput.innerHTML = '';
    logCount = 0;
    updateLogCount();
    addSystemLog('Logs cleared.');
}

function setStreamActive(active) {
    if (active) {
        statusDot.classList.add('pulse');
        statusDot.style.backgroundColor = 'var(--success)';
        stopBtn.disabled = false;
    } else {
        statusDot.classList.remove('pulse');
        statusDot.style.backgroundColor = 'var(--text-secondary)';
    }
}

function updateLogCount() {
    if (logCountEl) {
        logCountEl.textContent = logCount.toLocaleString();
    }
}

function addLogEntry(message) {
    const div = document.createElement('div');
    div.className = 'log-entry';

    const now = new Date();
    const timeString = now.toLocaleTimeString('en-US', {
        hour12: false,
        hour: '2-digit',
        minute: '2-digit',
        second: '2-digit',
        fractionalSecondDigits: 3,
    });

    div.innerHTML = `
        <span class="log-timestamp">${timeString}</span>
        <span class="log-content">${escapeHtml(message)}</span>
    `;

    logsOutput.appendChild(div);

    // Limit DOM nodes to 5000 to prevent memory issues
    while (logsOutput.children.length > 5000) {
        logsOutput.removeChild(logsOutput.firstChild);
    }
}

function addSystemLog(message, type = 'info') {
    const div = document.createElement('div');
    div.className = 'log-entry system-log';

    const colorMap = {
        info: 'var(--accent-secondary)',
        warning: 'var(--warning, #f59e0b)',
        error: 'var(--error)',
    };
    div.style.color = colorMap[type] || colorMap.info;
    div.style.fontStyle = 'italic';

    div.innerHTML = `
        <span class="log-timestamp">[SYSTEM]</span>
        <span class="log-content">${escapeHtml(message)}</span>
    `;

    logsOutput.appendChild(div);
    if (autoScroll) scrollToBottom();
}

function scrollToBottom() {
    logsContainer.scrollTop = logsContainer.scrollHeight;
}

function escapeHtml(text) {
    if (!text) return '';
    return text
        .replace(/&/g, '&amp;')
        .replace(/</g, '&lt;')
        .replace(/>/g, '&gt;')
        .replace(/"/g, '&quot;')
        .replace(/'/g, '&#039;');
}
