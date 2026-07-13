(() => {
  const shell = document.querySelector('[data-project-shell]');
  const dialog = shell?.querySelector('[data-share-dialog]');
  if (!shell || !dialog) return;

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
