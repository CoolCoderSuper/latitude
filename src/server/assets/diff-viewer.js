const workspace = document.querySelector('[data-diff-workspace]');

if (workspace) {
  const actionUrl = workspace.dataset.actionUrl || window.location.pathname;
  const statusBox = () => workspace.querySelector('[data-action-status]');

  const showStatus = (message, isError) => {
    const box = statusBox();
    if (!box) {
      return;
    }

    box.hidden = false;
    box.textContent = message;
    box.classList.toggle('error', Boolean(isError));
  };

  const hideStatus = () => {
    const box = statusBox();
    if (!box) {
      return;
    }

    box.hidden = true;
    box.textContent = '';
    box.classList.remove('error');
  };

  const fileCardKey = (card) => {
    const section = card.dataset.fileSection;
    const path = card.dataset.filePath;
    return section && path ? `${section}:${path}` : null;
  };

  const fileActionSection = (action) => {
    if (action === 'stage_file' || action === 'discard_file') {
      return 'unstaged';
    }

    if (action === 'unstage_file') {
      return 'staged';
    }

    return null;
  };

  const openFileKeys = () => new Set(
    Array.from(workspace.querySelectorAll(
      'details.file-card[open][data-file-section][data-file-path]',
    ))
      .map(fileCardKey)
      .filter(Boolean),
  );

  const restoreOpenFiles = (keys) => {
    workspace
      .querySelectorAll('details.file-card[data-file-section][data-file-path]')
      .forEach((card) => {
        const key = fileCardKey(card);
        if (key && keys.has(key)) {
          card.open = true;
        }
      });
  };

  const setDisabled = (disabled) => {
    workspace
      .querySelectorAll('button[data-git-action], input[data-commit-message]')
      .forEach((control) => {
        control.disabled = disabled;
      });
  };

  const actionButton = (target) => (
    target instanceof Element ? target.closest('button[data-git-action]') : null
  );

  workspace.addEventListener('keydown', (event) => {
    if (
      event.key !== 'Enter'
      || !(event.target instanceof Element)
      || !event.target.matches('[data-commit-message]')
    ) {
      return;
    }

    event.preventDefault();
    workspace.querySelector('button[data-git-action="commit"]')?.click();
  });

  workspace.addEventListener('click', async (event) => {
    const editorLink = event.target instanceof Element
      ? event.target.closest('a[data-open-editor]')
      : null;
    if (editorLink) {
      event.stopPropagation();
      return;
    }
    const button = actionButton(event.target);
    if (!button || !workspace.contains(button)) {
      return;
    }

    event.preventDefault();
    event.stopPropagation();

    const action = button.dataset.gitAction;
    const actionPath = button.dataset.path;
    const confirmMessage = button.dataset.confirm;
    if (confirmMessage && !window.confirm(confirmMessage)) {
      return;
    }

    const body = new URLSearchParams({ action });
    if (actionPath) {
      body.set('path', actionPath);
    }

    if (action === 'commit') {
      const input = workspace.querySelector('[data-commit-message]');
      const message = input ? input.value.trim() : '';
      if (!message) {
        showStatus('Commit message required.', true);
        input?.focus();
        return;
      }

      body.set('message', message);
    }

    const openKeys = openFileKeys();
    const scrollY = window.scrollY;

    workspace.setAttribute('aria-busy', 'true');
    setDisabled(true);
    showStatus('Working...', false);

    try {
      const response = await fetch(actionUrl, {
        method: 'PATCH',
        headers: {
          Accept: 'application/json',
          'Content-Type': 'application/x-www-form-urlencoded;charset=UTF-8',
        },
        body,
      });
      const payload = await response.json().catch(() => null);
      if (!payload || typeof payload.workspace_html !== 'string') {
        throw new Error(`Action failed (${response.status}).`);
      }

      workspace.innerHTML = payload.workspace_html;
      if (payload.ok) {
        const actionSection = fileActionSection(action);
        if (actionSection && actionPath) {
          openKeys.delete(`${actionSection}:${actionPath}`);
        }
      }
      restoreOpenFiles(openKeys);
      window.scrollTo(0, scrollY);

      if (payload.ok) {
        hideStatus();
      } else {
        showStatus(payload.error || 'Action failed.', true);
      }
    } catch (error) {
      showStatus(error instanceof Error ? error.message : 'Action failed.', true);
    } finally {
      workspace.removeAttribute('aria-busy');
      setDisabled(false);
    }
  });
}
