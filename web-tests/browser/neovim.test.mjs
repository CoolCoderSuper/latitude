import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawn, execFileSync } from 'node:child_process';
import { mkdtemp, mkdir, readFile, writeFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createServer } from 'node:net';
import { setTimeout as delay } from 'node:timers/promises';
import { chromium } from 'playwright';
import { NeovimGrid } from '../../src/server/assets/neovim-grid.js';

// Run against an explicitly selected build, with an isolated catalog and config:
// LATITUDE_TEST_BINARY=target/debug/latitude.exe node --test web-tests/browser/neovim.test.mjs
test(
  'real Neovim UI preserves unsaved buffers and splits across tab closure and quits explicitly',
  {
    skip: !process.env.LATITUDE_TEST_BINARY,
    timeout: 60000,
  },
  async (t) => {
    const directory = await mkdtemp(join(tmpdir(), 'latitude-neovim-'));
    let server;
    let browser;
    let page;
    t.after(async () => {
      await browser?.close();
      if (server && server.exitCode === null) {
        const stopped = new Promise((done) => server.once('exit', done));
        server.kill();
        await stopped;
      }
      await rm(directory, {
        recursive: true,
        force: true,
        maxRetries: 10,
        retryDelay: 200,
      });
    });
    const project = join(directory, 'project');
    await mkdir(project);
    execFileSync('git', ['init', '--quiet', project], { windowsHide: true });
    const gitFile = 'nested/notes [v2] % λ.txt';
    await mkdir(join(project, 'nested'));
    await writeFile(join(project, gitFile), 'opened from Git\n');
    if (process.env.LATITUDE_TEST_PROJECT_CONFIG)
      await writeFile(
        join(project, 'project.lua'),
        await readFile(process.env.LATITUDE_TEST_PROJECT_CONFIG),
      );
    const publicPort = await freePort();
    const commandPort = await freePort();
    const base = `http://127.0.0.1:${publicPort}`;
    const command = `http://127.0.0.1:${commandPort}`;
    const config = join(directory, 'latitude.json');
    await writeFile(
      config,
      JSON.stringify({
        public_bind: `127.0.0.1:${publicPort}`,
        command_bind: `127.0.0.1:${commandPort}`,
        public_password: 'neovim-test',
        data_dir: join(directory, 'data'),
      }),
    );
    server = spawn(
      resolve(process.env.LATITUDE_TEST_BINARY),
      ['--config', config],
      {
        windowsHide: true,
        env: {
          ...process.env,
          ...(process.env.LATITUDE_TEST_USE_USER_NVIM
            ? {}
            : {
                NVIM_APPNAME: 'latitude-neovim-test',
                XDG_CONFIG_HOME: directory,
                XDG_DATA_HOME: directory,
                XDG_STATE_HOME: directory,
              }),
        },
        stdio: ['ignore', 'pipe', 'pipe'],
      },
    );
    let logs = '';
    server.stdout.on('data', (data) => {
      logs += data;
    });
    server.stderr.on('data', (data) => {
      logs += data;
    });
    await until(
      async () => {
        try {
          return (await fetch(`${command}/health`)).ok;
        } catch {
          return false;
        }
      },
      () => `Server failed to start: ${logs}`,
    );
    const created = await fetch(`${command}/api/projects`, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({
        name: 'demo',
        project_dir: project,
        deployments: [],
      }),
    });
    assert.equal(created.status, 201, await created.text());
    browser = await chromium.launch({
      channel: process.env.PLAYWRIGHT_CHANNEL,
    });
    const context = await browser.newContext({
      viewport: { width: 1200, height: 850 },
    });
    page = await context.newPage();
    const errors = [];
    let screen = new NeovimGrid();
    const observe = (page) => {
      page.on('pageerror', (error) => errors.push(error.message));
      page.on('websocket', (socket) =>
        socket.on('framereceived', ({ payload }) => {
          const message = JSON.parse(payload);
          if (message.type === 'redraw')
            for (const [name, ...calls] of message.events)
              for (const args of calls) screen.apply(name, args);
        }),
      );
    };
    observe(page);
    const login = await page.request.post(`${base}/__latitude/api/session`, {
      data: { password: 'neovim-test' },
    });
    assert.equal(login.status(), 200);
    await page.goto(`${base}/demo`);
    assert.equal(
      await page.locator('[data-editor-tool="files"]').isVisible(),
      true,
    );
    assert.equal(
      await page.locator('[data-editor-tool="neovim"]').isVisible(),
      false,
    );
    await page
      .getByRole('button', { name: 'System settings', exact: true })
      .click();
    await page.getByRole('radio', { name: 'Neovim', exact: true }).check();
    if (process.env.LATITUDE_TEST_SETTINGS_SCREENSHOT)
      await page.screenshot({
        path: process.env.LATITUDE_TEST_SETTINGS_SCREENSHOT,
      });
    await page.getByRole('button', { name: 'Close settings' }).click();
    await page.reload();
    assert.equal(await page.locator('input[value=neovim]').isChecked(), true);
    assert.equal(
      await page.locator('[data-editor-tool="files"]').isVisible(),
      false,
    );
    await page
      .getByRole('link', {
        name: 'Neovim Open Neovim in the project directory',
      })
      .click();
    await page.waitForFunction(() =>
      ['Connected', 'normal'].includes(
        document.querySelector('[data-neovim-status]').textContent,
      ),
    );
    // Verify this is a native UI client, with no terminal implementation loaded.
    assert.equal(await page.locator('.xterm').count(), 0);
    assert.equal(await page.locator('canvas').count(), 1);
    await page.waitForFunction(() =>
      document
        .querySelector('canvas')
        .getContext('2d')
        .font.includes('CaskaydiaCove Nerd Font'),
    );
    assert.ok(
      await page
        .locator('canvas')
        .evaluate((canvas) =>
          canvas.getContext('2d').font.includes('CaskaydiaCove Nerd Font'),
        ),
    );
    const filesPage = await context.newPage();
    await filesPage.route('**/file-viewer.bundle.js', (route) =>
      route.fulfill({ contentType: 'text/javascript', body: '' }),
    );
    await filesPage.goto(`${base}/demo/_files`);
    const headerStyle = (header) => {
      const style = getComputedStyle(header);
      const title = getComputedStyle(header.querySelector('h1'));
      return {
        display: style.display,
        padding: style.padding,
        border: style.border,
        radius: style.borderRadius,
        fontSize: title.fontSize,
        fontWeight: title.fontWeight,
        height: style.height,
        minHeight: style.minHeight,
      };
    };
    assert.deepEqual(
      await page.locator('header').evaluate(headerStyle),
      await filesPage.locator('.files-header').evaluate(headerStyle),
    );
    await page.setViewportSize({ width: 550, height: 700 });
    await filesPage.setViewportSize({ width: 550, height: 700 });
    assert.deepEqual(
      await page.locator('header').evaluate(headerStyle),
      await filesPage.locator('.files-header').evaluate(headerStyle),
    );
    await filesPage.close();
    await page.setViewportSize({ width: 1200, height: 850 });
    await page.locator('textarea').focus();
    if (process.env.LATITUDE_TEST_USE_USER_NVIM) {
      await page.keyboard.press('Escape');
      await page.keyboard.type(
        `:call writefile(split(execute('messages'), "\\n"), 'startup-messages.txt')`,
      );
      await page.keyboard.press('Enter');
      let messages;
      await until(
        async () => {
          try {
            messages = await readFile(
              join(project, 'startup-messages.txt'),
              'utf8',
            );
            return true;
          } catch {
            return false;
          }
        },
        () => 'Could not read startup messages',
      );
      assert.ok(
        !messages.includes('Oil split could not find parent window'),
        messages,
      );
    }
    await page.keyboard.press('Escape');
    await page.keyboard.type(':enew');
    await page.keyboard.press('Enter');
    await page.keyboard.type('i');
    await page.keyboard.insertText('Hello λ界');
    await page.locator('textarea').evaluate((element) => {
      const clipboardData = new DataTransfer();
      clipboardData.setData('text/plain', '\npasted <literal> text');
      element.dispatchEvent(
        new ClipboardEvent('paste', { clipboardData, bubbles: true }),
      );
    });
    await page.keyboard.press('Escape');
    await page.keyboard.type(':w result.txt');
    await page.keyboard.press('Enter');
    let saved = '';
    await until(
      async () => {
        try {
          saved = (
            await readFile(join(project, 'result.txt'), 'utf8')
          ).replaceAll('\r\n', '\n');
          return saved === 'Hello λ界\npasted <literal> text\n';
        } catch {
          return false;
        }
      },
      () =>
        `Neovim did not save the expected text. Saved: ${JSON.stringify(saved)}. Screen: ${screen.cells
          .map((row) =>
            row
              .map(([text]) => text)
              .join('')
              .trimEnd(),
          )
          .join('\n')}`,
    );
    await page.setViewportSize({ width: 850, height: 650 });
    await page.keyboard.type(':vsplit');
    await page.keyboard.press('Enter');
    await page.keyboard.type(':set number');
    await page.keyboard.press('Enter');
    await page.screenshot({ path: join(directory, 'neovim.png') });
    if (process.env.LATITUDE_TEST_SCREENSHOT)
      await page.screenshot({ path: process.env.LATITUDE_TEST_SCREENSHOT });
    await page.keyboard.type(
      process.env.LATITUDE_TEST_PROJECT_CONFIG ? ':qa!' : ':qa',
    );
    await page.keyboard.press('Enter');
    await page.getByRole('button', { name: 'Start new session' }).waitFor();
    await page.getByRole('button', { name: 'Start new session' }).click();
    await page.waitForFunction(() =>
      ['Connected', 'normal'].includes(
        document.querySelector('[data-neovim-status]').textContent,
      ),
    );
    await page.keyboard.type(":call writefile([string(getpid())], 'nvim.pid')");
    await page.keyboard.press('Enter');
    let pid;
    await until(
      async () => {
        try {
          pid = Number(
            (await readFile(join(project, 'nvim.pid'), 'utf8')).trim(),
          );
          return pid > 0;
        } catch {
          return false;
        }
      },
      () => 'New Neovim session did not start',
    );
    await page.keyboard.type(':enew');
    await page.keyboard.press('Enter');
    await page.keyboard.type('i');
    await page.keyboard.insertText('unsaved buffer survives');
    await page.keyboard.press('Escape');
    await page.keyboard.type(':vsplit');
    await page.keyboard.press('Enter');
    await until(
      async () =>
        screen.cells.some((row) =>
          row
            .map(([text]) => text)
            .join('')
            .includes('unsaved buffer survives'),
        ),
      () => 'Unsaved text was not drawn',
    );
    await page.close();
    assert.doesNotThrow(
      () => process.kill(pid, 0),
      'Closing a tab must keep Neovim running',
    );
    screen = new NeovimGrid();
    page = await context.newPage();
    observe(page);
    await page.goto(`${base}/demo/_neovim`);
    await until(
      async () =>
        screen.cells.some((row) =>
          row
            .map(([text]) => text)
            .join('')
            .includes('unsaved buffer survives'),
        ),
      () => 'Reopening the tab did not restore unsaved text',
    );
    await page.keyboard.type(
      ":call writefile([string(getpid()), string(winnr('$'))], 'restored.txt')",
    );
    await page.keyboard.press('Enter');
    await until(
      async () => {
        try {
          return (
            (await readFile(join(project, 'restored.txt'), 'utf8'))
              .trim()
              .replaceAll('\r\n', '\n') === `${pid}\n2`
          );
        } catch {
          return false;
        }
      },
      () => 'Reattachment did not preserve the process and split windows',
    );
    await page.keyboard.type(':only');
    await page.keyboard.press('Enter');
    await page.keyboard.type(':set nohidden');
    await page.keyboard.press('Enter');
    const previousEditor = page;
    const gitPage = await context.newPage();
    await gitPage.goto(`${base}/demo/_diff`);
    const gitLink = gitPage.locator('[data-open-editor][href*="notes"]');
    await gitLink.waitFor();
    screen = new NeovimGrid();
    const openedPage = context.waitForEvent('page').then((popup) => {
      observe(popup);
      return popup;
    });
    await gitLink.click();
    page = await openedPage;
    await page.waitForURL('**/demo/_neovim?*');
    assert.equal(new URL(page.url()).searchParams.get('path'), gitFile);
    await until(
      async () =>
        screen.cells.some((row) =>
          row
            .map(([text]) => text)
            .join('')
            .includes('opened from Git'),
        ),
      () => 'Git file was not opened in Neovim',
    );
    await page.keyboard.type(
      ":call writefile([string(getpid()), string(tabpagenr('$'))], 'git-editor.pid')",
    );
    await page.keyboard.press('Enter');
    await until(
      async () => {
        try {
          return (
            (await readFile(join(project, 'git-editor.pid'), 'utf8'))
              .trim()
              .replaceAll('\r\n', '\n') === `${pid}\n2`
          );
        } catch {
          return false;
        }
      },
      () => 'Git action did not open a new tab in the existing Neovim session',
    );
    await page.keyboard.type(':tabprevious');
    await page.keyboard.press('Enter');
    await page.keyboard.type(':w recovered.txt');
    await page.keyboard.press('Enter');
    await until(
      async () => {
        try {
          return (
            (await readFile(join(project, 'recovered.txt'), 'utf8')).trim() ===
            'unsaved buffer survives'
          );
        } catch {
          return false;
        }
      },
      () => 'Restored buffer could not be saved',
    );
    await previousEditor.close();
    await gitPage.goto(`${base}/demo`);
    await gitPage
      .getByRole('button', { name: 'System settings', exact: true })
      .click();
    await gitPage.getByRole('radio', { name: 'Files', exact: true }).check();
    await gitPage.keyboard.press('Escape');
    assert.equal(
      await gitPage.locator('[data-editor-tool="neovim"]').isVisible(),
      false,
    );
    const filesRedirect = await gitPage.request.get(
      `${base}/demo/_editor?${new URLSearchParams({ path: gitFile })}`,
      { maxRedirects: 0 },
    );
    assert.equal(filesRedirect.status(), 307);
    assert.equal(
      new URL(filesRedirect.headers().location, base).pathname,
      '/demo/_files',
    );
    await gitPage.close();
    await page.keyboard.type(
      process.env.LATITUDE_TEST_PROJECT_CONFIG ? ':qa!' : ':qa',
    );
    await page.keyboard.press('Enter');
    await page.getByRole('button', { name: 'Start new session' }).waitFor();
    await until(
      async () => {
        try {
          process.kill(pid, 0);
          return false;
        } catch (error) {
          return error.code === 'ESRCH';
        }
      },
      () => 'Neovim process was left running after :qa',
    );
    assert.deepEqual(errors, []);
  },
);

async function until(predicate, message) {
  for (let i = 0; i < 100; i++) {
    if (await predicate()) return;
    await delay(100);
  }
  assert.fail(message());
}

async function freePort() {
  const server = createServer();
  await new Promise((done) => server.listen(0, '127.0.0.1', done));
  const { port } = server.address();
  await new Promise((done) => server.close(done));
  return port;
}
