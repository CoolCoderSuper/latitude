const workspace = document.querySelector('[data-history-workspace]');

if (workspace) {
  const viewport = workspace.querySelector('[data-history-viewport]');
  const list = workspace.querySelector('[data-history-rows]');
  const status = workspace.querySelector('[data-history-status]');
  const retry = workspace.querySelector('[data-history-retry]');
  const records = [];
  const rendered = new Map();
  let nextUrl;
  let loading = false;
  let failed = false;
  let frame;

  function appendPage(page) {
    for (const row of page.querySelectorAll('.history-commit')) {
      records.push(row.outerHTML);
    }
    nextUrl = page.dataset.nextUrl || null;
  }

  appendPage(list.querySelector('[data-history-page]'));
  list.replaceChildren();

  function render() {
    const height = parseFloat(
      getComputedStyle(viewport).getPropertyValue('--history-row-height'),
    );
    const start = Math.max(0, Math.floor(viewport.scrollTop / height) - 8);
    const end = Math.min(
      records.length,
      Math.ceil((viewport.scrollTop + viewport.clientHeight) / height) + 8,
    );
    list.style.height = `${records.length * height}px`;
    for (const [index, row] of rendered) {
      if (
        (index < start || index >= end) &&
        !row.contains(document.activeElement)
      ) {
        row.remove();
        rendered.delete(index);
      } else {
        row.style.top = `${index * height}px`;
      }
    }
    for (let index = start; index < end; index++) {
      if (rendered.has(index)) continue;
      const template = document.createElement('template');
      template.innerHTML = records[index];
      const row = template.content.firstElementChild;
      row.style.top = `${index * height}px`;
      row.dataset.historyIndex = index;
      row.setAttribute('aria-posinset', index + 1);
      row.setAttribute('aria-setsize', nextUrl ? -1 : records.length);
      const following = Array.from(list.children).find(
        (node) => Number(node.dataset.historyIndex) > index,
      );
      list.insertBefore(row, following || null);
      rendered.set(index, row);
    }
    if (!loading && !failed) {
      status.textContent = records.length ? '' : 'No commits found.';
      if (nextUrl && end >= records.length - 8) loadMore();
    }
  }

  function scheduleRender() {
    cancelAnimationFrame(frame);
    frame = requestAnimationFrame(render);
  }

  function loadMore() {
    if (loading || !nextUrl) return;
    loading = true;
    failed = false;
    retry.hidden = true;
    viewport.setAttribute('aria-busy', 'true');
    status.textContent = 'Loading older commits…';
    htmx.ajax('GET', nextUrl, {
      source: workspace,
      target: workspace,
      swap: 'none',
    });
  }

  function showError() {
    failed = true;
    status.textContent = 'Could not load older commits.';
    retry.hidden = false;
  }

  workspace.addEventListener('htmx:after:request', (event) => {
    if (!event.detail.ctx.response.raw.ok) return;
    const page = new DOMParser()
      .parseFromString(event.detail.ctx.text, 'text/html')
      .querySelector('[data-history-page]');
    if (!page) return showError();
    appendPage(page);
  });
  workspace.addEventListener('htmx:response:error', showError);
  workspace.addEventListener('htmx:error', showError);
  workspace.addEventListener('htmx:finally:request', () => {
    loading = false;
    viewport.removeAttribute('aria-busy');
    scheduleRender();
  });
  retry.addEventListener('click', loadMore);
  viewport.addEventListener('scroll', scheduleRender, { passive: true });
  // Keep keyboard navigation continuous across the rendered window.
  list.addEventListener('keydown', (event) => {
    const row = event.target.closest('[data-history-index]');
    if (!row || !['ArrowDown', 'ArrowUp', 'Tab'].includes(event.key)) return;
    const direction =
      event.key === 'ArrowUp' || (event.key === 'Tab' && event.shiftKey)
        ? -1
        : 1;
    const index = Number(row.dataset.historyIndex) + direction;
    if (index < 0 || index >= records.length) return;
    event.preventDefault();
    const height = parseFloat(
      getComputedStyle(viewport).getPropertyValue('--history-row-height'),
    );
    if (
      index * height < viewport.scrollTop ||
      (index + 1) * height > viewport.scrollTop + viewport.clientHeight
    ) {
      viewport.scrollTop = index * height;
    }
    render();
    rendered.get(index)?.focus({ preventScroll: true });
  });
  new ResizeObserver(scheduleRender).observe(viewport);
  render();
}
