import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { after, before, test } from 'node:test';
import { chromium } from 'playwright';

let browser;
before(async () => {
  browser = await chromium.launch({ channel: process.env.PLAYWRIGHT_CHANNEL });
});
after(async () => browser?.close());

for (const deviceScaleFactor of [1, 1.25, 1.375, 1.5, 2]) {
  test(`separators and window borders join at display scale ${deviceScaleFactor}`, async (t) => {
    const page = await browser.newPage({ deviceScaleFactor });
    const errors = [];
    page.on('pageerror', (error) => errors.push(error.message));
    t.after(async () => {
      await page.close();
      assert.deepEqual(errors, []);
    });
    await page.route('http://neovim.test/**', async (route) => {
      const url = new URL(route.request().url());
      if (url.pathname === '/')
        return route.fulfill({
          contentType: 'text/html',
          body: `<!doctype html>
            <style>
              [data-neovim-editor] { position: relative; width: 240px; height: 80px; }
              canvas { width: 100%; height: 100%; }
              textarea { position: absolute; width: 1px; height: 20px; opacity: 0.01; }
            </style>
            <main data-neovim data-ws-path="/ws">
              <div data-neovim-editor>
                <canvas></canvas><textarea></textarea>
                <div data-neovim-notice>
                  <span data-neovim-status></span>
                  <button data-neovim-dismiss></button>
                  <button data-neovim-restart></button>
                </div>
              </div>
            </main>
            <script type="module" src="/neovim.bundle.js"></script>`,
        });
      return route.fulfill({
        contentType: 'text/javascript',
        body: await readFile(
          new URL('../../src/server/assets/neovim.bundle.js', import.meta.url),
        ),
      });
    });
    await page.routeWebSocket('ws://neovim.test/ws', (socket) => {
      socket.onMessage((data) => {
        const message = JSON.parse(data);
        if (message.type !== 'resize') return;
        const boxes = [
          ['╭─╮', '│ │', '│ │', '╰─╯'],
          ['┌─┐', '│ │', '│ │', '└─┘'],
          ['┏━┓', '┃ ┃', '┃ ┃', '┗━┛'],
          ['╔═╗', '║ ║', '║ ║', '╚═╝'],
        ];
        socket.send(
          JSON.stringify({
            type: 'redraw',
            events: [
              ['grid_resize', [1, message.width, message.height]],
              ['default_colors_set', [0x9c6ade, 0x282334, -1]],
              ['hl_attr_define', [1, { foreground: 0x9c6ade }]],
              [
                'hl_attr_define',
                [
                  2,
                  { foreground: 0x282334, background: 0x9c6ade, reverse: true },
                ],
              ],
              [
                'grid_line',
                ...Array.from({ length: message.height }, (_, row) => [
                  1,
                  row,
                  0,
                  [
                    [' ', 0],
                    ['│', 1],
                    [' ', 0],
                    ['┃', 1],
                    [' ', 0],
                    ['║', 2],
                    ...boxes.flatMap((box) => [
                      [' ', 0],
                      ...[...box[row]].map((text) => [text, 1]),
                    ]),
                  ],
                ]),
              ],
              ['flush', []],
            ],
          }),
        );
      });
      socket.send(JSON.stringify({ type: 'ready' }));
    });
    await page.goto('http://neovim.test/');
    await page.waitForFunction(
      () =>
        document.querySelector('canvas').height ===
        Math.round(80 * devicePixelRatio),
    );
    // Wait for the redraw following the ready message and resize response.
    await page.evaluate(
      () =>
        new Promise((done) =>
          requestAnimationFrame(() => requestAnimationFrame(done)),
        ),
    );
    if (
      deviceScaleFactor === 1 &&
      process.env.LATITUDE_TEST_RENDERER_SCREENSHOT
    )
      await page.locator('canvas').screenshot({
        path: process.env.LATITUDE_TEST_RENDERER_SCREENSHOT,
      });
    const coverage = await page.locator('canvas').evaluate((canvas) => {
      const ctx = canvas.getContext('2d');
      const width = ctx.measureText('M').width * devicePixelRatio;
      const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
      return [1, 3, 5].map((col) => {
        const rows = [];
        for (let y = 0; y < canvas.height; y++) {
          let covered = 0;
          for (let x = Math.ceil(col * width); x < (col + 1) * width; x++) {
            const index = (y * canvas.width + x) * 4;
            if (
              pixels[index] === 0x9c &&
              pixels[index + 1] === 0x6a &&
              pixels[index + 2] === 0xde
            )
              covered++;
          }
          rows.push(covered);
        }
        return rows;
      });
    });
    for (const [index, character] of ['│', '┃', '║'].entries()) {
      assert.ok(
        coverage[index].every((count) => count > 0),
        `${character} has a gap at pixel row ${coverage[index].findIndex((count) => !count)}`,
      );
      assert.equal(
        new Set(coverage[index]).size,
        1,
        `${character} changes thickness between rows`,
      );
    }
    assert.ok(
      coverage[1][0] > coverage[0][0],
      'Heavy separator must be thicker',
    );
    assert.equal(
      coverage[2][0],
      coverage[0][0] * 2,
      'Double separator must have two strokes',
    );
    const borders = await page.locator('canvas').evaluate((canvas) => {
      const ctx = canvas.getContext('2d');
      const width = ctx.measureText('M').width * devicePixelRatio;
      const pixels = ctx.getImageData(0, 0, canvas.width, canvas.height).data;
      return [7, 11, 15, 19].map((col) => {
        const left = Math.round(col * width);
        const right = Math.round((col + 3) * width);
        const w = right - left;
        const h = canvas.height;
        // Include antialiased curve pixels, but exclude the background.
        const ink = Array.from({ length: w * h }, (_, index) => {
          const x = left + (index % w);
          const y = Math.floor(index / w);
          return pixels[(y * canvas.width + x) * 4] > 0x28 + 16;
        });
        const neighbors = (index, diagonal = false) => {
          const x = index % w;
          const y = Math.floor(index / w);
          const result = [];
          for (let dy = -1; dy <= 1; dy++)
            for (let dx = -1; dx <= 1; dx++) {
              if (!dx && !dy) continue;
              if (!diagonal && dx && dy) continue;
              if (x + dx >= 0 && x + dx < w && y + dy >= 0 && y + dy < h)
                result.push((y + dy) * w + x + dx);
            }
          return result;
        };
        const visited = new Set();
        let components = 0;
        for (let index = 0; index < ink.length; index++) {
          if (!ink[index] || visited.has(index)) continue;
          components++;
          const stack = [index];
          visited.add(index);
          while (stack.length) {
            for (const next of neighbors(stack.pop(), true))
              if (ink[next] && !visited.has(next)) {
                visited.add(next);
                stack.push(next);
              }
          }
        }
        const center = Math.floor(h / 2) * w + Math.floor(w / 2);
        const inside = new Set([center]);
        const stack = [center];
        let leaks = false;
        while (stack.length) {
          const index = stack.pop();
          const x = index % w;
          const y = Math.floor(index / w);
          if (!x || x === w - 1 || !y || y === h - 1) leaks = true;
          for (const next of neighbors(index))
            if (!ink[next] && !inside.has(next)) {
              inside.add(next);
              stack.push(next);
            }
        }
        return { components, leaks };
      });
    });
    for (const [index, name] of [
      'rounded',
      'light',
      'heavy',
      'double',
    ].entries()) {
      assert.equal(
        borders[index].components,
        index === 3 ? 2 : 1,
        `${name} border is disconnected`,
      );
      assert.equal(borders[index].leaks, false, `${name} border has a gap`);
    }
  });
}
