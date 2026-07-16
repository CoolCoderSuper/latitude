(function() {
  var cookieName = "latitude_theme";

  function hasCookie(name, value) {
    return (document.cookie ? document.cookie.split(';') : []).some(function(part) {
      return part.trim() === name + '=' + value;
    });
  }

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
  if (window.self !== window.top && hasCookie('latitude_t3code_embed_session', '1')) {
    root.dataset.latitudeT3codeEmbed = 'true';
  }
  var theme = cleanTheme(root.dataset.latitudeTheme) || cookieTheme() || 'light';
  root.dataset.latitudeTheme = theme;
  root.style.colorScheme = theme;
})();
