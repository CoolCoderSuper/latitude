import { lightColors } from '../../ui/theme/tokens';
import { deploymentThemeScript } from './deploymentDocument';

describe('deploymentThemeScript', () => {
  it('hides the website page header inside the mobile viewer', () => {
    const script = deploymentThemeScript('light', lightColors);

    expect(script).toContain('.latitude-page-header');
    expect(script).toContain('display: none !important');
  });
});
