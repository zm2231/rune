import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createServer } from 'node:http';
import { extname, join, normalize } from 'node:path';

const dir = process.argv[2];
assert.ok(dir, 'usage: node scripts/voice-web-smoke.mjs DIR');
const types = { '.html': 'text/html', '.js': 'text/javascript', '.wasm': 'application/wasm' };
const server = createServer(async (request, response) => {
    const path = normalize(new URL(request.url, 'http://localhost').pathname).replace(/^\/+/, '') || 'index.html';
    try {
        const body = await readFile(join(dir, path));
        response.writeHead(200, { 'content-type': types[extname(path)] ?? 'application/octet-stream' });
        response.end(body);
    } catch {
        response.writeHead(404);
        response.end();
    }
});
await new Promise((ready) => server.listen(0, '127.0.0.1', ready));
const { chromium } = await import(process.env.PLAYWRIGHT_MODULE || 'playwright');
const browser = await chromium.launch({
    args: ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream', '--autoplay-policy=no-user-gesture-required'],
});
try {
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', (error) => errors.push(String(error)));
    await page.goto(`http://127.0.0.1:${server.address().port}/index.html`);
    await page.click('#start');
    const counters = async () => {
        const text = await page.textContent('#report');
        const number = (name) => Number((new RegExp(`${name} (\\d+)`).exec(text) || [])[1] ?? NaN);
        return { text, captured: number('captured'), played: number('played'), late: number('late') };
    };
    await page.waitForFunction(() => /Running/.test(document.getElementById('report').textContent), null, { timeout: 15000 });
    await page.waitForTimeout(1500);
    const first = await counters();
    await page.waitForTimeout(3000);
    const second = await counters();
    console.log(second.text.replace(/\n/g, ' | '));
    console.log((await page.textContent('#log')).trim());
    assert.deepEqual(errors, []);
    const seconds = (second.captured - first.captured) / 48000;
    assert.ok(seconds > 2.5 && seconds < 3.5, `the microphone delivered ${seconds.toFixed(2)} s of audio in 3 s`);
    const playedSeconds = (second.played - first.played) / 48000;
    assert.ok(playedSeconds > 2.5 && playedSeconds < 3.5, `the speaker played ${playedSeconds.toFixed(2)} s of audio in 3 s`);
    assert.ok(second.late - first.late < 4800, `${second.late - first.late} speaker frames were late`);
    await page.click('#stop');
} finally {
    await browser.close();
}
const refusing = await chromium.launch({ args: ['--use-fake-device-for-media-stream'] });
try {
    const page = await refusing.newPage();
    await page.goto(`http://127.0.0.1:${server.address().port}/index.html`);
    await page.click('#start');
    await page.waitForFunction(() => /Failed/.test(document.getElementById('report').textContent), null, { timeout: 15000 });
    await page.waitForTimeout(2000);
    const log = (await page.textContent('#log')).trim().split('\n');
    console.log(log.join('\n'));
    assert.equal(log.filter((line) => /event Failed/.test(line)).length, 1, 'a browser that refuses the microphone fails the stream once');
    assert.ok(!log.some((line) => /Reconnecting/.test(line)), 'and does not retry');
} finally {
    await refusing.close();
    server.close();
}
console.log('voice web smoke: ok');
