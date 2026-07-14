(() => {
  const workspace = document.querySelector('[data-diff-workspace]');
  if (!workspace) return;

  const actionUrl = workspace.dataset.actionUrl || window.location.pathname;
  let openKeys = new Set();
  let refreshTimer = null;
  let autoRefreshPending = false;
  let commitMessage = '';

  const cardKey = (card) => {
    const section = card.dataset.fileSection;
    const path = card.dataset.filePath;
    return section && path ? `${section}:${path}` : null;
  };

  const requestVerb = (event) => event.detail.requestConfig?.verb?.toLowerCase();

  workspace.addEventListener('htmx:beforeRequest', (event) => {
    if (requestVerb(event) !== 'patch') return;
    event.detail.elt.classList.add('git-action-pending');
    event.detail.elt.querySelector('button')?.setAttribute('aria-busy', 'true');
    showStatus('Applying Git action…', false);
  });

  workspace.addEventListener('htmx:afterRequest', (event) => {
    if (requestVerb(event) !== 'patch') return;
    event.detail.elt.classList.remove('git-action-pending');
    event.detail.elt.querySelector('button')?.removeAttribute('aria-busy');
    if (!event.detail.successful) return;

    if (event.detail.xhr.status === 200 && applyFileUpdate(event.detail.xhr.responseText)) {
      hideStatus();
    } else {
      scheduleFullRefresh();
    }
  });

  workspace.addEventListener('htmx:beforeSwap', (event) => {
    if (requestVerb(event) !== 'get') return;
    commitMessage = workspace.querySelector('[data-commit-message]')?.value || '';
    openKeys = new Set(
      Array.from(workspace.querySelectorAll('details.file-card[open]'))
        .map(cardKey)
        .filter(Boolean),
    );
  });

  workspace.addEventListener('htmx:afterSwap', (event) => {
    if (requestVerb(event) !== 'get') return;
    autoRefreshPending = false;
    const messageInput = workspace.querySelector('[data-commit-message]');
    if (messageInput) messageInput.value = commitMessage;
    workspace.querySelectorAll('details.file-card').forEach((card) => {
      if (openKeys.has(cardKey(card))) card.open = true;
    });
  });

  workspace.addEventListener('htmx:responseError', (event) => {
    autoRefreshPending = false;
    const message = event.detail.xhr?.responseText?.trim()
      || 'The Git action could not be completed.';
    showStatus(message, true);
  });

  function applyFileUpdate(responseText) {
    const document = new DOMParser().parseFromString(responseText, 'text/html');
    const update = document.querySelector('[data-diff-file-update]');
    if (!update) return false;
    const path = update.dataset.path;

    update.querySelectorAll('template[data-file-section-update]').forEach((template) => {
      const section = template.dataset.fileSectionUpdate;
      const panel = workspace.querySelector(`[data-file-panel="${section}"]`);
      if (!panel) return;
      const existing = Array.from(panel.querySelectorAll('details.file-card'))
        .find((card) => card.dataset.filePath === path);
      const replacement = template.content.querySelector('details.file-card');
      const wasOpen = Boolean(existing?.open);

      if (existing && replacement) {
        const next = replacement.cloneNode(true);
        next.open = wasOpen;
        existing.replaceWith(next);
      } else if (existing) {
        existing.remove();
      } else if (replacement) {
        let list = panel.querySelector('.file-list');
        if (!list) {
          list = document.createElement('div');
          list.className = 'file-list';
          panel.querySelector('.empty')?.replaceWith(list);
        }
        list.append(replacement.cloneNode(true));
      }
      updatePanel(panel);
    });
    htmx.process(workspace);
    return true;
  }

  function updatePanel(panel) {
    const count = panel.querySelectorAll('details.file-card').length;
    const countLabel = count === 1 ? '1 file' : `${count} files`;
    const counter = panel.querySelector('.section-heading code');
    if (counter) counter.textContent = countLabel;

    const list = panel.querySelector('.file-list');
    if (count === 0) {
      list?.remove();
      if (!panel.querySelector('.empty')) {
        const empty = document.createElement('div');
        empty.className = 'empty';
        empty.textContent = panel.dataset.emptyMessage || 'No files.';
        panel.append(empty);
      }
    } else {
      panel.querySelector(':scope > .empty')?.remove();
    }
  }

  function scheduleFullRefresh() {
    clearTimeout(refreshTimer);
    showStatus('Refreshing changes in the background…', false);
    refreshTimer = window.setTimeout(() => {
      htmx.ajax('GET', actionUrl, {
        source: workspace,
        target: workspace,
        swap: 'innerHTML',
      });
    }, 150);
  }

  function refreshChanges() {
    if (
      autoRefreshPending
      || document.hidden
      || workspace.querySelector('.git-action-pending')
      || document.activeElement?.matches('[data-commit-message]')
    ) return;
    autoRefreshPending = true;
    htmx.ajax('GET', actionUrl, {
      source: workspace,
      target: workspace,
      swap: 'innerHTML',
    });
  }

  async function fetchRemote() {
    if (document.hidden || workspace.querySelector('.git-action-pending')) return;
    try {
      const body = new URLSearchParams({ action: 'fetch' });
      const response = await fetch(actionUrl, {
        method: 'PATCH',
        body,
        credentials: 'same-origin',
        headers: { 'Content-Type': 'application/x-www-form-urlencoded;charset=UTF-8' },
      });
      if (response.ok) refreshChanges();
    } catch {
      // Local changes can still refresh while the remote is unavailable.
    }
  }

  window.setInterval(refreshChanges, 2000);
  window.setInterval(fetchRemote, 30000);
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) {
      void fetchRemote();
      refreshChanges();
    }
  });
  void fetchRemote();

  function showStatus(message, isError) {
    const status = workspace.querySelector('[data-action-status]');
    if (!status) return;
    status.hidden = false;
    status.classList.toggle('error', isError);
    status.textContent = message;
  }

  function hideStatus() {
    const status = workspace.querySelector('[data-action-status]');
    if (!status) return;
    status.hidden = true;
    status.classList.remove('error');
    status.textContent = '';
  }
})();
