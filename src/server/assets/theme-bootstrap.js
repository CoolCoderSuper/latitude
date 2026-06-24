(function() {
  var cookieName = "__LATITUDE_THEME_COOKIE__";

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

  var root = document.documentElement;
  var theme = cleanTheme(root.dataset.latitudeTheme) || cookieTheme() || 'light';
  root.dataset.latitudeTheme = theme;
  root.style.colorScheme = theme;
})();
