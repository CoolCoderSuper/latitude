import { act, fireEvent, render, waitFor } from '@testing-library/react-native';
import { Alert } from 'react-native';

import type { LatitudePublicApi } from '../../api';
import type { DeploymentSummary } from '../../types';
import { DeploymentsPanel } from './DeploymentsPanel';

jest.mock('../../theme', () => ({
  useRefreshControl: () => undefined,
  useTheme: () => ({
    colors: {
      accent: '#087',
      danger: '#b00',
      muted: '#777',
      onAccent: '#fff',
      text: '#111',
    },
    styles: new Proxy({}, { get: () => ({}) }),
  }),
}));

jest.mock('./ShareManagerModal', () => ({
  ShareManagerModal: () => null,
}));

const activeDeployment: DeploymentSummary = {
  name: 'website',
  href: '/demo/website',
  kind: 'reverse_proxy',
  label: 'Web application',
  media_type: null,
  title: null,
};

const archivedDeployment: DeploymentSummary = {
  ...activeDeployment,
  name: 'draft',
  href: '/demo/draft',
};

describe('DeploymentsPanel', () => {
  afterEach(() => {
    jest.restoreAllMocks();
  });

  it('archives active deployments after confirmation', async () => {
    const setDeploymentArchived = jest.fn().mockResolvedValue(undefined);
    const onRefresh = jest.fn().mockResolvedValue(undefined);
    const alert = jest.spyOn(Alert, 'alert').mockImplementation(() => {});
    const api = { setDeploymentArchived } as unknown as LatitudePublicApi;
    const result = await render(
      <DeploymentsPanel
        api={api}
        archivedDeployments={[]}
        baseUrl="http://latitude"
        deployments={[activeDeployment]}
        onOpenViewer={jest.fn()}
        onRefresh={onRefresh}
        projectName="demo"
        refreshing={false}
      />,
    );

    await fireEvent.press(result.getByLabelText('Archive website'));
    const buttons = alert.mock.calls[0][2];
    await act(async () => {
      buttons?.find((button) => button.text === 'Archive')?.onPress?.();
      await Promise.resolve();
    });

    await waitFor(() =>
      expect(setDeploymentArchived).toHaveBeenCalledWith(
        'demo',
        'website',
        true,
      ),
    );
    expect(onRefresh).toHaveBeenCalled();
  });

  it('shows and restores archived deployments', async () => {
    const setDeploymentArchived = jest.fn().mockResolvedValue(undefined);
    const api = { setDeploymentArchived } as unknown as LatitudePublicApi;
    const result = await render(
      <DeploymentsPanel
        api={api}
        archivedDeployments={[archivedDeployment]}
        baseUrl="http://latitude"
        deployments={[]}
        onOpenViewer={jest.fn()}
        onRefresh={jest.fn()}
        projectName="demo"
        refreshing={false}
      />,
    );

    await fireEvent.press(result.getByText('View archived (1)'));
    await fireEvent.press(await result.findByLabelText('Restore draft'));

    await waitFor(() =>
      expect(setDeploymentArchived).toHaveBeenCalledWith(
        'demo',
        'draft',
        false,
      ),
    );
  });
});
