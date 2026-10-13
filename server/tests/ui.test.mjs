import test from 'node:test';
import assert from 'node:assert/strict';
import {readFile} from 'node:fs/promises';
const source = await readFile(new URL('../static/main.js', import.meta.url), 'utf8');
const ui = await import(`data:text/javascript;base64,${Buffer.from(source).toString('base64')}`);
const now = 100000;
const point = {time:now-60,value:14.25};
const channel = {last:point,statistics:{min:14,max:15,mean:14.5},points:[point]};
const data = {devices:[{name:'Berezki',sensors:{BerezkiWater:'Water',BerezkiAir:'Air'}}],channels:{BerezkiWater:channel,Unknown:channel},generated_at:now,history_hours:24,stale_after_seconds:240};
test('device grouping preserves missing sensors and exposes unmapped channels', () => {
    const groups = ui.groupDevices(data);
    assert.equal(groups[0].sensors.length,2);
    assert.equal(groups[0].sensors[1].data,null);
    assert.equal(groups[1].sensors[0].id,'Unknown');
    const html = ui.renderDevices(data);
    assert.match(html, /<h2>Berezki<\/h2>/);
    assert.match(html, /<h3>Water<\/h3>/);
    assert.match(html, /No readings/);
});
test('missing, stale, and future timestamps have distinct states', () => {
    assert.equal(ui.freshness(null, now).kind,'missing');
    assert.equal(ui.freshness(point, now).kind,'fresh');
    assert.equal(ui.freshness({...point,time:now-241},now).kind,'stale');
    assert.equal(ui.freshness({...point,time:now+100},now).kind,'clock');
    assert.equal(ui.temperature(null),'—');
    assert.equal(ui.temperature(Infinity),'—');
});
test('chart handles constant values, a single point, missing data, and outage gaps', () => {
    const chart=ui.chartGeometry([point],now,1);
    assert.ok(Number.isFinite(chart.y(point.value)));
    assert.equal(ui.chartGeometry([],now,1),null);
    const gaps=ui.chartGeometry([{time:now-500,value:15},point],now,1);
    assert.equal(gaps.segments.length,2);
    assert.equal(ui.chartGeometry([{time:now-7200,value:15}],now,1),null);
});
test('device and sensor names are escaped', () => {
    const html=ui.renderDevices({...data,devices:[{name:'<img src=x onerror=alert(1)>',sensors:{BerezkiWater:'<script>bad</script>'}}]});
    assert.ok(!html.includes('<script>'));
    assert.ok(html.includes('&lt;script&gt;'));
    assert.ok(!html.includes('<img'));
});
test('invalid responses fail validation instead of replacing valid readings', () => {
    assert.equal(ui.validateDashboard(data),data);
    for(const invalid of [null,{}, {...data,channels:[]},{...data,history_hours:999}]) assert.throws(()=>ui.validateDashboard(invalid));
});
