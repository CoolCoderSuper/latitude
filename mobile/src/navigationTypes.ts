import type { DeploymentKind } from './types';

export type ViewerState = {
  title: string;
  href: string;
  kind?: DeploymentKind;
  mediaType?: string | null;
};
export type ProjectTab = 'deployments' | 'code' | 'terminal';
export type RootStackParamList = {
  Home: undefined;
  Project: {
    initialTab?: ProjectTab;
    name: string;
  };
  Viewer: ViewerState;
};
