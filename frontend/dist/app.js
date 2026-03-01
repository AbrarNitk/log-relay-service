'use strict';

const inputPage = document.getElementById('input-page');
const logsPage = document.getElementById('logs-page');
const streamForm = document.getElementById('stream-form');
const runIdInput = document.getElementById('run-id');
const filterInput = document.getElementById('filter-pattern');
const displayRunId = document.getElementById('display-run-id');
const stopBtn = document.getElementById('stop-btn');
const forceStopBtn = document.getElementById('force-stop-btn');
const pauseBtn = document.getElementById('pause-btn');
const clearBtn = document.getElementById('clear-btn');
const logsOutput = document.getElementById('logs-output');
const statusMsg = document.getElementById('status-message');
const logsContainer = document.getElementById('logs-container');
const statusDot = document.querySelector('.status-dot');
const logCountEl = document.getElementById('log-count');
const streamIdBadge = document.getElementById('stream-id-badge');

let eventSource = null;
let currentRunId = null;
let currentSid = null;   // ULID stream_id received in the 'connected' event
let logCount = 0;
let autoScroll = true;
let paused = false;

runIdInput.focus();

// ── Show/hide seq/time inputs based on delivery policy ───────────────────────
window.onDeliveryChange = function () {
    const v = document.getElementById('delivery-policy').value;
    document.getElementById('opt-seq').style.display = v === 'by_start_seq' ? '' : 'none';
    document.getElementById('opt-time').style.display = v === 'by_start_time' ? '' : 'none';
};

// ── Form submit ───────────────────────────────────────────────────────────────
streamForm.addEventListener('submit', (e) => {
    e.preventDefault();
    const runId = runIdInput.value.trim();
    const delivery = document.getElementById('delivery-policy').value;
    const replay = document.getElementById('replay-policy').value;
    const filter = filterInput.value.trim();
    if (!runId) return;

    const params = new URLSearchParams({ delivery_policy: delivery, replay_policy: replay });

    if (delivery === 'by_start_seq') {
        const seq = document.getElementById('start-seq').value.trim();
        if (!seq) { alert('Enter a start sequence number'); return; }
        params.set('start_seq', seq);
    }
    if (delivery === 'by_start_time') {
        const dt = document.getElementById('start-time').value;
        if (!dt) { alert('Pick a start date/time'); return; }
        params.set('start_time', new Date(dt).toISOString());
    }

    startStreaming(runId, filter, params);
});

stopBtn.addEventListener('click', stopStreaming);
forceStopBtn.addEventListener('click', forceStopStream);
clearBtn.addEventListener('click', clearLogs);
pauseBtn.addEventListener('click', togglePause);

// Pause auto-scroll when user scrolls up
logsContainer.addEventListener('scroll', () => {
    const { scrollTop, scrollHeight, clientHeight } = logsContainer;
    autoScroll = scrollHeight - scrollTop - clientHeight < 50;
});

// ── Start streaming ───────────────────────────────────────────────────────────
function startStreaming(runId, filter, params) {
    inputPage.classList.remove('active');
    setTimeout(() => logsPage.classList.add('active'), 500);

    currentRunId = runId;
    currentSid = null;
    logCount = 0;
    paused = false;
    updateLogCount();
    displayRunId.textContent = runId;
    streamIdBadge.textContent = '';
    logsOutput.innerHTML = '';
    statusMsg.textContent = 'Connecting to stream…';
    statusMsg.style.display = 'block';
    setStreamActive(false);

    if (eventSource) { eventSource.close(); eventSource = null; }

    const url = `/stream-logs/${encodeURIComponent(runId)}?${params}`;
    console.log('Connecting to', url);
    eventSource = new EventSource(url);

    // ── Named SSE events from the server ─────────────────────────────────────

    eventSource.addEventListener('connected', (e) => {
        try {
            const data = JSON.parse(e.data);
            currentSid = data.stream_id;
            streamIdBadge.textContent = currentSid;
        } catch { }
        statusMsg.textContent = 'Stream connected.';
        setTimeout(() => { statusMsg.style.display = 'none'; }, 2000);
        setStreamActive(true);
        addSystemLog(`Connected · stream_id: ${currentSid ?? 'unknown'}`, 'info');
    });

    eventSource.addEventListener('log', (e) => {
        try {
            const log = JSON.parse(e.data);
            const msg = log.message ?? JSON.stringify(log);
            if (filter && !msg.toLowerCase().includes(filter.toLowerCase())) return;
            addLogEntry(log);
            logCount++;
            updateLogCount();
            if (autoScroll) scrollToBottom();
        } catch { }
    });

    eventSource.addEventListener('heartbeat', () => {
        // Silently keeps the connection alive — no UI noise
    });

    eventSource.addEventListener('warning', (e) => {
        addSystemLog(e.data, 'warning');
        if (autoScroll) scrollToBottom();
    });

    eventSource.addEventListener('error', (e) => {
        if (e.data) addSystemLog(e.data, 'error');
    });

    eventSource.addEventListener('terminated', (e) => {
        addSystemLog(`Stream terminated: ${e.data}`, 'error');
        setStreamActive(false);
        if (autoScroll) scrollToBottom();
        eventSource?.close();
        eventSource = null;
    });

    eventSource.onerror = () => {
        statusMsg.style.display = 'block';
        statusMsg.textContent = 'Connection interrupted or complete.';
        setStreamActive(false);
        addSystemLog('Stream ended or connection lost.', 'error');
        if (autoScroll) scrollToBottom();
        eventSource?.close();
        eventSource = null;
    };
}

