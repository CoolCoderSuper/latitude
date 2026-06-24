(function() {
  var cookieName = "__LATITUDE_THEME_COOKIE__";
  var toggle = document.querySelector('[data-latitude-theme-toggle]');

  function cleanTheme(value) {
    return value === 'light' || value === 'dark' ? value : null;
  }

  function cookieTheme() {
    var prefix = cookieName + '=';
    var parts = document.cookie ? document.cookie.split(';') : [];
    for (var index = 0; index < parts.length; index += 1) {
      var part = parts[index].trim();
      if (part.indexOf(prefix) === 0) {
        return cleanTheme(part.slice(prefix.length));
      }
    }
    return null;
  }

  function persistTheme(theme) {
    document.cookie = cookieName + '=' + theme + '; Path=/; Max-Age=31536000; SameSite=Lax';
  }

  function placeToggle() {
    if (!toggle) {
      return;
    }

    var header = document.querySelector('body header');
    if (header && toggle.parentElement !== header) {
      header.classList.add('latitude-theme-header');
      header.appendChild(toggle);
    }
  }

  function applyTheme(theme, persist) {
    var clean = cleanTheme(theme) || 'light';
    var root = document.documentElement;
    root.dataset.latitudeTheme = clean;
    root.style.colorScheme = clean;
    if (toggle) {
      var next = clean === 'dark' ? 'light' : 'dark';
      toggle.dataset.latitudeThemeCurrent = clean;
      toggle.setAttribute('aria-label', next === 'dark' ? 'Use dark mode' : 'Use light mode');
      toggle.title = next === 'dark' ? 'Use dark mode' : 'Use light mode';
    }
    if (persist) {
      persistTheme(clean);
    }
  }

  placeToggle();
  applyTheme(cleanTheme(document.documentElement.dataset.latitudeTheme) || cookieTheme(), false);

  if (toggle) {
    toggle.addEventListener('click', function() {
      var current = cleanTheme(document.documentElement.dataset.latitudeTheme) || 'light';
      applyTheme(current === 'dark' ? 'light' : 'dark', true);
    });
  }
})();
