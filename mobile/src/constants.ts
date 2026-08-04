import type { ProjectTab } from './navigationTypes';
import type { RootTerminalLink } from './types';

export const DEFAULT_BASE_URL = 'http://127.0.0.1:8080';
export const ANDROID_EMULATOR_URL = 'http://10.0.2.2:8080';
export const PROJECT_TABS: ProjectTab[] = [
  'deployments',
  'code',
  'files',
  'terminal',
];

export const DEFAULT_ROOT_TERMINAL: RootTerminalLink = {
  href: '/_terminal',
  api_href: '/__latitude/api/terminal',
  label: 'Root Terminal',
  description: 'Run commands in your user directory',
};
