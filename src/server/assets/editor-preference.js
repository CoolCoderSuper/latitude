(function () {
  const cookieName = 'latitude_editor';
  function preference() {
    return document.cookie
      .split(';')
      .some((part) => part.trim() === `${cookieName}=neovim`)
      ? 'neovim'
      : 'files';
  }
  function apply() {
    const editor = preference();
    document.documentElement.dataset.latitudeEditor = editor;
    document.querySelectorAll('[data-editor-preference]').forEach((input) => {
      input.checked = input.value === editor;
    });
  }
  apply();
  document.addEventListener('DOMContentLoaded', () => {
    apply();
    const button = document.querySelector('[data-settings-open]');
    const dialog = document.querySelector('[data-settings-dialog]');
    if (!button || !dialog) return;
    if (document.documentElement.dataset.latitudeT3codeEmbed === 'true') {
      button.remove();
      dialog.remove();
      return;
    }
    const header = document.querySelector('body header');
    if (header) {
      header.classList.add('latitude-settings-header');
      header.appendChild(button);
    }
    button.addEventListener('click', () => {
      apply();
      dialog.showModal();
    });
    dialog
      .querySelector('[data-settings-close]')
      .addEventListener('click', () => dialog.close());
    dialog.addEventListener('click', (event) => {
      const rect = dialog.getBoundingClientRect();
      if (
        event.target === dialog &&
        (event.clientX < rect.left ||
          event.clientX > rect.right ||
          event.clientY < rect.top ||
          event.clientY > rect.bottom)
      )
        dialog.close();
    });
  });
  window.addEventListener('focus', apply);
  document.addEventListener('change', (event) => {
    if (!event.target.matches('[data-editor-preference]')) return;
    const editor = event.target.value === 'neovim' ? 'neovim' : 'files';
    document.cookie = `${cookieName}=${editor}; Path=/; Max-Age=31536000; SameSite=Lax`;
    apply();
  });
})();
