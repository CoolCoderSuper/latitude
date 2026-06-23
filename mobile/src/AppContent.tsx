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
import { ServersScreen } from './screens/ServersScreen';
import {
  activateSession,
  loadBaseUrl,
  loadSession,
  loadSessions,
  removeSession,
  saveBaseUrl,
  saveSession,
} from './storage';
import { useTheme } from './theme';
import type { ProjectSummary, SessionRecord } from './types';
import { errorMessage } from './utils/errors';

const Stack = createNativeStackNavigator<RootStackParamList>();

export function AppContent() {
  const { colors, mode } = useTheme();
  const [booting, setBooting] = useState(true);
  const [rememberedBaseUrl, setRememberedBaseUrl] = useState(DEFAULT_BASE_URL);
  const [session, setSession] = useState<SessionRecord | null>(null);
  const [sessions, setSessions] = useState<SessionRecord[]>([]);
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
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

  const signOut = useCallback(async (): Promise<SessionRecord | null> => {
    if (!session) {
      return null;
    }

    const signedOutBaseUrl = session.baseUrl;
    const nextState = await removeSession(signedOutBaseUrl);
    setSessions(nextState.sessions);
    setSession(nextState.activeSession);
    setRememberedBaseUrl(nextState.activeSession?.baseUrl ?? signedOutBaseUrl);
    setProjects([]);
    setError(null);
    return nextState.activeSession;
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
      }
    } catch (loadError) {
      if (activeBaseUrlRef.current !== session.baseUrl) {
        return;
      }

      if (loadError instanceof LatitudeApiError && loadError.status === 401) {
        const nextSession = await signOut();
        if (!nextSession) {
          setError('Sign in again to continue.');
        }
      } else {
        setError(errorMessage(loadError));
      }
    } finally {
      if (activeBaseUrlRef.current === session.baseUrl) {
        setProjectsLoading(false);
        setBooting(false);
      }
    }
  }, [api, session, signOut]);

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
    };
    const nextSessions = await saveSession(nextSession);
    setSessions(nextSessions);
    setSession(nextSession);
    setProjects([]);
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
      }
      setError(null);
    },
    [session?.baseUrl],
  );

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
                error={error}
                loading={projectsLoading}
                projects={projects}
                serverSessions={sessions}
                onManageServers={() => navigation.navigate('Servers')}
                onOpenProject={(name) => navigation.navigate('Project', { name })}
                onRefresh={() => {
                  void loadProjects();
                }}
                onSwitchServer={handleSwitchServer}
                onSignOut={() => {
                  void signOut();
                }}
              />
            )}
          </Stack.Screen>
          <Stack.Screen name="Project">
            {({ navigation, route }) => (
              <ProjectRoute
                api={api}
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
          <Stack.Screen name="Servers">
            {({ navigation }) => (
              <ServersScreen
                activeBaseUrl={session.baseUrl}
                sessions={sessions}
                onAddServer={() => navigation.navigate('Connect')}
                onBack={() => navigation.goBack()}
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
