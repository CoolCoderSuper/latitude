(() => {
  const shell = document.querySelector('[data-project-shell], [data-server-shell]');
  const dialog = shell?.querySelector('[data-share-dialog]');
  if (!shell) return;

  let gitRefreshPending = false;
  const projectName = shell.dataset.project;

  async function refreshGitStatuses(fetchRemote = false) {
    if (gitRefreshPending || document.hidden) return;
    gitRefreshPending = true;
    const endpoint = projectName
      ? `/__latitude/api/projects/${encodeURIComponent(projectName)}`
      : '/__latitude/api/projects';
    try {
      const response = await fetch(`${endpoint}${fetchRemote ? '?fetch=1' : ''}`, {
        credentials: 'same-origin',
      });
      if (!response.ok) return;
      const payload = await response.json();
      const projects = projectName ? [payload] : payload.projects;
      projects.forEach((project) => {
        shell.querySelectorAll('[data-project-git-status]').forEach((container) => {
          if (container.dataset.projectGitStatus === project.name) {
            renderGitStatus(container, project);
          }
        });
      });
    } catch {
      // Keep the current status visible while the server or remote is unavailable.
    } finally {
      gitRefreshPending = false;
    }
  }

  function renderGitStatus(container, project) {
    container.replaceChildren();
    if (!project.git_dirty && project.git_ahead === 0 && project.git_behind === 0) return;

    const badge = document.createElement('span');
    badge.className = 'git-status';
    const labels = [];
    if (project.git_dirty) {
      if (project.git_additions > 0) {
        labels.push(`${project.git_additions} additions`);
        appendStat(badge, 'git-additions', `+${project.git_additions}`);
      }
      if (project.git_deletions > 0) {
        labels.push(`${project.git_deletions} deletions`);
        appendStat(badge, 'git-deletions', `-${project.git_deletions}`);
      }
      if (project.git_additions === 0 && project.git_deletions === 0) {
        labels.push('working tree changes');
        appendStat(badge, '', 'changed');
      }
    }
    if (project.git_behind > 0) {
      labels.push(commitLabel(project.git_behind, 'pull'));
      appendStat(badge, 'git-behind', `↓${project.git_behind}`, 'Commits to pull');
    }
    if (project.git_ahead > 0) {
      labels.push(commitLabel(project.git_ahead, 'push'));
      appendStat(badge, 'git-ahead', `↑${project.git_ahead}`, 'Commits to push');
    }
    badge.setAttribute('aria-label', labels.join(', '));
    badge.title = labels.join(', ');
    container.append(badge);
  }

  function appendStat(badge, className, text, title) {
    const stat = document.createElement('span');
    stat.className = `git-stat ${className}`;
    stat.textContent = text;
    if (title) stat.title = title;
    badge.append(stat);
  }

  function commitLabel(count, action) {
    return `${count} ${count === 1 ? 'commit' : 'commits'} to ${action}`;
  }

  window.setInterval(() => void refreshGitStatuses(false), 2000);
  window.setInterval(() => void refreshGitStatuses(true), 30000);
  document.addEventListener('visibilitychange', () => {
    if (!document.hidden) void refreshGitStatuses(true);
  });
  void refreshGitStatuses(true);

  if (!dialog) return;

  shell.addEventListener('click', async (event) => {
    const target = event.target instanceof Element ? event.target : null;
    const trigger = target?.closest('[data-share-trigger]');
    if (trigger) {
      dialog.showModal();
      return;
    }

    if (target?.closest('[data-share-close]')) {
      dialog.close();
      return;
    }

    const shareButton = target?.closest('[data-share-url]');
    if (!shareButton) return;

    const url = new URL(shareButton.dataset.shareUrl, window.location.origin).href;
    try {
      if (navigator.share) {
        await navigator.share({ title: 'Latitude share link', url });
      } else if (navigator.clipboard?.writeText) {
        await navigator.clipboard.writeText(url);
        showStatus('Share link copied to the clipboard.', false);
      } else {
        window.prompt('Copy this share link', url);
      }
    } catch (error) {
      if (error?.name !== 'AbortError') showStatus('The share link could not be shared.', true);
    }
  });

  dialog.addEventListener('click', (event) => {
    if (event.target === dialog) dialog.close();
  });

  dialog.addEventListener('htmx:afterSwap', localizeExpiryTimes);
  dialog.addEventListener('htmx:responseError', () => {
    showStatus('Latitude could not update the share links.', true);
  });

  function localizeExpiryTimes() {
    dialog.querySelectorAll('[data-share-expires-at]').forEach((element) => {
      const timestamp = Number(element.dataset.shareExpiresAt);
      if (Number.isFinite(timestamp)) {
        element.textContent = `Expires ${new Date(timestamp * 1000).toLocaleString()}`;
      }
    });
  }

  function showStatus(message, isError) {
    const panel = dialog.querySelector('[data-share-dialog-shell]');
    if (!panel) return;
    let status = panel.querySelector('[data-share-status]');
    if (!status) {
      status = document.createElement('div');
      status.className = 'share-status';
      status.dataset.shareStatus = '';
      status.setAttribute('aria-live', 'polite');
      panel.querySelector('.share-dialog-header')?.after(status);
    }
    status.dataset.tone = isError ? 'error' : 'success';
    status.textContent = message;
  }
})();
