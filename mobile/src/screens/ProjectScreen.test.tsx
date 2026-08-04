import { fireEvent, render } from '@testing-library/react-native';
import { useState } from 'react';

import { LatitudePublicApi } from '../api';
import type { ProjectDetail, SessionRecord } from '../types';
import { ProjectScreen } from './ProjectScreen';

jest.mock('../theme', () => ({
  useTheme: () => ({
    colors: {
      accent: '#087',
      muted: '#777',
      onAccent: '#fff',
      text: '#111',
    },
    styles: new Proxy({}, { get: () => ({}) }),
  }),
}));

jest.mock('../features/deployments/DeploymentsPanel', () => {
  const { Text } = jest.requireActual('react-native');
  return {
    DeploymentsPanel: () => <Text testID="deployments-panel">Deployments</Text>,
  };
});
jest.mock('../features/git/DiffPanel', () => {
  const { Text } = jest.requireActual('react-native');
  return { DiffPanel: () => <Text testID="diff-panel">Diff</Text> };
});
jest.mock('../features/files/FilesPanel', () => {
  const { forwardRef } = jest.requireActual('react');
  const { Text } = jest.requireActual('react-native');
  const FilesPanel = forwardRef(function MockFilesPanel() {
    return <Text testID="files-panel">Files</Text>;
  });
  return { FilesPanel };
});
jest.mock('../features/terminal/TerminalPanel', () => {
  const { Text } = jest.requireActual('react-native');
  return {
    TerminalPanel: () => (
      <Text testID="terminal-panel">Terminal workspace</Text>
    ),
  };
});

const session: SessionRecord = {
  baseUrl: 'http://latitude',
  token: 'token',
};

const project: ProjectDetail = {
  name: 'Latitude',
  device_hostname: 'latitude-box',
  href: '/Latitude',
  api_href: '/api/Latitude',
  summary: 'Project',
  deployment_count: 0,
  git_dirty: false,
  git_additions: 0,
  git_deletions: 0,
  git_ahead: 0,
  git_behind: 0,
  diff: {
    href: '/diff',
    api_href: '/api/diff',
    label: 'Diff',
    description: '',
  },
  terminal: {
    href: '/terminal',
    api_href: '/api/terminal',
    label: 'Terminal',
    description: '',
  },
  deployments: [],
};

function ProjectHarness() {
  const [tab, setTab] = useState<'deployments' | 'code' | 'files' | 'terminal'>(
    'deployments',
  );
  return (
    <ProjectScreen
      api={new LatitudePublicApi(session.baseUrl, session.token)}
      onBack={jest.fn()}
      onOpenGitHistory={jest.fn()}
      onOpenViewer={jest.fn()}
      onRefresh={jest.fn()}
      onSelectTab={setTab}
      project={project}
      projectLoading={false}
      session={session}
      tab={tab}
    />
  );
}

describe('ProjectScreen tabs', () => {
  it('does not mount expensive feature panels until their tab is visited', async () => {
    const result = await render(
      <ProjectScreen
        api={new LatitudePublicApi(session.baseUrl, session.token)}
        onBack={jest.fn()}
        onOpenGitHistory={jest.fn()}
        onOpenViewer={jest.fn()}
        onRefresh={jest.fn()}
        onSelectTab={jest.fn()}
        project={project}
        projectLoading={false}
        session={session}
        tab="deployments"
      />,
    );

    expect(result.getByTestId('deployments-panel')).toBeTruthy();
    expect(result.queryByTestId('diff-panel')).toBeNull();
    expect(result.queryByTestId('files-panel')).toBeNull();
    expect(result.queryByTestId('terminal-panel')).toBeNull();
  });

  it('mounts a panel when its tab is first visited', async () => {
    const result = await render(<ProjectHarness />);
    await fireEvent.press(result.getByText('Terminal'));
    expect(result.getByTestId('terminal-panel')).toBeTruthy();
  });
});
