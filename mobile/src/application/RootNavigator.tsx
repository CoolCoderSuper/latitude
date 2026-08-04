import {
  DarkTheme,
  DefaultTheme,
  NavigationContainer,
} from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import { useMemo } from 'react';

import { useLatitudeApi } from '../core/api/ApiContext';
import { useSession } from '../core/session/SessionContext';
import { DeploymentViewer } from '../features/deployments/DeploymentViewer';
import { useProjects } from '../features/projects/useProjects';
import type { RootStackParamList } from '../navigationTypes';
import { ConnectScreen } from '../screens/ConnectScreen';
import { GitCommitScreen } from '../screens/GitCommitScreen';
import { GitHistoryScreen } from '../screens/GitHistoryScreen';
import { HomeScreen } from '../screens/HomeScreen';
import { ProjectRoute } from '../screens/ProjectRoute';
import { RootDesktopScreen } from '../screens/RootDesktopScreen';
import { RootTerminalScreen } from '../screens/RootTerminalScreen';
import { ServersScreen } from '../screens/ServersScreen';
import { UnavailableScreen } from '../screens/UnavailableScreen';
import { useTheme } from '../theme';

const Stack = createNativeStackNavigator<RootStackParamList>();

export function RootNavigator() {
  const { colors, mode } = useTheme();
  const api = useLatitudeApi();
  const {
    clearError,
    error: sessionError,
    login,
    rememberedBaseUrl,
    removeServer,
    reorderServers,
    session,
    sessions,
    switchServer,
  } = useSession();
  const {
    error,
    loading,
    projects,
    refresh,
    rootDesktop,
    rootTerminal,
    setWorktreeArchived,
  } = useProjects();

  if (!session) {
    throw new Error('RootNavigator requires an active session.');
  }

  const navigationTheme = useMemo(() => {
    const baseTheme = mode === 'dark' ? DarkTheme : DefaultTheme;
    return {
      ...baseTheme,
      colors: {
        ...baseTheme.colors,
        background: colors.background,
        border: colors.border,
        card: colors.surface,
        primary: colors.accent,
        text: colors.text,
      },
    };
  }, [colors, mode]);

  return (
    <NavigationContainer key={session.baseUrl} theme={navigationTheme}>
      <Stack.Navigator screenOptions={{ headerShown: false }}>
        <Stack.Screen name="Home">
          {({ navigation }) => (
            <HomeScreen
              baseUrl={session.baseUrl}
              deviceHostname={session.deviceHostname}
              error={error}
              loading={loading}
              projects={projects}
              rootDesktop={rootDesktop}
              rootTerminal={rootTerminal}
              serverSessions={sessions}
              onManageServers={() => navigation.navigate('Servers')}
              onOpenRootDesktop={() => navigation.navigate('RootDesktop')}
              onOpenProject={(name) => navigation.navigate('Project', { name })}
              onOpenRootTerminal={() => navigation.navigate('RootTerminal')}
              onRefresh={refresh}
              onSetWorktreeArchived={setWorktreeArchived}
              onSwitchServer={switchServer}
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
              onOpenGitHistory={() =>
                navigation.navigate('GitHistory', {
                  projectName: route.params.name,
                })
              }
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
        <Stack.Screen name="GitHistory">
          {({ navigation, route }) => (
            <GitHistoryScreen
              api={api}
              deviceHostname={session.deviceHostname}
              projectName={route.params.projectName}
              onBack={() => navigation.goBack()}
              onOpenCommit={(hash) =>
                navigation.navigate('GitCommit', {
                  projectName: route.params.projectName,
                  hash,
                })
              }
            />
          )}
        </Stack.Screen>
        <Stack.Screen name="GitCommit">
          {({ navigation, route }) => (
            <GitCommitScreen
              api={api}
              deviceHostname={session.deviceHostname}
              hash={route.params.hash}
              projectName={route.params.projectName}
              onBack={() => navigation.goBack()}
            />
          )}
        </Stack.Screen>
        <Stack.Screen name="RootDesktop">
          {({ navigation }) =>
            rootDesktop ? (
              <RootDesktopScreen
                deviceHostname={session.deviceHostname}
                rootDesktop={rootDesktop}
                session={session}
                onBack={() => navigation.goBack()}
              />
            ) : (
              <UnavailableScreen
                message="Desktop is not enabled on this server."
                onBack={() => navigation.goBack()}
                title="Desktop unavailable"
              />
            )
          }
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
              onRemoveServer={removeServer}
              onReorderServers={reorderServers}
              onSwitchServer={switchServer}
            />
          )}
        </Stack.Screen>
        <Stack.Screen name="Connect">
          {({ navigation }) => (
            <ConnectScreen
              error={sessionError}
              initialBaseUrl={rememberedBaseUrl}
              onCancel={() => navigation.goBack()}
              onClearError={clearError}
              onLogin={async (baseUrl, password) => {
                await login(baseUrl, password);
                navigation.popToTop();
              }}
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
  );
}