// ── Disconnect (close only this SSE connection) ───────────────────────────────
async function stopStreaming() {
    if (eventSource) { eventSource.close(); eventSource = null; }
    setStreamActive(false);
    addSystemLog('Disconnected from stream.', 'info');

    currentRunId = null;
    currentSid = null;
    logsPage.classList.remove('active');
    setTimeout(() => { inputPage.classList.add('active'); runIdInput.focus(); }, 500);
}

// ── Force-terminate: kills the server session for this stream_id ─────────────
async function forceStopStream() {
    const sid = currentSid;
    const runId = currentRunId;

    if (eventSource) { eventSource.close(); eventSource = null; }
    setStreamActive(false);

    if (runId && sid) {
        try {
            await fetch(`/stream-logs/${encodeURIComponent(runId)}/${encodeURIComponent(sid)}`, {
                method: 'DELETE',
            });
            addSystemLog(`Session ${sid} terminated on server.`, 'warning');
        } catch (e) {
            addSystemLog('Could not reach server to terminate stream.', 'error');
        }
    }

    currentRunId = null;
    currentSid = null;
    logsPage.classList.remove('active');
    setTimeout(() => { inputPage.classList.add('active'); runIdInput.focus(); }, 500);
}

// ── Pause / Resume ────────────────────────────────────────────────────────────
async function togglePause() {
    const sid = currentSid;
    const runId = currentRunId;
    if (!sid || !runId) return;

    const action = paused ? 'resume' : 'pause';
    try {
        const r = await fetch(
            `/stream-logs/${encodeURIComponent(runId)}/${encodeURIComponent(sid)}/${action}`,
            { method: 'POST' },
        );
        if (r.ok) {
            paused = !paused;
            pauseBtn.innerHTML = paused
                ? `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polygon points="5 3 19 12 5 21 5 3"></polygon></svg> Resume`
                : `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="6" y="4" width="4" height="16"></rect><rect x="14" y="4" width="4" height="16"></rect></svg> Pause`;
            addSystemLog(`Stream ${action}d.`, 'info');
        }
    } catch (e) {
        addSystemLog(`Failed to ${action} stream.`, 'error');
    }
}

function clearLogs() {
    logsOutput.innerHTML = '';
    logCount = 0;
    updateLogCount();
    addSystemLog('Logs cleared.');
}

// ── UI helpers ────────────────────────────────────────────────────────────────
function setStreamActive(active) {
    if (active) {
        statusDot.classList.add('pulse');
        statusDot.style.backgroundColor = 'var(--success)';
        stopBtn.disabled = false;
        pauseBtn.disabled = false;
        forceStopBtn.disabled = false;
    } else {
        statusDot.classList.remove('pulse');
        statusDot.style.backgroundColor = 'var(--text-secondary)';
        pauseBtn.disabled = true;
    }
}

function updateLogCount() {
    if (logCountEl) logCountEl.textContent = logCount.toLocaleString();
}

function addLogEntry(log) {
    const div = document.createElement('div');
    div.className = 'log-entry';

    const ts = log.timestamp_ms
        ? new Date(log.timestamp_ms).toISOString().replace('T', ' ').slice(0, 23)
        : new Date().toLocaleTimeString('en-US', { hour12: false, hour: '2-digit', minute: '2-digit', second: '2-digit', fractionalSecondDigits: 3 });

    div.innerHTML = `
        <span class="log-timestamp">${ts}</span>
        <span class="log-seq">#${log.sequence_id}</span>
        <span class="log-content">${escapeHtml(log.message ?? '')}</span>
    `;
    logsOutput.appendChild(div);

    // Cap DOM nodes to prevent memory issues
    while (logsOutput.children.length > 5000) {
        logsOutput.removeChild(logsOutput.firstChild);
    }
}

function addSystemLog(message, type = 'info') {
    const div = document.createElement('div');
    div.className = 'log-entry system-log';
    const colorMap = { info: 'var(--accent-secondary)', warning: 'var(--warning, #f59e0b)', error: 'var(--error)' };
    div.style.color = colorMap[type] || colorMap.info;
    div.style.fontStyle = 'italic';
    div.innerHTML = `<span class="log-timestamp">[SYSTEM]</span><span class="log-content">${escapeHtml(message)}</span>`;
    logsOutput.appendChild(div);
    if (autoScroll) scrollToBottom();
}

function scrollToBottom() { logsContainer.scrollTop = logsContainer.scrollHeight; }

function escapeHtml(text) {
    if (!text) return '';
    return text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;').replace(/'/g, '&#039;');
}
