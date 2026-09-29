import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { after, before, test } from 'node:test';
import { chromium } from 'playwright';

let browser;
before(async () => {
  browser = await chromium.launch({ channel: process.env.PLAYWRIGHT_CHANNEL });
});
after(async () => browser?.close());

test('history loads on scroll, retries errors, and keeps a bounded set of rows', async (t) => {
  let fail = true;
  const requests = [];
  const batch = (offset) =>
    `<div data-history-page ${offset < 100 ? `data-next-url="/history?offset=${offset + 50}&snapshot=fixed"` : ''}>${Array.from({ length: 50 }, (_, index) => `<a class="history-commit" href="/commit/${offset + index}"><div class="history-summary"><code>${offset + index}</code><strong>Commit ${offset + index}</strong><span>Author</span></div></a>`).join('')}</div>`;
  const css = await readFile(
    new URL('../../src/server/assets/diff-viewer.css', import.meta.url),
    'utf8',
  );
  const page = await fixture(
    t,
    `<style>${css}</style><main class="history-page"><section data-history-workspace>
    <div class="history-viewport" data-history-viewport><div data-history-rows>${batch(0)}</div></div>
    <div class="history-load-status"><span data-history-status></span><button data-history-retry hidden>Retry</button></div></section></main>`,
    'git-history.js',
    async (route, url) => {
      const offset = Number(url.searchParams.get('offset'));
      requests.push(offset);
      assert.equal(url.searchParams.get('snapshot'), 'fixed');
      return route.fulfill({
        status: fail ? 503 : 200,
        contentType: 'text/html',
        body: fail ? 'Unavailable' : batch(offset),
      });
    },
  );
  await page.waitForFunction(
    () =>
      document.querySelector('[data-history-rows]').style.height === '1800px',
  );
  assert.deepEqual(requests, []);
  const scrollBottom = () =>
    page.locator('[data-history-viewport]').evaluate((element) => {
      element.scrollTop = element.scrollHeight;
    });
  await scrollBottom();
  await page.waitForFunction(
    () => !document.querySelector('[data-history-retry]').hidden,
  );
  fail = false;
  await page.getByRole('button', { name: 'Retry' }).click();
  await page.waitForFunction(
    () =>
      document.querySelector('[data-history-rows]').style.height === '3600px',
  );
  await scrollBottom();
  await page.waitForFunction(
    () =>
      document.querySelector('[data-history-rows]').style.height === '5400px',
  );
  await scrollBottom();
  await page.waitForFunction(() =>
    document.querySelector('[href="/commit/149"]'),
  );
  assert.ok((await page.locator('.history-commit').count()) < 50);
  await page.locator('[data-history-viewport]').evaluate((element) => {
    element.scrollTop = 0;
  });
  await page.waitForFunction(() =>
    document.querySelector('[href="/commit/0"]'),
  );
  assert.ok((await page.locator('.history-commit').count()) < 50);
  assert.deepEqual(requests, [50, 50, 100]);
  assert.equal(await page.locator('.history-load-status').isVisible(), false);
});

async function fixture(t, body, script, respond) {
  const page = await browser.newPage();
  const errors = [];
  page.on('pageerror', (error) => errors.push(error.message));
  t.after(async () => {
    await page.close();
    assert.deepEqual(errors, []);
  });
  // Use the same response policy as the server's document head.
  const template = await readFile(
    new URL('../../src/server/html.rs', import.meta.url),
    'utf8',
  );
  const config = template.match(
    /meta name="htmx-config" content=r#"(.+)"#;/,
  )[1];
  await page.route('http://latitude.test/**', async (route) => {
    const url = new URL(route.request().url());
    if (url.pathname === '/') {
      return route.fulfill({
        contentType: 'text/html',
        body: `<!doctype html><meta name="htmx-config" content='${config}'>
          <script src="/assets/htmx.min.js"></script>${body}
          <script type="module" src="/assets/${script}"></script>`,
      });
    }
    if (url.pathname.startsWith('/assets/')) {
      return route.fulfill({
        contentType: 'text/javascript',
        body: await readFile(
          new URL(`../../src/server${url.pathname}`, import.meta.url),
        ),
      });
    }
    return respond(route, url);
  });
  await page.goto('http://latitude.test/');
  assert.equal(await page.evaluate(() => htmx.version), '4.0.0');
  return page;
}

