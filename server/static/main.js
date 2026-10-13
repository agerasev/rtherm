const REFRESH_MS = 10000;
const REQUEST_TIMEOUT_MS = 8000;
const escape = (value) => String(value ?? '').replace(/[&<>"']/g, ch => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[ch]));
export const temperature = value => typeof value === 'number' && Number.isFinite(value) ? value.toFixed(1) : '—';
export const localTime = seconds => new Date(seconds * 1000).toLocaleTimeString([], {hour:'2-digit', minute:'2-digit'});
const axisTime = (seconds, hours) => hours === 24
    ? new Date(seconds * 1000).toLocaleString([], {month:'short', day:'numeric', hour:'2-digit', minute:'2-digit'})
    : localTime(seconds);
export function freshness(last, now, staleAfter = 240) {
    if (!last || !Number.isFinite(last.time) || !Number.isFinite(last.value)) return {kind:'missing', label:'No readings'};
    const age = now - last.time;
    if (age < -60) return {kind:'clock', label:'Check sensor clock'};
    if (age > staleAfter) return {kind:'stale', label:'Stale'};
    return {kind:'fresh', label:'Reporting'};
}
export function ageText(seconds) {
    if (seconds < -60) return 'Timestamp is in the future';
    if (seconds < 60) return 'Updated just now';
    if (seconds < 3600) return `Updated ${Math.floor(seconds / 60)} min ago`;
    if (seconds < 86400) return `Updated ${Math.floor(seconds / 3600)} hr ago`;
    return `Updated ${Math.floor(seconds / 86400)} days ago`;
}
export function groupDevices(data) {
    const assigned = new Set();
    const groups = data.devices.map(device => ({name:device.name, sensors:Object.entries(device.sensors).map(([id, name]) => {
        assigned.add(id);
        return {id, name, data:data.channels[id] ?? null};
    })}));
    const extra = Object.keys(data.channels).sort().filter(id => !assigned.has(id)).map(id => ({id, name:id, data:data.channels[id]}));
    if (extra.length) groups.push({name:'Unassigned device', unassigned:true, sensors:extra});
    return groups;
}
export function validPoints(points) {
    return (Array.isArray(points) ? points : []).filter(p => p && Number.isFinite(p.time) && Number.isFinite(p.value)).sort((a,b) => a.time-b.time);
}
export function chartGeometry(points, now, hours, gap = 240, gaps) {
    const start = now - hours * 3600;
    points = validPoints(points).filter(p => p.time >= start && p.time <= now);
    if (!points.length) return null;
    let low = Math.min(...points.map(p => p.value)), high = Math.max(...points.map(p => p.value));
    const pad = Math.max((high - low) * .2, .5);
    low -= pad; high += pad;
    const x = time => 38 + 552 * (time - start) / (hours * 3600);
    const y = value => 12 + 126 * (high - value) / (high - low);
    const segments = [];
    for (const point of points) {
        const segment = segments.at(-1);
        const hasGap = segment && (Array.isArray(gaps) ? gaps.some(time => time > segment.at(-1).time && time <= point.time) : point.time - segment.at(-1).time > gap);
        if (!segment || hasGap) segments.push([point]);
        else segment.push(point);
    }
    return {points, segments, low, high, x, y, start, now};
}
function chartMarkup(sensor, data) {
    const geometry = chartGeometry(sensor.data?.points, data.generated_at, data.history_hours, data.stale_after_seconds, sensor.data?.gaps);
    if (!geometry) return '<div class="chart-empty">No readings in this time range.<br>New measurements will appear here automatically.</div>';
    const {segments, low, high, x, y, start, now} = geometry;
    const lines = segments.map(segment => {
        const path = segment.map((p,i) => `${i ? 'L':'M'}${x(p.time).toFixed(2)},${y(p.value).toFixed(2)}`).join(' ');
        return `<path class="area" d="${path} L${x(segment.at(-1).time)},138 L${x(segment[0].time)},138 Z"/><path class="line" d="${path}"/>${segment.length === 1 ? `<circle class="point" cx="${x(segment[0].time)}" cy="${y(segment[0].value)}" r="3.5"/>` : ''}`;
    }).join('');
    const grid = [high,(high+low)/2,low].map(value => `<line class="grid" x1="38" x2="590" y1="${y(value)}" y2="${y(value)}"/><text x="30" y="${y(value)+3}" text-anchor="end">${value.toFixed(1)}</text>`).join('');
    return `<svg class="chart" data-chart="${escape(sensor.id)}" viewBox="0 0 600 162" role="img" tabindex="0" aria-label="${escape(sensor.name)} temperature history. Use left and right arrow keys to inspect readings."><title>${escape(sensor.name)}: last ${data.history_hours} hour${data.history_hours === 1 ? '' : 's'}, ${temperature(sensor.data.statistics.min)} to ${temperature(sensor.data.statistics.max)} degrees Celsius</title>${grid}${lines}<text x="38" y="159">${axisTime(start, data.history_hours)}</text><text x="590" y="159" text-anchor="end">${axisTime(now, data.history_hours)}</text><g class="chart-marker" hidden><line class="cursor" y1="10" y2="138"/><circle class="point" r="4"/></g></svg>`;
}
export function renderDevices(data) {
    const groups = groupDevices(data);
    if (!groups.length) return '<div class="empty-state"><h2>No devices yet</h2><p>Waiting for the first sensor to send a reading.</p></div>';
    const icon = '<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" aria-hidden="true"><rect x="5" y="5" width="14" height="14" rx="3"/><rect x="9" y="9" width="6" height="6" rx="1"/><path d="M9 2v3m6-3v3M9 19v3m6-3v3M2 9h3m-3 6h3m14-6h3m-3 6h3"/></svg>';
    return groups.map(device => {
        const reporting = device.sensors.filter(s => freshness(s.data?.last, data.generated_at, data.stale_after_seconds).kind === 'fresh').length;
        return `<section class="device"><header class="device-header"><div class="device-title"><span class="device-icon">${icon}</span><div><h2>${escape(device.name)}</h2><p class="device-subtitle">${device.sensors.length} sensor${device.sensors.length === 1 ? '' : 's'}</p></div></div><span class="badge ${reporting === device.sensors.length && reporting ? 'fresh' : 'stale'}" data-device-status>${reporting} / ${device.sensors.length} reporting</span></header><div class="sensors">${device.sensors.map(sensor => {
            const last = sensor.data?.last;
            const status = freshness(last, data.generated_at, data.stale_after_seconds);
            const stats = sensor.data?.statistics;
            return `<article class="sensor" data-sensor="${escape(sensor.id)}"><div class="sensor-top"><div><h3>${escape(sensor.name)}</h3><p class="channel-id">${escape(sensor.id)}</p></div><span class="badge ${status.kind}" data-freshness>${status.label}</span></div><div class="reading ${status.kind === 'missing' ? 'unavailable':''}">${temperature(last?.value)}<span class="unit">°C</span></div><p class="updated" data-age title="${last ? escape(new Date(last.time*1000).toLocaleString()):''}">${last ? ageText(data.generated_at-last.time):'Waiting for first measurement'}</p><div class="metrics">${[['Low',stats?.min],['Average',stats?.mean],['High',stats?.max]].map(([label,value]) => `<div><span class="metric-label">${label} · ${data.history_hours}h</span><span class="metric-value">${temperature(value)} <small>°C</small></span></div>`).join('')}</div><div class="chart-heading"><span>Recent history</span><span class="chart-tooltip" data-tooltip>Hover or focus to inspect</span></div>${chartMarkup(sensor,data)}</article>`;
        }).join('')}</div>${device.unassigned ? '<p class="device-note">These sensors have not been assigned to a device.</p>':''}</section>`;
    }).join('');
}
export function validateDashboard(data) {
    if (!data || !Array.isArray(data.devices) || !data.channels || typeof data.channels !== 'object' || Array.isArray(data.channels) || !Number.isFinite(data.generated_at) || ![1,6,24].includes(data.history_hours) || !Number.isFinite(data.stale_after_seconds)) throw new Error('Invalid dashboard response');
    for (const device of data.devices) if (!device || typeof device.name !== 'string' || !device.sensors || typeof device.sensors !== 'object' || Array.isArray(device.sensors)) throw new Error('Invalid device configuration');
    for (const channel of Object.values(data.channels)) {
        if (!channel || !channel.statistics || !Array.isArray(channel.points) || (channel.last !== null && (!channel.last || !Number.isFinite(channel.last.time) || !Number.isFinite(channel.last.value)))) throw new Error('Invalid sensor response');
    }
    return data;
}
async function start() {
    const root = document.getElementById('devices');
    const notice = document.getElementById('notice');
    const refresh = document.getElementById('refresh');
    const connection = document.getElementById('connection-status');
    const dot = document.getElementById('connection-dot');
    let snapshot = null, receivedAt = 0, hours = 24, timer, controller, requestId = 0;
    function updateAges() {
        if (!snapshot) return;
        const now = snapshot.generated_at + (Date.now()-receivedAt)/1000;
        let total = 0, fresh = 0;
        for (const card of root.querySelectorAll('.device')) {
            let deviceFresh = 0, deviceTotal = 0;
            for (const sensor of card.querySelectorAll('[data-sensor]')) {
                const last = snapshot.channels[sensor.dataset.sensor]?.last;
                const status = freshness(last, now, snapshot.stale_after_seconds);
                const badge = sensor.querySelector('[data-freshness]');
                badge.className = `badge ${status.kind}`; badge.textContent = status.label;
                sensor.querySelector('[data-age]').textContent = last ? ageText(now-last.time) : 'Waiting for first measurement';
                deviceTotal++; total++;
                if (status.kind === 'fresh') { deviceFresh++; fresh++; }
            }
            const badge = card.querySelector('[data-device-status]');
            badge.textContent = `${deviceFresh} / ${deviceTotal} reporting`;
            badge.className = `badge ${deviceFresh && deviceFresh === deviceTotal ? 'fresh' : 'stale'}`;
        }
        document.getElementById('overview').textContent = `${groupDevices(snapshot).length} device${groupDevices(snapshot).length === 1 ? '' : 's'} · ${total} sensor${total === 1 ? '' : 's'} · ${fresh} reporting`;
    }
    function bindCharts() {
        for (const svg of root.querySelectorAll('[data-chart]')) {
            const geometry = chartGeometry(snapshot.channels[svg.dataset.chart]?.points, snapshot.generated_at, snapshot.history_hours, snapshot.stale_after_seconds, snapshot.channels[svg.dataset.chart]?.gaps);
            let index = geometry.points.length-1;
            const tooltip = svg.parentElement.querySelector('[data-tooltip]');
            function show(point) {
                tooltip.textContent = `${localTime(point.time)} · ${temperature(point.value)} °C`;
                const marker = svg.querySelector('.chart-marker'); marker.removeAttribute('hidden');
                marker.querySelector('line').setAttribute('x1', geometry.x(point.time)); marker.querySelector('line').setAttribute('x2', geometry.x(point.time));
                marker.querySelector('circle').setAttribute('cx', geometry.x(point.time)); marker.querySelector('circle').setAttribute('cy', geometry.y(point.value));
            }
            function hide() { svg.querySelector('.chart-marker').setAttribute('hidden',''); tooltip.textContent = 'Hover or focus to inspect'; }
            svg.addEventListener('pointermove', event => {
                const x = (event.clientX-svg.getBoundingClientRect().left)/svg.getBoundingClientRect().width*600;
                index = geometry.points.reduce((best,p,i) => Math.abs(geometry.x(p.time)-x) < Math.abs(geometry.x(geometry.points[best].time)-x) ? i : best, 0);
                show(geometry.points[index]);
            });
            svg.addEventListener('pointerleave', hide);
            svg.addEventListener('focus', () => show(geometry.points[index]));
            svg.addEventListener('blur', hide);
            svg.addEventListener('keydown', event => {
                if (!['ArrowLeft','ArrowRight','Home','End'].includes(event.key)) return;
                event.preventDefault();
                index = event.key === 'Home' ? 0 : event.key === 'End' ? geometry.points.length-1 : Math.max(0,Math.min(geometry.points.length-1,index+(event.key==='ArrowRight'?1:-1)));
                show(geometry.points[index]);
            });
        }
    }
    async function poll() {
        clearTimeout(timer); controller?.abort();
        const id = ++requestId;
        const currentController = new AbortController();
        controller = currentController;
        const timeout = setTimeout(() => currentController.abort(), REQUEST_TIMEOUT_MS);
        refresh.disabled = true;
        try {
            const response = await fetch(`dashboard?hours=${hours}`, {signal:currentController.signal, cache:'no-store'});
            if (!response.ok) throw new Error(`HTTP ${response.status}`);
            const data = validateDashboard(await response.json());
            if (id !== requestId) return;
            snapshot = data; receivedAt = Date.now();
            const focused = document.activeElement?.dataset?.chart;
            root.innerHTML = renderDevices(data);
            root.setAttribute('aria-busy','false');
            updateAges(); bindCharts();
            if (focused) [...root.querySelectorAll('[data-chart]')].find(el => el.dataset.chart===focused)?.focus({preventScroll:true});
            notice.hidden = true; connection.textContent = 'Server connected'; dot.className = 'status-dot online';
            document.getElementById('last-check').textContent = `Last checked ${new Date().toLocaleTimeString()} · Auto-refresh 10s`;
        } catch (error) {
            if (id !== requestId) return;
            connection.textContent = 'Connection lost'; dot.className = 'status-dot offline';
            notice.textContent = snapshot ? 'Cannot reach the server. Showing the last received readings; retrying automatically.' : 'Cannot reach the server. Check your connection or try Refresh. Retrying automatically.';
            notice.hidden = false; root.setAttribute('aria-busy','false');
            if (!snapshot) root.innerHTML = '<div class="empty-state"><h2>Waiting for a connection</h2><p>Your devices will appear when the server is available.</p></div>';
        } finally {
            clearTimeout(timeout);
            if (id === requestId) { refresh.disabled = false; timer = setTimeout(poll, REFRESH_MS); }
        }
    }
    refresh.addEventListener('click', poll);
    document.querySelectorAll('[data-hours]').forEach(button => button.addEventListener('click', () => {
        hours = Number(button.dataset.hours);
        document.querySelectorAll('[data-hours]').forEach(b => b.setAttribute('aria-pressed',String(b === button)));
        poll();
    }));
    setInterval(updateAges, 1000);
    await poll();
}
if (typeof document !== 'undefined') start();
