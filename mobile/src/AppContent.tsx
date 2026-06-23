import {
  DarkTheme,
  DefaultTheme,
  NavigationContainer,
} from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import { useCallback, useEffect, useMemo, useRef, useState } from 'react';

import {
  LatitudeApiError,
  LatitudePublicApi,
  normalizeBaseUrl,
} from './api';
import { DEFAULT_BASE_URL } from './constants';
import { Shell, LoadingScreen } from './components/Shell';
import { DeploymentViewer } from './features/deployments/DeploymentViewer';
import type { RootStackParamList } from './navigationTypes';
import { ConnectScreen } from './screens/ConnectScreen';
import { HomeScreen } from './screens/HomeScreen';
import { ProjectRoute } from './screens/ProjectRoute';
import { RootTerminalScreen } from './screens/RootTerminalScreen';
import { ServersScreen } from './screens/ServersScreen';
import {
  activateSession,
  loadBaseUrl,
  loadSession,
  loadSessions,
  removeSession,
  requireSessionLogin,
  saveBaseUrl,
  saveSessionOrder,
  saveSession,
} from './storage';
import { useTheme } from './theme';
import type { ProjectSummary, RootTerminalLink, SessionRecord } from './types';
import { errorMessage } from './utils/errors';

const Stack = createNativeStackNavigator<RootStackParamList>();
const DEFAULT_ROOT_TERMINAL: RootTerminalLink = {
  href: '/_terminal',
  api_href: '/__latitude/api/terminal',
  label: 'Root Terminal',
  description: 'Run commands in your user directory',
};

