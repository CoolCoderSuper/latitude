import type { DeploymentKind } from './types';

export type ViewerState = {
  title: string;
  href: string;
  kind?: DeploymentKind;
  mediaType?: string | null;
};
export type ProjectTab = 'deployments' | 'code' | 'files' | 'terminal';
export type RootStackParamList = {
  Connect: undefined;
  Home: undefined;
  Project: {
    initialTab?: ProjectTab;
    name: string;
  };
  RootDesktop: undefined;
  RootTerminal: undefined;
  Servers: undefined;
  Viewer: ViewerState;
};