test('share dialog replacements localize expiry dates and preserve controls on errors', async (t) => {
  let mode = 'success';
  const shell = `<div data-share-dialog-shell><div class="share-dialog-header">Shares</div>
    <form hx-post="/shares" hx-target="closest [data-share-dialog-shell]" hx-swap="outerHTML">
      <input name="password" value="secret"><button>Create link</button></form>
    <span data-share-expires-at="2000000000">Expires soon</span>
    <button id="revoke" hx-delete="/shares/token" hx-target="closest [data-share-dialog-shell]" hx-swap="outerHTML" hx-confirm="Revoke?">Revoke</button></div>`;
  const methods = [];
  const page = await fixture(
    t,
    `<main data-server-shell><dialog open data-share-dialog>${shell}</dialog></main>`,
    'project-home.js',
    async (route, url) => {
      if (!url.pathname.startsWith('/shares'))
        return route.fulfill({ json: { projects: [] } });
      methods.push(route.request().method());
      if (mode === 'network') return route.abort();
      return route.fulfill({
        status: mode === 'success' ? 200 : 500,
        contentType: 'text/html',
        body: mode === 'success' ? shell : 'Share error',
      });
    },
  );
  await page.locator('form button').click();
  await page.waitForFunction(() => {
    const text = document.querySelector('[data-share-expires-at]').textContent;
    return text.startsWith('Expires ') && text !== 'Expires soon';
  });
  page.on('dialog', (dialog) => dialog.accept());
  await page.locator('#revoke').click();
  await page.waitForFunction(
    () =>
      document.querySelector('[data-share-expires-at]').textContent !==
      'Expires soon',
  );
  for (mode of ['http', 'network']) {
    await page.locator('form button').click();
    await page.waitForFunction(
      () =>
        document.querySelector('[data-share-status]')?.textContent ===
        'Latitude could not update the share links.',
    );
    assert.equal(await page.locator('form button').count(), 1);
    await page
      .locator('[data-share-status]')
      .evaluate((element) => element.remove());
  }
  assert.deepEqual(methods, ['POST', 'DELETE', 'POST', 'POST']);
});

test('file saves send editor content, finish after swapping, and recover from errors', async (t) => {
  let mode = 'success';
  let saved;
  const page = await fixture(
    t,
    `
    <main data-file-workspace data-api-url="/files"><div class="file-workspace">
      <div data-file-tree></div><div data-file-preview></div><span data-file-title></span>
      <div data-file-resizer></div><button data-find-file></button><button data-grep-search></button>
      <form data-file-actions hx-put="/save" hx-target="[data-save-state]" hx-swap="innerHTML" hidden>
        <span data-save-state></span><button type="button" data-vim-toggle></button>
        <button data-save disabled>Save</button>
      </form>
      <div data-search-palette hidden><input data-search-input><div data-search-results></div>
        <div data-search-title></div><div data-search-preview-path></div>
        <div data-search-preview-content></div><div data-search-help></div></div>
    </div></main>`,
    'file-viewer.bundle.js',
    async (route, url) => {
      if (url.pathname === '/save') {
        saved = new URLSearchParams(route.request().postData());
        if (mode === 'network') return route.abort();
        return route.fulfill({
          status: mode === 'success' ? 200 : 500,
          contentType: 'text/html',
          body:
            mode === 'success'
              ? '<span data-file-save-result data-ok="true">Saved</span>'
              : 'Server error',
        });
      }
      return route.fulfill({
        json:
          route.request().method() === 'POST'
            ? []
            : url.searchParams.get('path')
              ? {
                  editable: true,
                  content: 'original',
                  git_base_content: 'original',
                }
              : {
                  entries: [
                    { name: 'note.txt', path: 'note.txt', kind: 'file' },
                  ],
                },
      });
    },
  );
  await page.locator('.tree-row').click();
  await page.locator('.cm-content').fill('edited content');
  await page.locator('[data-save]').click();
  await page.waitForFunction(
    () => document.querySelector('[data-save-state]').textContent === 'Saved',
  );
  assert.equal(saved.get('path'), 'note.txt');
  assert.equal(saved.get('content'), 'edited content');
  assert.equal(await page.locator('[data-save]').isDisabled(), true);
  for (mode of ['http', 'network']) {
    await page.locator('.cm-content').fill(`retry ${mode}`);
    await page.locator('[data-save]').click();
    await page.waitForFunction(
      () =>
        document.querySelector('[data-save-state]').textContent ===
        'File could not be saved',
    );
    assert.equal(await page.locator('[data-save]').isEnabled(), true);
  }
});