export function AppContent() {
  const { colors, mode } = useTheme();
  const [booting, setBooting] = useState(true);
  const [rememberedBaseUrl, setRememberedBaseUrl] = useState(DEFAULT_BASE_URL);
  const [session, setSession] = useState<SessionRecord | null>(null);
  const [sessions, setSessions] = useState<SessionRecord[]>([]);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [rootTerminal, setRootTerminal] = useState<RootTerminalLink>(
    DEFAULT_ROOT_TERMINAL,
  );
  const [projectsLoading, setProjectsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const activeBaseUrlRef = useRef<string | null>(null);

  const api = useMemo(
    () => new LatitudePublicApi(session?.baseUrl ?? '', session?.token),
    [session],
  );

  useEffect(() => {
    activeBaseUrlRef.current = session?.baseUrl ?? null;
  }, [session]);

  const loadProjects = useCallback(async () => {
    if (!session) {
      return;
    }

    setProjectsLoading(true);
    setError(null);
    try {
      const response = await api.projects();
      if (activeBaseUrlRef.current === session.baseUrl) {
        setProjects(response.projects);
        setRootTerminal(response.root_terminal ?? DEFAULT_ROOT_TERMINAL);
        if (
          response.device_hostname &&
          session.deviceHostname !== response.device_hostname
        ) {
          const nextSession = {
            ...session,
            deviceHostname: response.device_hostname,
          };
          setSession(nextSession);
          setSessions(await saveSession(nextSession));
        }
      }
    } catch (loadError) {
      if (activeBaseUrlRef.current !== session.baseUrl) {
        return;
      }

      if (loadError instanceof LatitudeApiError && loadError.status === 401) {
        setSessions(await requireSessionLogin(session));
        setRememberedBaseUrl(session.baseUrl);
        setSession(null);
        setProjects([]);
        setRootTerminal(DEFAULT_ROOT_TERMINAL);
        setError('Sign in again to continue.');
      } else {
        setError(errorMessage(loadError));
      }
    } finally {
      if (activeBaseUrlRef.current === session.baseUrl) {
        setProjectsLoading(false);
        setBooting(false);
      }
    }
  }, [api, session]);

  useEffect(() => {
    let mounted = true;

    Promise.all([loadSession(), loadBaseUrl(), loadSessions()])
      .then(([storedSession, storedBaseUrl, storedSessions]) => {
        if (mounted) {
          setSessions(storedSessions);
          setRememberedBaseUrl(
            storedSession?.baseUrl ?? storedBaseUrl ?? DEFAULT_BASE_URL,
          );
          setSession(storedSession);
          if (!storedSession) {
            setBooting(false);
          }
        }
      })
      .catch((storageError) => {
        if (mounted) {
          setError(errorMessage(storageError));
          setBooting(false);
        }
      });

    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    if (session) {
      void loadProjects();
    }
  }, [loadProjects, session]);

  const handleLogin = useCallback(async (baseUrl: string, password: string) => {
    const normalizedBaseUrl = normalizeBaseUrl(baseUrl);
    await saveBaseUrl(normalizedBaseUrl);
    setRememberedBaseUrl(normalizedBaseUrl);
    const loginApi = new LatitudePublicApi(normalizedBaseUrl);
    const response = await loginApi.login(password);
    const nextSession = {
      baseUrl: normalizedBaseUrl,
      token: response.token,
      deviceHostname: response.device_hostname,
    };
    const nextSessions = await saveSession(nextSession);
    setSessions(nextSessions);
    setSession(nextSession);
    setProjects([]);
    setRootTerminal(response.root_terminal ?? DEFAULT_ROOT_TERMINAL);
    setError(null);
  }, []);

  const handleSwitchServer = useCallback(async (baseUrl: string) => {
    const nextSession = await activateSession(baseUrl);
    if (!nextSession) {
      return;
    }

    setSession(nextSession);
    setRememberedBaseUrl(nextSession.baseUrl);
    setProjects([]);
    setRootTerminal(DEFAULT_ROOT_TERMINAL);
    setError(null);
  }, []);

  const handleRemoveServer = useCallback(
    async (baseUrl: string) => {
      const previousBaseUrl = session?.baseUrl ?? null;
      const nextState = await removeSession(baseUrl);
      const nextBaseUrl = nextState.activeSession?.baseUrl ?? null;

      setSessions(nextState.sessions);
      setSession(nextState.activeSession);
      setRememberedBaseUrl(nextState.activeSession?.baseUrl ?? baseUrl);
      if (nextBaseUrl !== previousBaseUrl) {
        setProjects([]);
        setRootTerminal(DEFAULT_ROOT_TERMINAL);
      }
      setError(null);
    },
    [session?.baseUrl],
  );

  const handleReorderServers = useCallback(async (nextSessions: SessionRecord[]) => {
    setSessions(await saveSessionOrder(nextSessions));
  }, []);

  if (booting) {
    return <Shell><LoadingScreen /></Shell>;
  }

  if (!session) {
    return (
      <Shell>
        <ConnectScreen
          error={error}
          initialBaseUrl={rememberedBaseUrl}
          onLogin={handleLogin}
          onClearError={() => setError(null)}
        />
      </Shell>
    );
  }

  const baseNavigationTheme = mode === 'dark' ? DarkTheme : DefaultTheme;
  const navigationTheme = {
    ...baseNavigationTheme,
    colors: {
      ...baseNavigationTheme.colors,
      background: colors.background,
      border: colors.border,
      card: colors.surface,
      primary: colors.accent,
      text: colors.text,
    },
  };

  return (
    <Shell>
      <NavigationContainer key={session.baseUrl} theme={navigationTheme}>
        <Stack.Navigator screenOptions={{ headerShown: false }}>
          <Stack.Screen name="Home">
            {({ navigation }) => (
              <HomeScreen
                baseUrl={session.baseUrl}
                deviceHostname={session.deviceHostname}
                error={error}
                loading={projectsLoading}
                projects={projects}
                rootTerminal={rootTerminal}
                serverSessions={sessions}
                onManageServers={() => navigation.navigate('Servers')}
                onOpenProject={(name) => navigation.navigate('Project', { name })}
                onOpenRootTerminal={() => navigation.navigate('RootTerminal')}
                onRefresh={() => {
                  void loadProjects();
                }}
                onSwitchServer={handleSwitchServer}
              />
            )}
          </Stack.Screen>
          <Stack.Screen name="Project">
            {({ navigation, route }) => (
              <ProjectRoute
                api={api}
                deviceHostname={session.deviceHostname}
                initialTab={route.params.initialTab ?? 'deployments'}
                projectName={route.params.name}
                session={session}
                onBack={() => navigation.goBack()}
                onOpenViewer={(deployment) =>
                  navigation.navigate('Viewer', {
                    href: deployment.href,
                    kind: deployment.kind,
                    mediaType: deployment.media_type,
                    title: deployment.title ?? deployment.name,
                  })
                }
              />
            )}
          </Stack.Screen>
          <Stack.Screen name="RootTerminal">
            {({ navigation }) => (
              <RootTerminalScreen
                api={api}
                deviceHostname={session.deviceHostname}
                rootTerminal={rootTerminal}
                session={session}
                onBack={() => navigation.goBack()}
              />
            )}
          </Stack.Screen>
          <Stack.Screen name="Servers">
            {({ navigation }) => (
              <ServersScreen
                activeBaseUrl={session.baseUrl}
                deviceHostname={session.deviceHostname}
                sessions={sessions}
                onAddServer={() => navigation.navigate('Connect')}
                onBack={() => navigation.goBack()}
                onReorderServers={handleReorderServers}
                onRemoveServer={handleRemoveServer}
                onSwitchServer={handleSwitchServer}
              />
            )}
          </Stack.Screen>
          <Stack.Screen name="Connect">
            {({ navigation }) => (
              <ConnectScreen
                error={error}
                initialBaseUrl={rememberedBaseUrl}
                onCancel={() => navigation.goBack()}
                onLogin={handleLogin}
                onClearError={() => setError(null)}
              />
            )}
          </Stack.Screen>
          <Stack.Screen name="Viewer">
            {({ navigation, route }) => (
              <DeploymentViewer
                baseUrl={session.baseUrl}
                deviceHostname={session.deviceHostname}
                token={session.token}
                viewer={route.params}
                onBack={() => navigation.goBack()}
              />
            )}
          </Stack.Screen>
        </Stack.Navigator>
      </NavigationContainer>
    </Shell>
  );
}
