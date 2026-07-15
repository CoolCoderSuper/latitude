(() => {
  const workspace = document.querySelector('[data-diff-workspace]');
  if (!workspace) return;

  const actionUrl = workspace.dataset.actionUrl || window.location.pathname;
  let openKeys = new Set();
  let refreshTimer = null;
  let autoRefreshPending = false;
  let commitMessage = '';
  let selectedPaths = {
    unstaged: new Set(),
    staged: new Set(),
  };
  let pointerInteractionActive = false;
  let interactionBlockedUntil = 0;
  let forceNextRefresh = false;

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
    if (
      (!forceNextRefresh && userIsInteracting())
      || !diffContentChanged(event.detail.xhr?.responseText || '')
    ) {
      forceNextRefresh = false;
      event.detail.shouldSwap = false;
      autoRefreshPending = false;
      hideStatus();
      return;
    }
    forceNextRefresh = false;
    commitMessage = workspace.querySelector('[data-commit-message]')?.value || '';
    captureSelections();
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
    restoreSelection();
  });

  workspace.addEventListener('click', (event) => {
    if (event.target.matches('[data-file-select]')) event.stopPropagation();
  });

  workspace.addEventListener('change', (event) => {
    if (!event.target.matches('[data-file-select]')) return;
    captureSelections();
    updateSelectionActions();
  });

  workspace.addEventListener('pointerdown', (event) => {
    if (event.target.closest('button, input, a, form')) return;
    pointerInteractionActive = true;
  });

  document.addEventListener('pointerup', () => {
    if (!pointerInteractionActive) return;
    pointerInteractionActive = false;
    blockRefreshFor(5000);
  });

  workspace.addEventListener('scroll', () => blockRefreshFor(3000), true);
  window.addEventListener('scroll', () => blockRefreshFor(3000), { passive: true });
  workspace.addEventListener('wheel', () => blockRefreshFor(3000), { passive: true });

  workspace.addEventListener('htmx:responseError', (event) => {
    autoRefreshPending = false;
    forceNextRefresh = false;
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
    restoreSelection();
    return true;
  }

  function restoreSelection() {
    const availablePaths = {
      unstaged: new Set(),
      staged: new Set(),
    };
    workspace.querySelectorAll('[data-file-select]').forEach((checkbox) => {
      const kind = checkbox.dataset.selectionKind;
      availablePaths[kind].add(checkbox.value);
      checkbox.checked = selectedPaths[kind].has(checkbox.value);
    });
    for (const kind of ['unstaged', 'staged']) {
      selectedPaths[kind] = new Set(
        Array.from(selectedPaths[kind]).filter((path) => availablePaths[kind].has(path)),
      );
    }
    updateSelectionActions();
  }

  function captureSelections() {
    for (const kind of ['unstaged', 'staged']) {
      selectedPaths[kind] = new Set(
        Array.from(workspace.querySelectorAll(
          `[data-file-select][data-selection-kind="${kind}"]:checked`,
        )).map((checkbox) => checkbox.value),
      );
    }
  }

  function updateSelectionActions() {
    updateSelectionAction('unstaged', 'stage', 'Stage');
    updateSelectionAction('staged', 'unstage', 'Unstage');
  }

  function updateSelectionAction(kind, actionVerb, labelVerb) {
    const button = workspace.querySelector(`[data-${actionVerb}-action]`);
    if (!button) return;
    const form = button.closest('form');
    form.querySelectorAll('[data-selected-path]').forEach((input) => input.remove());
    const paths = Array.from(workspace.querySelectorAll(
      `[data-file-select][data-selection-kind="${kind}"]:checked`,
    )).map((checkbox) => checkbox.value);
    for (const path of paths) {
      const input = document.createElement('input');
      input.type = 'hidden';
      input.name = 'path';
      input.value = path;
      input.dataset.selectedPath = '';
      form.append(input);
    }
    const count = paths.length;
    const action = count > 0 ? `${actionVerb}_selected` : `${actionVerb}_all`;
    button.value = action;
    button.dataset.gitAction = action;
    button.textContent = count > 0
      ? `${labelVerb} selected (${count})`
      : `${labelVerb} all`;
  }

  function diffContentChanged(responseText) {
    const incoming = new DOMParser().parseFromString(responseText, 'text/html');
    return diffSnapshot(workspace) !== diffSnapshot(incoming);
  }

  function diffSnapshot(root) {
    const overview = root.querySelector('.git-overview')?.outerHTML || '';
    const panels = Array.from(root.querySelectorAll('[data-file-panel]')).map((panel) => {
      const clone = panel.cloneNode(true);
      clone.querySelectorAll('details[open]').forEach((details) => details.removeAttribute('open'));
      clone.querySelectorAll('[data-file-select]').forEach((checkbox) => {
        checkbox.checked = false;
      });
      const button = clone.querySelector('[data-stage-action]');
      if (button) {
        button.value = 'stage_all';
        button.dataset.gitAction = 'stage_all';
        button.textContent = 'Stage all';
      }
      return clone.outerHTML;
    });
    return `${overview}\n${panels.join('\n')}`;
  }

  function blockRefreshFor(milliseconds) {
    interactionBlockedUntil = Math.max(interactionBlockedUntil, Date.now() + milliseconds);
  }

  function userIsInteracting() {
    const selection = window.getSelection();
    const selectionIsInsideWorkspace = Boolean(
      selection
      && !selection.isCollapsed
      && selection.anchorNode
      && workspace.contains(selection.anchorNode),
    );
    return (
      pointerInteractionActive
      || Date.now() < interactionBlockedUntil
      || selectionIsInsideWorkspace
    );
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
    autoRefreshPending = false;
    forceNextRefresh = true;
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
      || userIsInteracting()
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
  restoreSelection();

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