test('Git actions apply fragments and clear pending state on HTTP and network failures', async (t) => {
  let mode = 'success';
  let action;
  const page = await fixture(
    t,
    `
    <main data-diff-workspace data-action-url="/diff">
      <div data-action-status hidden></div>
      <form class="commit-form" hx-patch="/diff" hx-swap="none">
        <input data-commit-message name="message" value="A commit">
        <button name="action" value="commit">Commit</button>
      </form>
    </main>`,
    'diff-viewer.js',
    async (route) => {
      const params = new URLSearchParams(route.request().postData());
      if (params.get('action') === 'fetch')
        return route.fulfill({ status: 503 });
      action = params;
      if (mode === 'network') return route.abort();
      return route.fulfill({
        status: mode === 'success' ? 200 : 422,
        contentType: 'text/html',
        body:
          mode === 'success'
            ? '<div data-diff-file-update data-path="note.txt"></div>'
            : 'Git rejected the action',
      });
    },
  );
  await page.locator('button').click();
  await page.waitForFunction(
    () => document.querySelector('[data-commit-message]').value === '',
  );
  assert.equal(action.get('action'), 'commit');
  assert.equal(action.get('message'), 'A commit');
  for (mode of ['http', 'network']) {
    await page.locator('[data-commit-message]').fill('Keep this message');
    await page.locator('button').click();
    await page.waitForFunction(() =>
      document
        .querySelector('[data-action-status]')
        .classList.contains('error'),
    );
    assert.equal(
      await page.locator('[data-commit-message]').inputValue(),
      'Keep this message',
    );
    assert.equal(await page.locator('.git-action-pending').count(), 0);
    assert.equal(await page.locator('button').getAttribute('aria-busy'), null);
  }
});

test('Git staging and polling preserve the commit input, focus, and selection', async (t) => {
  let releaseStage;
  let staged = false;
  let revision = 0;
  const fragment = () => `
    <section class="git-overview">Revision ${revision}</section>
    <div data-action-status hidden></div>
    <section id="git-action-panel" class="action-panel" hx-morph-skip>
      <form hx-patch="/diff" hx-swap="none">
        <button name="action" value="stage_all">Stage all</button>
      </form>
      <form class="commit-form" hx-patch="/diff" hx-swap="none">
        <input data-commit-message name="message"><button name="action" value="commit">Commit</button>
      </form>
    </section>
    <section data-file-panel="unstaged">${staged ? 'No unstaged files' : 'note.txt'}</section>
    <section data-file-panel="staged">${staged ? 'note.txt' : 'No staged files'}</section>`;
  const page = await fixture(
    t,
    `<main data-diff-workspace data-action-url="/diff">${fragment()}</main>`,
    'diff-viewer.js',
    async (route) => {
      if (route.request().method() === 'GET') {
        return route.fulfill({ contentType: 'text/html', body: fragment() });
      }
      const action = new URLSearchParams(route.request().postData()).get(
        'action',
      );
      if (action === 'fetch') return route.fulfill({ status: 503 });
      await new Promise((resolve) => {
        releaseStage = resolve;
      });
      staged = true;
      return route.fulfill({ status: 204 });
    },
  );
  await page.addStyleTag({
    content: await readFile(
      new URL('../../src/server/assets/diff-viewer.css', import.meta.url),
      'utf8',
    ),
  });
  const input = page.locator('[data-commit-message]');
  const inputBounds = await input.boundingBox();
  await page.getByRole('button', { name: 'Stage all' }).click();
  await page.waitForFunction(() =>
    document.querySelector('.git-action-pending'),
  );
  assert.deepEqual(await input.boundingBox(), inputBounds);
  assert.equal(
    await page
      .locator('[data-action-status]')
      .evaluate((element) => getComputedStyle(element).pointerEvents),
    'none',
  );
  await input.fill('Keep typing while staging');
  await input.evaluate((element) => {
    window.originalCommitInput = element;
    element.setSelectionRange(5, 11);
  });
  releaseStage();
  await page.waitForFunction(
    () =>
      document.querySelector('[data-file-panel="staged"]').textContent ===
      'note.txt',
  );
  const assertInput = async () => {
    assert.deepEqual(
      await input.evaluate((element) => ({
        same: element === window.originalCommitInput,
        focused: document.activeElement === element,
        value: element.value,
        start: element.selectionStart,
        end: element.selectionEnd,
      })),
      {
        same: true,
        focused: true,
        value: 'Keep typing while staging',
        start: 5,
        end: 11,
      },
    );
  };
  await assertInput();
  revision = 1;
  await page.waitForFunction(
    () => document.querySelector('.git-overview').textContent === 'Revision 1',
  );
  await assertInput();
  assert.equal(await page.locator('.git-action-pending').count(), 0);
});

