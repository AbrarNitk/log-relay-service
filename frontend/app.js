const inputPage = document.getElementById('input-page');
const logsPage = document.getElementById('logs-page');
const streamForm = document.getElementById('stream-form');
const runIdInput = document.getElementById('run-id');
const filterInput = document.getElementById('filter-pattern');
const displayRunId = document.getElementById('display-run-id');
const stopBtn = document.getElementById('stop-btn');
const logsOutput = document.getElementById('logs-output');
const statusMsg = document.getElementById('status-message');
const logsContainer = document.getElementById('logs-container');

let eventSource = null;

// Initialize
runIdInput.focus();

// Event Listeners
streamForm.addEventListener('submit', (e) => {
    e.preventDefault();
    const runId = runIdInput.value.trim();
    if (!runId) return;
    const filter = filterInput.value.trim();

    startStreaming(runId, filter);
});

stopBtn.addEventListener('click', stopStreaming);

// Functions
function startStreaming(runId, filter = '') {
    // Switch View with simple transition logic
    inputPage.classList.remove('active');
    // Allow fade out
    setTimeout(() => {
        logsPage.classList.add('active');
    }, 500); // match CSS transition

    displayRunId.textContent = runId;
    logsOutput.innerHTML = '';
    statusMsg.textContent = 'Connecting to stream...';
    statusMsg.style.display = 'block';

    if (eventSource) {
        eventSource.close();
    }

    const url = `/logs/${encodeURIComponent(runId)}`;
    console.log(`Connecting to ${url}`);

    eventSource = new EventSource(url);

    eventSource.onopen = () => {
        console.log('SSE Open');
        statusMsg.textContent = 'Stream connected.';
        setTimeout(() => {
            statusMsg.style.display = 'none'; // Hide status when running
        }, 2000);
        addSystemLog('Connection established successfully.');
    };

    eventSource.onmessage = (event) => {
        // console.log('SSE Message:', event.data);
        if (filter && !event.data.toLowerCase().includes(filter.toLowerCase())) {
            return;
        }
        addLogEntry(event.data);
        scrollToBottom();
    };

    eventSource.onerror = (err) => {
        console.error('SSE Error:', err);
        statusMsg.style.display = 'block';
        statusMsg.textContent = 'Connection interrupted or complete.';
        addSystemLog('Stream ended or connection lost.', 'error');
        eventSource.close();
    };
}

function stopStreaming() {
    if (eventSource) {
        eventSource.close();
        eventSource = null;
    }

    logsPage.classList.remove('active');
    setTimeout(() => {
        inputPage.classList.add('active');
        runIdInput.focus();
    }, 500);
}

function addLogEntry(message) {
    const div = document.createElement('div');
    div.className = 'log-entry';

    const now = new Date();
    const timeString = now.toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3 });

    // Simple formatting for demonstration
    div.innerHTML = `
        <span class="log-timestamp">${timeString}</span>
        <span class="log-content">${escapeHtml(message)}</span>
    `;

    logsOutput.appendChild(div);
}

function addSystemLog(message, type = 'info') {
    const div = document.createElement('div');
    div.className = 'log-entry system-log';
    div.style.color = type === 'error' ? 'var(--error)' : 'var(--accent-secondary)';
    div.style.fontStyle = 'italic';

    const now = new Date();
    const timeString = now.toLocaleTimeString();

    div.innerHTML = `
        <span class="log-timestamp">[SYSTEM]</span>
        <span class="log-content">${escapeHtml(message)}</span>
    `;

    logsOutput.appendChild(div);
    scrollToBottom();
}

function scrollToBottom() {
    logsContainer.scrollTop = logsContainer.scrollHeight;
}

function escapeHtml(text) {
    if (!text) return '';
    return text
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/"/g, "&quot;")
        .replace(/'/g, "&#039;");
}
