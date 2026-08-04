import type { ThemeColors, ThemeMode } from '../../theme';

export function deploymentThemeScript(
  mode: ThemeMode,
  colors: ThemeColors,
): string {
  const theme = {
    mode,
    variables: {
      '--latitude-page-bg': colors.background,
      '--latitude-page-text': colors.text,
      '--latitude-page-heading': colors.text,
      '--latitude-page-muted': colors.softText,
      '--latitude-page-accent': colors.accent,
      '--latitude-page-inline-code-bg': colors.panel,
      '--latitude-page-code-bg': colors.codeBg,
      '--latitude-page-code-text': colors.codeText,
      '--latitude-page-border': colors.border,
    },
  };

  return `
(function() {
  var theme = ${JSON.stringify(theme)};
  var applyTheme = function() {
    var root = document.documentElement;
    if (!root) {
      return;
    }

    root.dataset.latitudeTheme = theme.mode;
    root.style.colorScheme = theme.mode;

    Object.keys(theme.variables).forEach(function(name) {
      root.style.setProperty(name, theme.variables[name]);
    });

    var style = document.getElementById('latitude-mobile-deployment');
    if (!style) {
      style = document.createElement('style');
      style.id = 'latitude-mobile-deployment';
      (document.head || root).appendChild(style);
    }
    style.textContent = '.latitude-page-header { display: none !important; }';
  };

  applyTheme();
  document.addEventListener('DOMContentLoaded', applyTheme);
})();
true;
`;
}
