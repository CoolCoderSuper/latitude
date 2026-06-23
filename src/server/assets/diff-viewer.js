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

  const openFilePaths = () => new Set(
    Array.from(workspace.querySelectorAll('details.file-card[open][data-file-path]'))
      .map((card) => card.dataset.filePath)
      .filter(Boolean),
  );

  const restoreOpenFiles = (paths) => {
    workspace.querySelectorAll('details.file-card[data-file-path]').forEach((card) => {
      if (paths.has(card.dataset.filePath)) {
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
    const button = actionButton(event.target);
    if (!button || !workspace.contains(button)) {
      return;
    }

    event.preventDefault();

    const action = button.dataset.gitAction;
    const body = new URLSearchParams({ action });
    if (button.dataset.path) {
      body.set('path', button.dataset.path);
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

    const openPaths = openFilePaths();
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
      restoreOpenFiles(openPaths);
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