test('project refresh skips identical HTML and archive triggers refresh while disabling its button', async (t) => {
  let archived = false;
  let requests = 0;
  const list = () =>
    `<div id="project-list" data-project-list hx-get="/projects" hx-trigger="worktreeArchived from:body" hx-target="#project-list" hx-select="[data-project-list]" hx-swap="outerHTML" hx-sync="this:drop"><span>${archived ? 'Archived' : 'Active'}</span></div>`;
  const page = await fixture(
    t,
    `<main data-server-shell>${list()}
    <button id="archive" hx-patch="/archive" hx-swap="none" hx-disable="this" hx-confirm="Archive?">Archive</button>
    </main>`,
    'project-home.js',
    async (route, url) => {
      if (url.pathname === '/archive') {
        archived = true;
        await new Promise((resolve) => setTimeout(resolve, 150));
        return route.fulfill({
          status: 204,
          headers: { 'HX-Trigger': 'worktreeArchived' },
        });
      }
      if (url.pathname === '/projects') {
        requests++;
        return route.fulfill({ contentType: 'text/html', body: list() });
      }
      return route.fulfill({ json: { projects: [] } });
    },
  );
  await page.evaluate(async () => {
    window.originalList = document.querySelector('#project-list');
    await htmx.ajax('GET', '/projects', {
      source: originalList,
      target: originalList,
      swap: 'outerHTML',
    });
  });
  assert.equal(
    await page.evaluate(
      () => originalList === document.querySelector('#project-list'),
    ),
    true,
  );
  page.on('dialog', (dialog) => dialog.accept());
  await page.locator('#archive').click();
  assert.equal(await page.locator('#archive').isDisabled(), true);
  await page.waitForFunction(
    () => document.querySelector('#project-list').textContent === 'Archived',
  );
  assert.equal(await page.locator('#archive').isEnabled(), true);
  assert.equal(requests, 2);
});

test('Git refresh skips identical content and retains selections, expanded files, and commit drafts', async (t) => {
  let content = 'original';
  const fragment = () => `<div data-action-status hidden></div>
    <section id="git-action-panel" class="action-panel" hx-morph-skip><input data-commit-message></section>
    <section data-file-panel="unstaged"><div class="section-heading"><code>1 file</code></div>
      <details class="file-card" data-file-section="unstaged" data-file-path="note.txt">
        <summary>note.txt</summary><input type="checkbox" data-file-select data-selection-kind="unstaged" value="note.txt">
        <form hx-patch="/diff" hx-swap="none"><button>Stage</button></form><pre>${content}</pre>
      </details></section>`;
  const page = await fixture(
    t,
    `<main data-diff-workspace data-action-url="/diff">${fragment()}</main>`,
    'diff-viewer.js',
    async (route) => {
      if (route.request().method() === 'PATCH')
        return route.fulfill({ status: 503 });
      return route.fulfill({ contentType: 'text/html', body: fragment() });
    },
  );
  await page.evaluate(async () => {
    window.originalCard = document.querySelector('details');
    originalCard.open = true;
    const checkbox = document.querySelector('[data-file-select]');
    checkbox.checked = true;
    checkbox.dispatchEvent(new Event('change', { bubbles: true }));
    document.querySelector('[data-commit-message]').value = 'Keep my draft';
    const workspace = document.querySelector('[data-diff-workspace]');
    await htmx.ajax('GET', '/diff', {
      source: workspace,
      target: workspace,
      swap: 'innerMorph',
    });
  });
  assert.equal(
    await page.evaluate(
      () => originalCard === document.querySelector('details'),
    ),
    true,
  );
  content = 'updated';
  await page.evaluate(async () => {
    const workspace = document.querySelector('[data-diff-workspace]');
    await htmx.ajax('GET', '/diff', {
      source: workspace,
      target: workspace,
      swap: 'innerMorph',
    });
  });
  assert.equal(await page.locator('pre').textContent(), 'updated');
  assert.equal(
    await page.locator('details').evaluate((element) => element.open),
    true,
  );
  assert.equal(await page.locator('[data-file-select]').isChecked(), true);
  assert.equal(
    await page.locator('[data-commit-message]').inputValue(),
    'Keep my draft',
  );
});
