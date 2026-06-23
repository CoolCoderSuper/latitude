import {
  DarkTheme,
  DefaultTheme,
  NavigationContainer,
} from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import { useCallback, useEffect, useMemo, useState } from 'react';

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
import {
  clearSession,
  loadBaseUrl,
  loadSession,
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
  const [projects, setProjects] = useState<ProjectSummary[]>([]);
  const [projectsLoading, setProjectsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const api = useMemo(
    () => new LatitudePublicApi(session?.baseUrl ?? '', session?.token),
    [session],
  );

  const signOut = useCallback(async () => {
    if (session?.baseUrl) {
      setRememberedBaseUrl(session.baseUrl);
    }
    await clearSession();
    setSession(null);
    setProjects([]);
    setError(null);
  }, [session?.baseUrl]);

  const loadProjects = useCallback(async () => {
    if (!session) {
      return;
    }

    setProjectsLoading(true);
    setError(null);
    try {
      const response = await api.projects();
      setProjects(response.projects);
    } catch (loadError) {
      if (loadError instanceof LatitudeApiError && loadError.status === 401) {
        await signOut();
        setError('Sign in again to continue.');
      } else {
        setError(errorMessage(loadError));
      }
    } finally {
      setProjectsLoading(false);
      setBooting(false);
    }
  }, [api, session, signOut]);

  useEffect(() => {
    let mounted = true;

    Promise.all([loadSession(), loadBaseUrl()])
      .then(([storedSession, storedBaseUrl]) => {
        if (mounted) {
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
    await saveSession(nextSession);
    setSession(nextSession);
    setProjects([]);
    setError(null);
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
      <NavigationContainer theme={navigationTheme}>
        <Stack.Navigator screenOptions={{ headerShown: false }}>
          <Stack.Screen name="Home">
            {({ navigation }) => (
              <HomeScreen
                baseUrl={session.baseUrl}
                error={error}
                loading={projectsLoading}
                projects={projects}
                onOpenProject={(name) => navigation.navigate('Project', { name })}
                onRefresh={() => {
                  void loadProjects();
                }}
                onSignOut={signOut}
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
