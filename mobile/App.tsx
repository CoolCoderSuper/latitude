import 'react-native-gesture-handler';

import {
  DarkTheme,
  DefaultTheme,
  NavigationContainer,
} from '@react-navigation/native';
import { createNativeStackNavigator } from '@react-navigation/native-stack';
import { StatusBar } from 'expo-status-bar';
import {
  ArrowLeft,
  CheckCircle2,
  ChevronRight,
  Download,
  ExternalLink,
  FileText,
  FolderOpen,
  GitBranch,
  GitCommitHorizontal,
  Globe2,
  LogOut,
  Moon,
  Plus,
  Rocket,
  Server,
  Sun,
  Terminal as TerminalIcon,
  Upload,
  X,
  XCircle,
} from 'lucide-react-native';
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import type { ReactNode } from 'react';
import {
  ActivityIndicator,
  AppState,
  KeyboardAvoidingView,
  PanResponder,
  Platform,
  Pressable,
  RefreshControl,
  ScrollView,
  StyleSheet,
  Text,
  TextInput,
  useColorScheme,
  View,
} from 'react-native';
import {
  SafeAreaProvider,
  SafeAreaView,
} from 'react-native-safe-area-context';
import WebView from 'react-native-webview';

import {
  LatitudeApiError,
  LatitudePublicApi,
  absoluteUrl,
  authHeaders,
  normalizeBaseUrl,
} from './src/api';
import {
  clearSession,
  loadBaseUrl,
  loadSession,
  loadThemeMode,
  saveBaseUrl,
  saveSession,
  saveThemeMode,
} from './src/storage';
import type {
  DeploymentKind,
  DeploymentSummary,
  GitActionPayload,
  GitDiffResponse,
  GitFileChange,
  GitFileDiff,
  ProjectDetail,
  ProjectSummary,
  SessionRecord,
  TerminalSessionSummary,
} from './src/types';

type ViewerState = {
  title: string;
  href: string;
  kind?: DeploymentKind;
};

type ProjectTab = 'deployments' | 'code' | 'terminal';
type RootStackParamList = {
  Home: undefined;
  Project: {
    initialTab?: ProjectTab;
    name: string;
  };
  Viewer: ViewerState;
};

const Stack = createNativeStackNavigator<RootStackParamList>();
const PROJECT_TABS: ProjectTab[] = ['deployments', 'code', 'terminal'];
export default function App() {
  return (
    <ThemeProvider>
      <AppContent />
    </ThemeProvider>
  );
}

const DEFAULT_BASE_URL = 'http://127.0.0.1:8080';
const ANDROID_EMULATOR_URL = 'http://10.0.2.2:8080';

type ThemeMode = 'light' | 'dark';
type ThemeColors = typeof lightColors;
type AppStyles = ReturnType<typeof createStyles>;

type ThemeContextValue = {
  colors: ThemeColors;
  mode: ThemeMode;
  styles: AppStyles;
  toggleMode: () => void;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);

function ThemeProvider({ children }: { children: ReactNode }) {
  const systemScheme = useColorScheme();
  const [mode, setMode] = useState<ThemeMode>(
    systemScheme === 'dark' ? 'dark' : 'light',
  );
  const [loadedPreference, setLoadedPreference] = useState(false);
  const colors = mode === 'dark' ? darkColors : lightColors;
  const styles = useMemo(() => createStyles(colors), [colors]);

  useEffect(() => {
    let mounted = true;

    loadThemeMode()
      .then((storedMode) => {
        if (mounted && storedMode) {
          setMode(storedMode);
        }
      })
      .finally(() => {
        if (mounted) {
          setLoadedPreference(true);
        }
      });

    return () => {
      mounted = false;
    };
  }, []);

  useEffect(() => {
    if (loadedPreference) {
      void saveThemeMode(mode);
    }
  }, [loadedPreference, mode]);

  const value = useMemo(
    () => ({
      colors,
      mode,
      styles,
      toggleMode: () =>
        setMode((current) => (current === 'dark' ? 'light' : 'dark')),
    }),
    [colors, mode, styles],
  );

  return (
    <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>
  );
}

function useTheme() {
  const value = useContext(ThemeContext);
  if (!value) {
    throw new Error('ThemeContext is missing.');
  }

  return value;
}

function useRefreshControl(refreshing: boolean, onRefresh: () => void | Promise<void>) {
  const { colors } = useTheme();

  return (
    <RefreshControl
      colors={[colors.accent]}
      onRefresh={onRefresh}
      progressBackgroundColor={colors.surface}
      refreshing={refreshing}
      tintColor={colors.accent}
    />
  );
}

function AppContent() {
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

function Shell({ children }: { children: ReactNode }) {
  const { mode, styles } = useTheme();

  return (
    <SafeAreaProvider>
      <SafeAreaView style={styles.safeArea}>
        <StatusBar style={mode === 'dark' ? 'light' : 'dark'} />
        {children}
      </SafeAreaView>
    </SafeAreaProvider>
  );
}

function LoadingScreen() {
  const { colors, styles } = useTheme();

  return (
    <View style={styles.centered}>
      <Server color={colors.accent} size={34} />
      <Text style={styles.loadingTitle}>Latitude</Text>
      <ActivityIndicator color={colors.accent} />
    </View>
  );
}

function ConnectScreen({
  error,
  initialBaseUrl,
  onClearError,
  onLogin,
}: {
  error: string | null;
  initialBaseUrl: string;
  onClearError: () => void;
  onLogin: (baseUrl: string, password: string) => Promise<void>;
}) {
  const { colors, styles } = useTheme();
  const [baseUrl, setBaseUrl] = useState(initialBaseUrl);
  const [password, setPassword] = useState('');
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    setBaseUrl(initialBaseUrl);
  }, [initialBaseUrl]);

  const submit = useCallback(async () => {
    if (!baseUrl.trim() || !password) {
      return;
    }

    setSubmitting(true);
    onClearError();
    try {
      await onLogin(baseUrl, password);
    } catch (loginError) {
      onClearError();
      throw loginError;
    } finally {
      setSubmitting(false);
    }
  }, [baseUrl, onClearError, onLogin, password]);

  const [localError, setLocalError] = useState<string | null>(null);

  const submitWithError = useCallback(async () => {
    setLocalError(null);
    try {
      await submit();
    } catch (submitError) {
      setLocalError(errorMessage(submitError));
    }
  }, [submit]);

  return (
    <KeyboardAvoidingView
      behavior={Platform.OS === 'ios' ? 'padding' : undefined}
      style={styles.flex}
    >
      <ScrollView
        contentContainerStyle={styles.connectContent}
        keyboardShouldPersistTaps="handled"
      >
        <View style={styles.brandRow}>
          <View style={styles.brandMark}>
            <Server color={colors.onAccent} size={28} />
          </View>
          <View>
            <Text style={styles.appName}>Latitude</Text>
            <Text style={styles.appSubhead}>Native client</Text>
          </View>
        </View>

        <View style={styles.formGroup}>
          <Text style={styles.label}>Public URL</Text>
          <TextInput
            autoCapitalize="none"
            autoCorrect={false}
            keyboardType="url"
            onChangeText={setBaseUrl}
            placeholder={DEFAULT_BASE_URL}
            placeholderTextColor={colors.muted}
            style={styles.input}
            value={baseUrl}
          />
          <View style={styles.quickRow}>
            <Chip label="Localhost" onPress={() => setBaseUrl(DEFAULT_BASE_URL)} />
            <Chip
              label="Android"
              onPress={() => setBaseUrl(ANDROID_EMULATOR_URL)}
            />
          </View>
        </View>

        <View style={styles.formGroup}>
          <Text style={styles.label}>Password</Text>
          <TextInput
            autoCapitalize="none"
            autoCorrect={false}
            onChangeText={setPassword}
            placeholder="Public password"
            placeholderTextColor={colors.muted}
            secureTextEntry
            style={styles.input}
            value={password}
          />
        </View>

        {(error || localError) && (
          <InlineNotice tone="error" text={localError ?? error ?? ''} />
        )}

        <AppButton
          disabled={submitting || !baseUrl.trim() || !password}
          icon={<CheckCircle2 color={colors.onAccent} size={18} />}
          label={submitting ? 'Signing in' : 'Sign in'}
          onPress={submitWithError}
        />
      </ScrollView>
    </KeyboardAvoidingView>
  );
}

function HomeScreen({
  baseUrl,
  error,
  loading,
  onOpenProject,
  onRefresh,
  onSignOut,
  projects,
}: {
  baseUrl: string;
  error: string | null;
  loading: boolean;
  onOpenProject: (name: string) => void;
  onRefresh: () => void | Promise<void>;
  onSignOut: () => void;
  projects: ProjectSummary[];
}) {
  const { colors, styles } = useTheme();
  const refreshControl = useRefreshControl(loading, onRefresh);

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={baseUrl}
        right={
          <IconButton
            accessibilityLabel="Sign out"
            icon={<LogOut color={colors.text} size={20} />}
            onPress={onSignOut}
          />
        }
        title="Projects"
      />
      <ScrollView
        contentContainerStyle={styles.screenContent}
        refreshControl={refreshControl}
      >
        {error && <InlineNotice tone="error" text={error} />}
        {loading && projects.length === 0 ? (
          <LoadingBlock label="Loading projects" />
        ) : projects.length === 0 ? (
          <EmptyState title="No enabled projects" />
        ) : (
          <View style={styles.list}>
            {projects.map((project) => (
              <Pressable
                key={project.name}
                onPress={() => onOpenProject(project.name)}
                style={({ pressed }) => [
                  styles.projectCard,
                  pressed && styles.pressed,
                ]}
              >
                <View style={styles.cardIcon}>
                  <FolderOpen color={colors.accent} size={21} />
                </View>
                <View style={styles.cardBody}>
                  <Text style={styles.cardTitle}>{project.name}</Text>
                  <Text style={styles.cardMeta}>{project.summary}</Text>
                </View>
                <ChevronRight color={colors.muted} size={20} />
              </Pressable>
            ))}
          </View>
        )}
      </ScrollView>
    </View>
  );
}

function ProjectRoute({
  api,
  initialTab,
  onBack,
  onOpenViewer,
  projectName,
  session,
}: {
  api: LatitudePublicApi;
  initialTab: ProjectTab;
  onBack: () => void;
  onOpenViewer: (deployment: DeploymentSummary) => void;
  projectName: string;
  session: SessionRecord;
}) {
  const { colors, styles } = useTheme();
  const [project, setProject] = useState<ProjectDetail | null>(null);
  const [projectLoading, setProjectLoading] = useState(true);
  const [tab, setTab] = useState<ProjectTab>(initialTab);
  const [error, setError] = useState<string | null>(null);

  const loadProject = useCallback(async () => {
    setProjectLoading(true);
    setError(null);
    try {
      setProject(await api.project(projectName));
    } catch (projectError) {
      setError(errorMessage(projectError));
    } finally {
      setProjectLoading(false);
    }
  }, [api, projectName]);

  useEffect(() => {
    void loadProject();
  }, [loadProject]);

  const refreshControl = useRefreshControl(projectLoading, loadProject);

  if (!project) {
    return (
      <View style={styles.flex}>
        <ScreenHeader
          eyebrow={projectLoading ? 'Loading project' : 'Project unavailable'}
          left={
            <IconButton
              accessibilityLabel="Back"
              icon={<ArrowLeft color={colors.text} size={22} />}
              onPress={onBack}
            />
          }
          title={projectName}
        />
        <ScrollView
          contentContainerStyle={styles.screenContent}
          refreshControl={refreshControl}
        >
          {projectLoading ? (
            <LoadingBlock label="Loading project" />
          ) : error ? (
            <InlineNotice text={error} tone="error" />
          ) : (
            <EmptyState title="Project unavailable" />
          )}
        </ScrollView>
      </View>
    );
  }

  return (
    <ProjectScreen
      api={api}
      project={project}
      projectLoading={projectLoading}
      session={session}
      tab={tab}
      onBack={onBack}
      onOpenViewer={onOpenViewer}
      onRefresh={loadProject}
      onSelectTab={setTab}
    />
  );
}

function ProjectScreen({
  api,
  onBack,
  onOpenViewer,
  onRefresh,
  onSelectTab,
  project,
  projectLoading,
  session,
  tab,
}: {
  api: LatitudePublicApi;
  onBack: () => void;
  onOpenViewer: (deployment: DeploymentSummary) => void;
  onRefresh: () => void | Promise<void>;
  onSelectTab: (tab: ProjectTab) => void;
  project: ProjectDetail;
  projectLoading: boolean;
  session: SessionRecord;
  tab: ProjectTab;
}) {
  const { colors, styles } = useTheme();
  const [codeInteractionActive, setCodeInteractionActive] = useState(false);

  const selectTab = useCallback(
    (nextTab: ProjectTab) => {
      onSelectTab(nextTab);
    },
    [onSelectTab],
  );

  const selectAdjacentTab = useCallback(
    (direction: number) => {
      const currentIndex = PROJECT_TABS.indexOf(tab);
      const nextTab = PROJECT_TABS[currentIndex + direction];
      if (nextTab) {
        selectTab(nextTab);
      }
    },
    [selectTab, tab],
  );

  const handleCodeInteractionChange = useCallback((active: boolean) => {
    setCodeInteractionActive(active);
  }, []);

  const tabSwipeResponder = useMemo(
    () =>
      PanResponder.create({
        onMoveShouldSetPanResponder: (_event, gesture) => {
          if (codeInteractionActive) {
            return false;
          }

          const absDx = Math.abs(gesture.dx);
          const absDy = Math.abs(gesture.dy);
          return absDx > 18 && absDx > absDy * 1.35;
        },
        onPanResponderRelease: (_event, gesture) => {
          const absDx = Math.abs(gesture.dx);
          const absDy = Math.abs(gesture.dy);
          const deliberateSwipe =
            (absDx > 72 || Math.abs(gesture.vx) > 0.45) &&
            absDx > absDy * 1.2;

          if (!deliberateSwipe) {
            return;
          }

          selectAdjacentTab(gesture.dx < 0 ? 1 : -1);
        },
        onPanResponderTerminationRequest: () => true,
      }),
    [codeInteractionActive, selectAdjacentTab],
  );

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={project.summary}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title={project.name}
      />
      <View style={styles.segmented}>
        <SegmentButton
          active={tab === 'deployments'}
          icon={<Globe2 color={tab === 'deployments' ? colors.onAccent : colors.text} size={17} />}
          label="Deployments"
          onPress={() => selectTab('deployments')}
        />
        <SegmentButton
          active={tab === 'code'}
          icon={<GitBranch color={tab === 'code' ? colors.onAccent : colors.text} size={17} />}
          label="Code"
          onPress={() => selectTab('code')}
        />
        <SegmentButton
          active={tab === 'terminal'}
          icon={<TerminalIcon color={tab === 'terminal' ? colors.onAccent : colors.text} size={17} />}
          label="Terminal"
          onPress={() => selectTab('terminal')}
        />
      </View>
      <View style={styles.tabPager} {...tabSwipeResponder.panHandlers}>
        <View
          pointerEvents={tab === 'deployments' ? 'auto' : 'none'}
          style={[
            styles.tabPage,
            tab === 'deployments' ? styles.tabPageActive : styles.tabPageHidden,
          ]}
        >
          <DeploymentsPanel
            deployments={project.deployments}
            onOpenViewer={onOpenViewer}
            onRefresh={onRefresh}
            refreshing={projectLoading}
          />
        </View>
        <View
          pointerEvents={tab === 'code' ? 'auto' : 'none'}
          style={[
            styles.tabPage,
            tab === 'code' ? styles.tabPageActive : styles.tabPageHidden,
          ]}
        >
          <DiffPanel
            api={api}
            onCodeInteractionChange={handleCodeInteractionChange}
            projectName={project.name}
            session={session}
          />
        </View>
        <View
          pointerEvents={tab === 'terminal' ? 'auto' : 'none'}
          style={[
            styles.tabPage,
            tab === 'terminal' ? styles.tabPageActive : styles.tabPageHidden,
          ]}
        >
          <TerminalPanel api={api} project={project} session={session} />
        </View>
      </View>
    </View>
  );
}

function DeploymentsPanel({
  deployments,
  onOpenViewer,
  onRefresh,
  refreshing,
}: {
  deployments: DeploymentSummary[];
  onOpenViewer: (deployment: DeploymentSummary) => void;
  onRefresh: () => void | Promise<void>;
  refreshing: boolean;
}) {
  const { colors, styles } = useTheme();
  const refreshControl = useRefreshControl(refreshing, onRefresh);

  return (
    <ScrollView
      contentContainerStyle={styles.screenContent}
      nestedScrollEnabled
      refreshControl={refreshControl}
    >
      {deployments.length === 0 ? (
        <EmptyState title="No enabled deployments" />
      ) : (
        <View style={styles.list}>
          {deployments.map((deployment) => (
            <Pressable
              key={deployment.name}
              onPress={() => onOpenViewer(deployment)}
              style={({ pressed }) => [
                styles.deploymentCard,
                pressed && styles.pressed,
              ]}
            >
              <View style={styles.cardIcon}>
                {kindIcon(deployment.kind, colors)}
              </View>
              <View style={styles.cardBody}>
                <Text style={styles.cardTitle}>{deployment.name}</Text>
                <Text style={styles.cardMeta}>
                  {deployment.title
                    ? `${deployment.label}: ${deployment.title}`
                    : deployment.label}
                </Text>
              </View>
              <ExternalLink color={colors.muted} size={20} />
            </Pressable>
          ))}
        </View>
      )}
    </ScrollView>
  );
}

function DeploymentViewer({
  baseUrl,
  onBack,
  token,
  viewer,
}: {
  baseUrl: string;
  onBack: () => void;
  token: string;
  viewer: ViewerState;
}) {
  const { colors, mode, styles } = useTheme();
  const webViewRef = useRef<WebView>(null);
  const shouldThemePage = viewer.kind === 'page';
  const uri = absoluteUrl(baseUrl, viewer.href);
  const themeScript = useMemo(
    () => (shouldThemePage ? deploymentThemeScript(mode, colors) : 'true;'),
    [colors, mode, shouldThemePage],
  );

  useEffect(() => {
    webViewRef.current?.injectJavaScript(themeScript);
  }, [themeScript]);

  return (
    <View style={styles.flex}>
      <ScreenHeader
        eyebrow={uri}
        left={
          <IconButton
            accessibilityLabel="Back"
            icon={<ArrowLeft color={colors.text} size={22} />}
            onPress={onBack}
          />
        }
        title={viewer.title}
      />
      <WebView
        ref={webViewRef}
        injectedJavaScript={themeScript}
        injectedJavaScriptBeforeContentLoaded={themeScript}
        javaScriptEnabled
        originWhitelist={['http://*', 'https://*']}
        sharedCookiesEnabled
        source={{
          uri,
          headers: {
            ...authHeaders(token),
            ...(shouldThemePage ? { 'X-Latitude-Theme': mode } : {}),
          },
        }}
        startInLoadingState
        style={styles.webView}
      />
    </View>
  );
}

function deploymentThemeScript(mode: ThemeMode, colors: ThemeColors): string {
  const theme = {
    mode,
    variables: {
      '--latitude-page-bg': colors.background,
      '--latitude-page-text': colors.text,
      '--latitude-page-heading': colors.text,
      '--latitude-page-muted': colors.softText,
      '--latitude-page-accent': colors.accent,
      '--latitude-page-inline-code-bg': colors.panel,
      '--latitude-page-code-bg': colors.codeBg,
      '--latitude-page-code-text': colors.codeText,
      '--latitude-page-border': colors.border,
    },
  };

  return `
(function() {
  var theme = ${JSON.stringify(theme)};
  var applyTheme = function() {
    var root = document.documentElement;
    if (!root) {
      return;
    }

    root.dataset.latitudeTheme = theme.mode;
    root.style.colorScheme = theme.mode;

    Object.keys(theme.variables).forEach(function(name) {
      root.style.setProperty(name, theme.variables[name]);
    });
  };

  applyTheme();
  document.addEventListener('DOMContentLoaded', applyTheme);
})();
true;
`;
}

function DiffPanel({
  api,
  onCodeInteractionChange,
  projectName,
}: {
  api: LatitudePublicApi;
  onCodeInteractionChange: (active: boolean) => void;
  projectName: string;
  session: SessionRecord;
}) {
  const { colors, styles } = useTheme();
  const [diff, setDiff] = useState<GitDiffResponse | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [loading, setLoading] = useState(true);
  const [actioning, setActioning] = useState(false);
  const [message, setMessage] = useState('');
  const [notice, setNotice] = useState<string | null>(null);
  const [noticeTone, setNoticeTone] = useState<'success' | 'error'>('success');

  const loadDiff = useCallback(async () => {
    setLoading(true);
    setNotice(null);
    try {
      setDiff(await api.diff(projectName));
    } catch (diffError) {
      setNotice(errorMessage(diffError));
      setNoticeTone('error');
    } finally {
      setLoading(false);
    }
  }, [api, projectName]);

  useEffect(() => {
    void loadDiff();
  }, [loadDiff]);

  const runAction = useCallback(
    async (payload: GitActionPayload, successMessage: string) => {
      setActioning(true);
      setNotice(null);
      try {
        const response = await api.runGitAction(projectName, payload);
        setDiff(response.diff);
        setNotice(response.ok ? successMessage : response.error ?? 'Action failed.');
        setNoticeTone(response.ok ? 'success' : 'error');
        if (response.ok && payload.action === 'commit') {
          setMessage('');
        }
      } catch (actionError) {
        setNotice(errorMessage(actionError));
        setNoticeTone('error');
      } finally {
        setActioning(false);
      }
    },
    [api, projectName],
  );

  const unstaged = diff?.file_changes.filter(canStage) ?? [];
  const staged = diff?.file_changes.filter(canUnstage) ?? [];
  const refreshControl = useRefreshControl(loading, loadDiff);

  return (
    <ScrollView
      contentContainerStyle={styles.screenContent}
      nestedScrollEnabled
      refreshControl={refreshControl}
    >
      <View style={styles.diffToolbar}>
        <AppButton
          compact
          disabled={actioning || unstaged.length === 0}
          icon={<Upload color={colors.onAccent} size={16} />}
          label="Stage all"
          onPress={() =>
            runAction({ action: 'stage_all' }, 'All changes staged.')
          }
        />
        <AppButton
          compact
          disabled={actioning || staged.length === 0}
          icon={<Download color={colors.text} size={16} />}
          label="Unstage all"
          onPress={() =>
            runAction({ action: 'unstage_all' }, 'All changes unstaged.')
          }
          variant="secondary"
        />
      </View>

      <View style={styles.commitRow}>
        <TextInput
          editable={!actioning}
          onChangeText={setMessage}
          placeholder="Commit message"
          placeholderTextColor={colors.muted}
          style={[styles.input, styles.commitInput]}
          value={message}
        />
        <AppButton
          compact
          disabled={actioning || staged.length === 0 || !message.trim()}
          icon={<GitCommitHorizontal color={colors.onAccent} size={16} />}
          label="Commit"
          onPress={() =>
            runAction(
              { action: 'commit', message: message.trim() },
              'Staged changes committed.',
            )
          }
        />
      </View>

      <AppButton
        compact
        disabled={actioning}
        icon={<Rocket color={colors.text} size={16} />}
        label="Push"
        onPress={() => runAction({ action: 'push' }, 'Push completed.')}
        variant="secondary"
      />

      {notice && <InlineNotice tone={noticeTone} text={notice} />}

      {loading ? (
        <LoadingBlock label="Loading code changes" />
      ) : diff ? (
        <>
          <DiffSection
            actioning={actioning}
            changes={unstaged}
            empty="No unstaged files."
            expanded={expanded}
            onAction={(change) =>
              runAction(
                { action: 'stage_file', path: change.path },
                `${change.path} staged.`,
              )
            }
            onCodeInteractionChange={onCodeInteractionChange}
            onToggle={(path) => toggleExpanded(setExpanded, path)}
            section="unstaged"
            title="Unstaged"
          />
          <DiffSection
            actioning={actioning}
            changes={staged}
            empty="No staged files."
            expanded={expanded}
            onAction={(change) =>
              runAction(
                { action: 'unstage_file', path: change.path },
                `${change.path} unstaged.`,
              )
            }
            onCodeInteractionChange={onCodeInteractionChange}
            onToggle={(path) => toggleExpanded(setExpanded, path)}
            section="staged"
            title="Staged"
          />
        </>
      ) : (
        <EmptyState title="No diff available" />
      )}
    </ScrollView>
  );
}

function TerminalPanel({
  api,
  project,
  session,
}: {
  api: LatitudePublicApi;
  project: ProjectDetail;
  session: SessionRecord;
}) {
  const { colors, styles } = useTheme();
  const [sessions, setSessions] = useState<TerminalSessionSummary[]>([]);
  const [activeSessionId, setActiveSessionId] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [closingSessionId, setClosingSessionId] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);

  const loadSessions = useCallback(async () => {
    setLoading(true);
    setNotice(null);
    try {
      let nextSessions = (await api.terminalSessions(project.name)).sessions;
      if (nextSessions.length === 0) {
        nextSessions = [await api.createTerminalSession(project.name)];
      }
      setSessions(nextSessions);
      setActiveSessionId((current) =>
        nextSessions.some((item) => item.id === current)
          ? current
          : nextSessions[0]?.id ?? null,
      );
    } catch (sessionError) {
      setNotice(errorMessage(sessionError));
    } finally {
      setLoading(false);
    }
  }, [api, project.name]);

  useEffect(() => {
    void loadSessions();
  }, [loadSessions]);

  const createSession = useCallback(async () => {
    if (creating) {
      return;
    }

    setCreating(true);
    setNotice(null);
    try {
      const created = await api.createTerminalSession(project.name);
      setSessions((current) => [...current, created]);
      setActiveSessionId(created.id);
    } catch (sessionError) {
      setNotice(errorMessage(sessionError));
    } finally {
      setCreating(false);
    }
  }, [api, creating, project.name]);

  const closeSession = useCallback(
    async (sessionId: string) => {
      if (closingSessionId) {
        return;
      }

      setClosingSessionId(sessionId);
      setNotice(null);
      try {
        await api.closeTerminalSession(project.name, sessionId);
        setSessions((current) => {
          const next = current.filter((item) => item.id !== sessionId);
          setActiveSessionId((active) =>
            active === sessionId ? next[0]?.id ?? null : active,
          );
          return next;
        });
      } catch (sessionError) {
        setNotice(errorMessage(sessionError));
      } finally {
        setClosingSessionId(null);
      }
    },
    [api, closingSessionId, project.name],
  );

  return (
    <View style={styles.terminalPanel}>
      <View style={styles.terminalSessionBar}>
        <ScrollView
          horizontal
          contentContainerStyle={styles.terminalSessionList}
          showsHorizontalScrollIndicator={false}
        >
          {sessions.map((terminalSession) => {
            const active = terminalSession.id === activeSessionId;
            return (
              <View key={terminalSession.id} style={styles.terminalSessionItem}>
                <Pressable
                  onPress={() => setActiveSessionId(terminalSession.id)}
                  style={({ pressed }) => [
                    styles.terminalSessionChip,
                    active && styles.terminalSessionChipActive,
                    pressed && styles.pressed,
                  ]}
                >
                  <TerminalIcon
                    color={active ? colors.onAccent : colors.text}
                    size={15}
                  />
                  <Text
                    numberOfLines={1}
                    style={[
                      styles.terminalSessionText,
                      active && styles.terminalSessionTextActive,
                    ]}
                  >
                    {terminalSession.title}
                  </Text>
                </Pressable>
                <Pressable
                  accessibilityLabel={`Close ${terminalSession.title}`}
                  disabled={closingSessionId === terminalSession.id}
                  onPress={() => {
                    void closeSession(terminalSession.id);
                  }}
                  style={({ pressed }) => [
                    styles.terminalSessionClose,
                    pressed && styles.pressed,
                  ]}
                >
                  <X color={colors.muted} size={14} />
                </Pressable>
              </View>
            );
          })}
        </ScrollView>
        <Pressable
          accessibilityLabel="New terminal"
          disabled={creating}
          onPress={() => {
            void createSession();
          }}
          style={({ pressed }) => [
            styles.terminalNewButton,
            pressed && styles.pressed,
          ]}
        >
          <Plus color={colors.onAccent} size={18} />
        </Pressable>
      </View>

      {notice && <InlineNotice tone="error" text={notice} />}

      <View style={styles.terminalStack}>
        {loading ? (
          <LoadingBlock label="Loading terminals" />
        ) : sessions.length === 0 ? (
          <EmptyState title="No terminals" />
        ) : (
          sessions.map((terminalSession) => (
            <TerminalSessionView
              key={terminalSession.id}
              active={terminalSession.id === activeSessionId}
              baseUrl={session.baseUrl}
              project={project}
              session={terminalSession}
              token={session.token}
            />
          ))
        )}
      </View>
    </View>
  );
}

function terminalWebSocketUrl(
  baseUrl: string,
  terminalHref: string,
  token: string,
  sessionId: string,
): string {
  const cleanHref = terminalHref.replace(/\/+$/, '');
  const url = new URL(`${cleanHref}/ws`, `${normalizeBaseUrl(baseUrl)}/`);
  url.protocol = url.protocol === 'https:' ? 'wss:' : 'ws:';
  url.searchParams.set('token', token);
  url.searchParams.set('session', sessionId);
  return url.toString();
}

function TerminalSessionView({
  active,
  baseUrl,
  project,
  session,
  token,
}: {
  active: boolean;
  baseUrl: string;
  project: ProjectDetail;
  session: TerminalSessionSummary;
  token: string;
}) {
  const { styles } = useTheme();
  const webViewRef = useRef<WebView>(null);
  const terminalUrl = useMemo(
    () => terminalWebSocketUrl(baseUrl, project.terminal.href, token, session.id),
    [baseUrl, project.terminal.href, session.id, token],
  );
  const terminalHtml = useMemo(
    () => terminalDocument(`${project.name} - ${session.title}`, terminalUrl),
    [project.name, session.title, terminalUrl],
  );

  useEffect(() => {
    const subscription = AppState.addEventListener('change', (state) => {
      if (state === 'active') {
        webViewRef.current?.injectJavaScript(
          'window.latitudeReconnect && window.latitudeReconnect(true); true;',
        );
      }
    });
    return () => subscription.remove();
  }, []);

  return (
    <View
      pointerEvents={active ? 'auto' : 'none'}
      style={[styles.terminalFrame, active && styles.terminalFrameActive]}
    >
      <WebView
        ref={webViewRef}
        domStorageEnabled
        javaScriptEnabled
        keyboardDisplayRequiresUserAction={false}
        mixedContentMode="always"
        originWhitelist={['*']}
        setSupportMultipleWindows={false}
        source={{ html: terminalHtml, baseUrl: normalizeBaseUrl(baseUrl) }}
        startInLoadingState
        style={styles.webView}
      />
    </View>
  );
}

function terminalDocument(projectName: string, websocketUrl: string): string {
  const projectNameJson = JSON.stringify(projectName);
  const websocketUrlJson = JSON.stringify(websocketUrl);

  return `<!doctype html>
<html lang="en">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1, viewport-fit=cover" />
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/css/xterm.css" />
  <style>
    html,
    body {
      height: 100%;
      margin: 0;
      overflow: hidden;
      background: #101514;
      color: #edf4f1;
      font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    }

    body {
      box-sizing: border-box;
      display: grid;
      grid-template-rows: auto minmax(0, 1fr);
      gap: 8px;
      padding: env(safe-area-inset-top, 0) 8px env(safe-area-inset-bottom, 0);
    }

    .bar {
      box-sizing: border-box;
      display: flex;
      align-items: center;
      gap: 8px;
      min-height: 34px;
      border-bottom: 1px solid #2e3936;
      padding: 6px 2px;
      color: #aeb9c7;
      font-size: 12px;
      font-weight: 800;
    }

    .name {
      min-width: 0;
      overflow: hidden;
      color: #edf4f1;
      text-overflow: ellipsis;
      white-space: nowrap;
    }

    .status {
      margin-left: auto;
      color: #8fe0ad;
      white-space: nowrap;
    }

    .status.error {
      color: #ffb3a7;
    }

    #terminal {
      box-sizing: border-box;
      width: 100%;
      height: 100%;
      min-height: 0;
      overflow: hidden;
      border: 1px solid #2e3936;
      border-radius: 8px;
      padding: 6px;
      background: #101514;
    }

    .xterm {
      height: 100%;
    }

    .xterm .xterm-viewport {
      overflow-y: auto;
    }
  </style>
</head>
<body>
  <div class="bar">
    <span class="name"></span>
    <span class="status">Connecting</span>
  </div>
  <div id="terminal"></div>
  <script src="https://cdn.jsdelivr.net/npm/@xterm/xterm@5.5.0/lib/xterm.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/@xterm/addon-fit@0.10.0/lib/addon-fit.min.js"></script>
  <script>
    const projectName = ${projectNameJson};
    const websocketUrl = ${websocketUrlJson};
    const terminalElement = document.getElementById('terminal');
    const statusElement = document.querySelector('.status');
    document.querySelector('.name').textContent = projectName;

    const setStatus = (text, isError = false) => {
      statusElement.textContent = text;
      statusElement.classList.toggle('error', Boolean(isError));
    };

    const start = () => {
      if (!window.Terminal || !window.FitAddon) {
        setStatus('Assets failed', true);
        return;
      }

      const terminal = new window.Terminal({
        convertEol: true,
        cursorBlink: true,
        cursorStyle: 'block',
        disableStdin: false,
        fontFamily: 'Menlo, Monaco, Consolas, "Liberation Mono", monospace',
        fontSize: 12,
        lineHeight: 1.18,
        scrollback: 5000,
        theme: {
          background: '#101514',
          foreground: '#edf4f1',
          cursor: '#2aa79c',
          selectionBackground: '#2e3936',
          black: '#101514',
          red: '#ff9d87',
          green: '#8fe0ad',
          yellow: '#e1b95a',
          blue: '#9ed2ff',
          magenta: '#c9b6ff',
          cyan: '#73d7e7',
          white: '#edf4f1',
          brightBlack: '#8f9b97',
          brightRed: '#ffd0ca',
          brightGreen: '#c8f2d5',
          brightYellow: '#ffd98b',
          brightBlue: '#c8e4ff',
          brightMagenta: '#e0d6ff',
          brightCyan: '#bdf4fb',
          brightWhite: '#ffffff',
        },
      });
      const fitAddon = new window.FitAddon.FitAddon();
      terminal.loadAddon(fitAddon);
      terminal.open(terminalElement);
      terminal.focus();

      let socket = null;
      let resizeTimer = null;
      let reconnectTimer = null;
      let reconnectDelay = 1000;
      let hasConnected = false;
      const maxReconnectDelay = 8000;

      const sendJson = (payload) => {
        if (socket && socket.readyState === WebSocket.OPEN) {
          socket.send(JSON.stringify(payload));
        }
      };

      const fitAndResize = () => {
        try {
          fitAddon.fit();
        } catch (_) {
          return;
        }
        sendJson({ type: 'resize', cols: terminal.cols, rows: terminal.rows });
      };

      const socketIsActive = (candidate) =>
        candidate &&
        (candidate.readyState === WebSocket.OPEN ||
          candidate.readyState === WebSocket.CONNECTING);

      const clearReconnectTimer = () => {
        if (reconnectTimer) {
          window.clearTimeout(reconnectTimer);
          reconnectTimer = null;
        }
      };

      const scheduleReconnect = () => {
        clearReconnectTimer();
        const delay = reconnectDelay;
        setStatus('Reconnecting', true);
        reconnectTimer = window.setTimeout(() => {
          reconnectTimer = null;
          connect();
        }, delay);
        reconnectDelay = Math.min(
          maxReconnectDelay,
          Math.floor(reconnectDelay * 1.6),
        );
      };

      const connect = () => {
        if (socketIsActive(socket)) {
          return;
        }

        clearReconnectTimer();
        setStatus('Connecting');
        const nextSocket = new WebSocket(websocketUrl);
        socket = nextSocket;

        nextSocket.addEventListener('open', () => {
          if (socket !== nextSocket) {
            nextSocket.close();
            return;
          }

          clearReconnectTimer();
          reconnectDelay = 1000;
          if (hasConnected) {
            terminal.reset();
          }
          hasConnected = true;
          setStatus('Connected');
          fitAndResize();
          window.setTimeout(() => setStatus(''), 800);
        });

        nextSocket.addEventListener('message', (event) => {
          if (socket !== nextSocket) {
            return;
          }

          if (typeof event.data === 'string') {
            terminal.write(event.data);
          } else if (event.data instanceof Blob) {
            event.data.text().then((text) => terminal.write(text));
          }
        });

        nextSocket.addEventListener('close', () => {
          if (socket !== nextSocket) {
            return;
          }

          socket = null;
          scheduleReconnect();
        });

        nextSocket.addEventListener('error', () => {
          if (socket !== nextSocket) {
            return;
          }

          setStatus('Connection failed', true);
          try {
            nextSocket.close();
          } catch (_) {
            scheduleReconnect();
          }
        });
      };

      window.latitudeReconnect = (force) => {
        clearReconnectTimer();
        reconnectDelay = 1000;
        if (force && socket && socket.readyState !== WebSocket.CLOSED) {
          const staleSocket = socket;
          socket = null;
          try {
            staleSocket.close();
          } catch (_) {}
        }

        if (socketIsActive(socket)) {
          fitAndResize();
          return;
        }

        connect();
      };

      terminal.onData((data) => {
        if (!socketIsActive(socket)) {
          window.latitudeReconnect(false);
        }
        sendJson({ type: 'input', data });
      });

      const queueResize = () => {
        window.clearTimeout(resizeTimer);
        resizeTimer = window.setTimeout(fitAndResize, 80);
      };

      window.addEventListener('resize', queueResize);
      window.addEventListener('focus', () => window.latitudeReconnect(false));
      window.addEventListener('online', () => window.latitudeReconnect(true));
      window.visualViewport?.addEventListener('resize', queueResize);
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible') {
          window.latitudeReconnect(false);
        }
      });
      terminalElement.addEventListener('touchstart', () => terminal.focus(), {
        passive: true,
      });

      connect();
    };

    if (document.readyState === 'loading') {
      document.addEventListener('DOMContentLoaded', start);
    } else {
      start();
    }
  </script>
</body>
</html>`;
}

function DiffSection({
  actioning,
  changes,
  empty,
  expanded,
  onAction,
  onCodeInteractionChange,
  onToggle,
  section,
  title,
}: {
  actioning: boolean;
  changes: GitFileChange[];
  empty: string;
  expanded: Set<string>;
  onAction: (change: GitFileChange) => void;
  onCodeInteractionChange: (active: boolean) => void;
  onToggle: (path: string) => void;
  section: 'unstaged' | 'staged';
  title: string;
}) {
  const { colors, styles } = useTheme();

  return (
    <View style={styles.diffSection}>
      <View style={styles.sectionHeading}>
        <Text style={styles.sectionTitle}>{title}</Text>
        <Text style={styles.sectionCount}>
          {changes.length} {changes.length === 1 ? 'file' : 'files'}
        </Text>
      </View>
      {changes.length === 0 ? (
        <View style={styles.emptyPanel}>
          <Text style={styles.emptyText}>{empty}</Text>
        </View>
      ) : (
        changes.map((change) => {
          const isOpen = expanded.has(`${section}:${change.path}`);
          const visibleDiffs = visibleDiffsForSection(change, section);
          const actionLabel = section === 'unstaged' ? 'Stage' : 'Unstage';
          return (
            <View key={`${section}:${change.path}`} style={styles.fileCard}>
              <View style={styles.fileSummary}>
                <Pressable
                  onPress={() => onToggle(`${section}:${change.path}`)}
                  style={({ pressed }) => [
                    styles.fileSummaryMain,
                    pressed && styles.pressed,
                  ]}
                >
                  <Text style={styles.statusBadge}>{statusLabel(change)}</Text>
                  <View style={styles.cardBody}>
                    <Text numberOfLines={2} style={styles.filePath}>
                      {change.path}
                    </Text>
                    {change.original_path && (
                      <Text numberOfLines={1} style={styles.cardMeta}>
                        from {change.original_path}
                      </Text>
                    )}
                  </View>
                  <Text style={styles.cardMeta}>
                    {visibleDiffs.length === 0
                      ? 'status'
                      : `${visibleDiffs.length} diff${visibleDiffs.length === 1 ? '' : 's'}`}
                  </Text>
                </Pressable>
                <Pressable
                  accessibilityLabel={`${actionLabel} ${change.path}`}
                  disabled={actioning}
                  onPress={() => onAction(change)}
                  style={({ pressed }) => [
                    styles.fileRowAction,
                    actioning && styles.fileRowActionDisabled,
                    pressed && !actioning && styles.pressed,
                  ]}
                >
                  {section === 'unstaged' ? (
                    <Upload color={colors.onAccent} size={14} />
                  ) : (
                    <Download color={colors.onAccent} size={14} />
                  )}
                  <Text style={styles.fileRowActionText}>{actionLabel}</Text>
                </Pressable>
              </View>
              {isOpen && (
                <View style={styles.fileDetail}>
                  {visibleDiffs.length === 0 ? (
                    <Text style={styles.fileDetailEmptyText}>
                      No inline diff for this file.
                    </Text>
                  ) : (
                    visibleDiffs.map((fileDiff) => (
                      <DiffBlock
                        key={`${fileDiff.label}:${fileDiff.path}:${fileDiff.command}`}
                        diff={fileDiff}
                        onInteractionChange={onCodeInteractionChange}
                      />
                    ))
                  )}
                </View>
              )}
            </View>
          );
        })
      )}
    </View>
  );
}

function DiffBlock({
  diff,
  onInteractionChange,
}: {
  diff: GitFileDiff;
  onInteractionChange: (active: boolean) => void;
}) {
  const { styles } = useTheme();
  const lines = diff.content.length ? diff.content.split(/\r?\n/) : [];
  const language = syntaxLanguageForPath(diff.path);
  const endInteraction = useCallback(() => {
    onInteractionChange(false);
  }, [onInteractionChange]);

  return (
    <View style={styles.diffBlock}>
      <ScrollView
        horizontal
        contentContainerStyle={styles.diffScrollerContent}
        nestedScrollEnabled
        onMomentumScrollEnd={endInteraction}
        onScrollBeginDrag={() => onInteractionChange(true)}
        onScrollEndDrag={endInteraction}
        onTouchCancel={endInteraction}
        onTouchEnd={endInteraction}
        onTouchStart={() => onInteractionChange(true)}
        showsHorizontalScrollIndicator
        style={styles.diffScroller}
      >
        <Text selectable style={styles.diffText}>
          {lines.map((line, lineIndex) => (
            <Text
              key={`${lineIndex}:${line}`}
              style={diffLineStyle(line, styles)}
            >
              {renderDiffLineTokens(line || ' ', language).map((token, tokenIndex) => (
                <Text
                  key={`${lineIndex}:${tokenIndex}:${token.text}`}
                  style={tokenStyle(token.kind, styles)}
                >
                  {token.text}
                </Text>
              ))}
              {lineIndex < lines.length - 1 ? '\n' : ''}
            </Text>
          ))}
        </Text>
      </ScrollView>
    </View>
  );
}

function ScreenHeader({
  eyebrow,
  left,
  right,
  title,
}: {
  eyebrow?: string;
  left?: ReactNode;
  right?: ReactNode;
  title: string;
}) {
  const { styles } = useTheme();

  return (
    <View style={styles.header}>
      <View style={styles.headerTop}>
        {left ?? <View style={styles.headerSide} />}
        <View style={styles.headerTextWrap}>
          <Text numberOfLines={1} style={styles.headerTitle}>
            {title}
          </Text>
          {eyebrow && (
            <Text numberOfLines={1} style={styles.headerEyebrow}>
              {eyebrow}
            </Text>
          )}
        </View>
        <View style={styles.headerSide}>
          <View style={styles.headerActions}>
            {right}
            <ThemeToggle />
          </View>
        </View>
      </View>
    </View>
  );
}

function ThemeToggle() {
  const { colors, mode, styles, toggleMode } = useTheme();
  const isDark = mode === 'dark';

  return (
    <IconButton
      accessibilityLabel={isDark ? 'Use light mode' : 'Use dark mode'}
      icon={
        isDark ? (
          <Sun color={colors.text} size={20} />
        ) : (
          <Moon color={colors.text} size={20} />
        )
      }
      onPress={toggleMode}
      style={styles.themeToggle}
    />
  );
}

function AppButton({
  compact = false,
  disabled = false,
  icon,
  label,
  onPress,
  variant = 'primary',
}: {
  compact?: boolean;
  disabled?: boolean;
  icon?: ReactNode;
  label: string;
  onPress: () => void;
  variant?: 'primary' | 'secondary';
}) {
  const { styles } = useTheme();

  return (
    <Pressable
      disabled={disabled}
      onPress={onPress}
      style={({ pressed }) => [
        styles.button,
        compact && styles.buttonCompact,
        variant === 'secondary' && styles.buttonSecondary,
        disabled && styles.buttonDisabled,
        pressed && !disabled && styles.pressed,
      ]}
    >
      {icon}
      <Text
        numberOfLines={1}
        style={[
          styles.buttonText,
          variant === 'secondary' && styles.buttonSecondaryText,
          disabled && styles.buttonDisabledText,
        ]}
      >
        {label}
      </Text>
    </Pressable>
  );
}

function IconButton({
  accessibilityLabel,
  icon,
  onPress,
  style,
}: {
  accessibilityLabel: string;
  icon: ReactNode;
  onPress: () => void;
  style?: object;
}) {
  const { styles } = useTheme();

  return (
    <Pressable
      accessibilityLabel={accessibilityLabel}
      onPress={onPress}
      style={({ pressed }) => [
        styles.iconButton,
        style,
        pressed && styles.pressed,
      ]}
    >
      {icon}
    </Pressable>
  );
}

function SegmentButton({
  active,
  icon,
  label,
  onPress,
}: {
  active: boolean;
  icon: ReactNode;
  label: string;
  onPress: () => void;
}) {
  const { styles } = useTheme();

  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [
        styles.segmentButton,
        active && styles.segmentButtonActive,
        pressed && styles.pressed,
      ]}
    >
      {icon}
      <Text
        numberOfLines={1}
        style={[
          styles.segmentText,
          active && styles.segmentTextActive,
        ]}
      >
        {label}
      </Text>
    </Pressable>
  );
}

function Chip({ label, onPress }: { label: string; onPress: () => void }) {
  const { styles } = useTheme();

  return (
    <Pressable
      onPress={onPress}
      style={({ pressed }) => [styles.chip, pressed && styles.pressed]}
    >
      <Text style={styles.chipText}>{label}</Text>
    </Pressable>
  );
}

function InlineNotice({
  text,
  tone,
}: {
  text: string;
  tone: 'error' | 'success';
}) {
  const { colors, styles } = useTheme();

  return (
    <View
      style={[
        styles.notice,
        tone === 'error' ? styles.noticeError : styles.noticeSuccess,
      ]}
    >
      {tone === 'error' ? (
        <XCircle color={colors.danger} size={18} />
      ) : (
        <CheckCircle2 color={colors.success} size={18} />
      )}
      <Text
        style={[
          styles.noticeText,
          tone === 'error' ? styles.noticeErrorText : styles.noticeSuccessText,
        ]}
      >
        {text}
      </Text>
    </View>
  );
}

function LoadingBlock({ label }: { label: string }) {
  const { colors, styles } = useTheme();

  return (
    <View style={styles.loadingBlock}>
      <ActivityIndicator color={colors.accent} />
      <Text style={styles.emptyText}>{label}</Text>
    </View>
  );
}

function EmptyState({ title }: { title: string }) {
  const { styles } = useTheme();

  return (
    <View style={styles.emptyPanel}>
      <Text style={styles.emptyTitle}>{title}</Text>
    </View>
  );
}

function kindIcon(kind: DeploymentKind, colors: ThemeColors) {
  switch (kind) {
    case 'reverse_proxy':
      return <Globe2 color={colors.accent} size={21} />;
    case 'static':
      return <FolderOpen color={colors.gold} size={21} />;
    case 'page':
      return <FileText color={colors.coral} size={21} />;
  }
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  return 'Something went wrong.';
}

function canStage(change: GitFileChange): boolean {
  return change.index_status === '?' || change.worktree_status !== ' ';
}

function canUnstage(change: GitFileChange): boolean {
  return (
    change.index_status !== ' ' &&
    change.index_status !== '?' &&
    change.index_status !== '!'
  );
}

function statusLabel(change: GitFileChange): string {
  return `${change.index_status}${change.worktree_status}`.replace(/ /g, '-');
}

function visibleDiffsForSection(
  change: GitFileChange,
  section: 'unstaged' | 'staged',
): GitFileDiff[] {
  if (section === 'unstaged') {
    return change.diffs.filter(
      (diff) => diff.label === 'Unstaged' || diff.label === 'Untracked',
    );
  }

  return change.diffs.filter((diff) => diff.label === 'Staged');
}

function toggleExpanded(
  setExpanded: (update: (current: Set<string>) => Set<string>) => void,
  key: string,
) {
  setExpanded((current) => {
    const next = new Set(current);
    if (next.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
    }
    return next;
  });
}

type SyntaxLanguage = 'plain' | 'rust' | 'javascript' | 'css' | 'html' | 'json' | 'config';
type TokenKind =
  | 'comment'
  | 'keyword'
  | 'number'
  | 'property'
  | 'punctuation'
  | 'string'
  | 'type';

type DiffToken = {
  text: string;
  kind?: TokenKind;
};

function diffLineStyle(line: string, styles: AppStyles) {
  if (
    line.startsWith('diff --git') ||
    line.startsWith('index ') ||
    line.startsWith('new file mode ') ||
    line.startsWith('deleted file mode ') ||
    line.startsWith('--- ') ||
    line.startsWith('+++ ')
  ) {
    return styles.diffLineFile;
  }
  if (line.startsWith('@@')) {
    return styles.diffLineHunk;
  }
  if (line.startsWith('+')) {
    return styles.diffLineAdd;
  }
  if (line.startsWith('-')) {
    return styles.diffLineRemove;
  }

  return undefined;
}

function tokenStyle(kind: TokenKind | undefined, styles: AppStyles) {
  switch (kind) {
    case 'comment':
      return styles.tokenComment;
    case 'keyword':
      return styles.tokenKeyword;
    case 'number':
      return styles.tokenNumber;
    case 'property':
      return styles.tokenProperty;
    case 'punctuation':
      return styles.tokenPunctuation;
    case 'string':
      return styles.tokenString;
    case 'type':
      return styles.tokenType;
    default:
      return undefined;
  }
}

function renderDiffLineTokens(line: string, language: SyntaxLanguage): DiffToken[] {
  if (isDiffHeaderLine(line) || line.startsWith('@@')) {
    return [{ text: line }];
  }

  const first = line[0];
  if (first === '+' || first === '-' || first === ' ') {
    return [{ text: first }, ...syntaxTokens(line.slice(1), language)];
  }

  return syntaxTokens(line, language);
}

function isDiffHeaderLine(line: string): boolean {
  return (
    line.startsWith('diff --git') ||
    line.startsWith('index ') ||
    line.startsWith('new file mode ') ||
    line.startsWith('deleted file mode ') ||
    line.startsWith('--- ') ||
    line.startsWith('+++ ')
  );
}

function syntaxLanguageForPath(path: string): SyntaxLanguage {
  const lower = path.toLowerCase();
  const name = lower.split(/[\\/]/).pop() ?? lower;

  if (
    [
      'cargo.toml',
      'cargo.lock',
      'package.json',
      'tsconfig.json',
      'vite.config.js',
      'vite.config.ts',
      'svelte.config.js',
      'svelte.config.ts',
    ].includes(name)
  ) {
    if (name.endsWith('.json')) {
      return 'json';
    }
    if (name.endsWith('.js') || name.endsWith('.ts')) {
      return 'javascript';
    }
    return 'config';
  }

  const extension = name.split('.').pop() ?? '';
  switch (extension) {
    case 'rs':
      return 'rust';
    case 'js':
    case 'jsx':
    case 'mjs':
    case 'cjs':
    case 'ts':
    case 'tsx':
    case 'svelte':
      return 'javascript';
    case 'css':
    case 'scss':
    case 'sass':
      return 'css';
    case 'html':
    case 'htm':
    case 'xml':
    case 'svg':
      return 'html';
    case 'json':
      return 'json';
    case 'toml':
    case 'yaml':
    case 'yml':
    case 'env':
    case 'ini':
    case 'conf':
    case 'lock':
      return 'config';
    default:
      return 'plain';
  }
}

function syntaxTokens(line: string, language: SyntaxLanguage): DiffToken[] {
  if (language === 'plain') {
    return [{ text: line }];
  }

  const tokens: DiffToken[] = [];
  let index = 0;

  while (index < line.length) {
    const rest = line.slice(index);
    const commentLength = commentTokenLength(rest, language);
    if (commentLength) {
      tokens.push({ text: rest.slice(0, commentLength), kind: 'comment' });
      index += commentLength;
      continue;
    }

    const ch = rest[0];
    if (ch === '"' || ch === "'" || ch === '`') {
      const length = stringTokenLength(rest, ch);
      const kind =
        language === 'json' && followedByColon(rest.slice(length))
          ? 'property'
          : 'string';
      tokens.push({ text: rest.slice(0, length), kind });
      index += length;
      continue;
    }

    if (language === 'css' && ch === '#') {
      const length = cssColorTokenLength(rest);
      if (length > 1) {
        tokens.push({ text: rest.slice(0, length), kind: 'number' });
        index += length;
        continue;
      }
    }

    if (isAsciiDigit(ch)) {
      const length = numberTokenLength(rest);
      tokens.push({ text: rest.slice(0, length), kind: 'number' });
      index += length;
      continue;
    }

    if (isIdentifierStart(ch)) {
      const length = identifierTokenLength(rest);
      const text = rest.slice(0, length);
      tokens.push({
        text,
        kind: identifierTokenKind(language, text, rest.slice(length)),
      });
      index += length;
      continue;
    }

    if (isPunctuation(ch)) {
      tokens.push({ text: ch, kind: 'punctuation' });
      index += 1;
      continue;
    }

    tokens.push({ text: ch });
    index += 1;
  }

  return tokens;
}

function commentTokenLength(rest: string, language: SyntaxLanguage) {
  if ((language === 'rust' || language === 'javascript') && rest.startsWith('//')) {
    return rest.length;
  }
  if (language === 'css' && rest.startsWith('/*')) {
    const end = rest.indexOf('*/');
    return end === -1 ? rest.length : end + 2;
  }
  if (language === 'html' && rest.startsWith('<!--')) {
    const end = rest.indexOf('-->');
    return end === -1 ? rest.length : end + 3;
  }
  if (language === 'config' && rest.startsWith('#')) {
    return rest.length;
  }

  return 0;
}

function stringTokenLength(rest: string, quote: string) {
  let escaped = false;
  for (let index = 1; index < rest.length; index += 1) {
    const ch = rest[index];
    if (escaped) {
      escaped = false;
    } else if (ch === '\\') {
      escaped = true;
    } else if (ch === quote) {
      return index + 1;
    }
  }

  return rest.length;
}

function cssColorTokenLength(rest: string) {
  let length = 1;
  while (length < rest.length && /[0-9a-f]/i.test(rest[length])) {
    length += 1;
  }
  return length;
}

function numberTokenLength(rest: string) {
  let length = 0;
  while (length < rest.length && /[0-9a-z_.]/i.test(rest[length])) {
    length += 1;
  }
  return length;
}

function identifierTokenLength(rest: string) {
  let length = 0;
  while (length < rest.length && isIdentifierContinue(rest[length])) {
    length += 1;
  }
  return length;
}

function isIdentifierStart(ch: string) {
  return ch === '_' || /[a-z]/i.test(ch);
}

function isIdentifierContinue(ch: string) {
  return ch === '_' || ch === '-' || /[0-9a-z]/i.test(ch);
}

function isAsciiDigit(ch: string) {
  return /[0-9]/.test(ch);
}

function isPunctuation(ch: string) {
  return '{}[]()<>;:,.=+-*/!?|&%'.includes(ch);
}

function followedByColon(rest: string) {
  return rest.trimStart().startsWith(':');
}

function identifierTokenKind(
  language: SyntaxLanguage,
  token: string,
  following: string,
): TokenKind | undefined {
  if (isKeyword(language, token)) {
    return 'keyword';
  }
  if (isTypeToken(language, token)) {
    return 'type';
  }
  if (language === 'css' && followedByColon(following)) {
    return 'property';
  }
  return undefined;
}

function isKeyword(language: SyntaxLanguage, token: string) {
  switch (language) {
    case 'rust':
      return [
        'as',
        'async',
        'await',
        'break',
        'const',
        'continue',
        'crate',
        'else',
        'enum',
        'extern',
        'false',
        'fn',
        'for',
        'if',
        'impl',
        'in',
        'let',
        'loop',
        'match',
        'mod',
        'move',
        'mut',
        'pub',
        'ref',
        'return',
        'self',
        'Self',
        'static',
        'struct',
        'super',
        'trait',
        'true',
        'type',
        'unsafe',
        'use',
        'where',
        'while',
      ].includes(token);
    case 'javascript':
      return [
        'as',
        'async',
        'await',
        'break',
        'case',
        'catch',
        'class',
        'const',
        'continue',
        'default',
        'else',
        'export',
        'extends',
        'false',
        'finally',
        'for',
        'from',
        'function',
        'if',
        'import',
        'in',
        'interface',
        'let',
        'new',
        'null',
        'return',
        'switch',
        'this',
        'throw',
        'true',
        'try',
        'type',
        'typeof',
        'var',
        'while',
      ].includes(token);
    case 'css':
      return ['and', 'from', 'important', 'keyframes', 'media', 'not', 'only', 'supports', 'to'].includes(token);
    case 'json':
      return ['false', 'null', 'true'].includes(token);
    case 'html':
      return token === 'DOCTYPE';
    default:
      return false;
  }
}

function isTypeToken(language: SyntaxLanguage, token: string) {
  if (language === 'rust') {
    return (
      [
        'bool',
        'char',
        'f32',
        'f64',
        'i8',
        'i16',
        'i32',
        'i64',
        'i128',
        'isize',
        'str',
        'String',
        'u8',
        'u16',
        'u32',
        'u64',
        'u128',
        'usize',
      ].includes(token) || /^[A-Z]/.test(token)
    );
  }

  return (language === 'javascript' || language === 'html') && /^[A-Z]/.test(token);
}

const lightColors = {
  background: '#f4f7f6',
  surface: '#ffffff',
  panel: '#eef3f1',
  text: '#18201f',
  softText: '#46524f',
  muted: '#75817e',
  border: '#d7dfdc',
  accent: '#0f766e',
  accentDark: '#115e59',
  onAccent: '#ffffff',
  danger: '#b42318',
  dangerBg: '#fff0ed',
  success: '#18794e',
  successBg: '#eaf7ef',
  gold: '#9a6b04',
  coral: '#c4553f',
  codeBg: '#f8fbfa',
  codeBorder: '#d9e4df',
  codeMuted: '#64736f',
  codeText: '#24302d',
  codeFile: '#25302d',
  codeHunkBg: '#e8f1f8',
  codeHunkText: '#245d9c',
  codeAddBg: '#eaf7ef',
  codeAddText: '#17643e',
  codeRemoveBg: '#fff0ed',
  codeRemoveText: '#a63a2c',
  tokenComment: '#71807c',
  tokenKeyword: '#7a4cc2',
  tokenNumber: '#08788a',
  tokenProperty: '#b03a5b',
  tokenPunctuation: '#73807d',
  tokenString: '#986900',
  tokenType: '#166aa3',
};

const darkColors = {
  background: '#101514',
  surface: '#171d1b',
  panel: '#202826',
  text: '#edf4f1',
  softText: '#c2cbc8',
  muted: '#8f9b97',
  border: '#2e3936',
  accent: '#2aa79c',
  accentDark: '#6dd4ca',
  onAccent: '#061413',
  danger: '#ffb3a7',
  dangerBg: '#351b18',
  success: '#8fe0ad',
  successBg: '#173323',
  gold: '#e1b95a',
  coral: '#ff9d87',
  codeBg: '#151b19',
  codeBorder: '#33413d',
  codeMuted: '#9da9a5',
  codeText: '#edf4f1',
  codeFile: '#e0e9e5',
  codeHunkBg: '#1d2d35',
  codeHunkText: '#9ec5ff',
  codeAddBg: '#183726',
  codeAddText: '#c8f2d5',
  codeRemoveBg: '#3c1e1a',
  codeRemoveText: '#ffd0ca',
  tokenComment: '#8c9995',
  tokenKeyword: '#c9b6ff',
  tokenNumber: '#73d7e7',
  tokenProperty: '#ffa5be',
  tokenPunctuation: '#9da9a5',
  tokenString: '#ffd98b',
  tokenType: '#9ed2ff',
};

function createStyles(colors: ThemeColors) {
  return StyleSheet.create({
  safeArea: {
    flex: 1,
    backgroundColor: colors.background,
  },
  flex: {
    flex: 1,
  },
  centered: {
    flex: 1,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 12,
  },
  loadingTitle: {
    color: colors.text,
    fontSize: 24,
    fontWeight: '800',
  },
  connectContent: {
    flexGrow: 1,
    justifyContent: 'center',
    gap: 18,
    padding: 20,
  },
  brandRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 14,
    marginBottom: 12,
  },
  brandMark: {
    width: 52,
    height: 52,
    borderRadius: 8,
    alignItems: 'center',
    justifyContent: 'center',
    backgroundColor: colors.accent,
  },
  appName: {
    color: colors.text,
    fontSize: 34,
    fontWeight: '900',
  },
  appSubhead: {
    color: colors.softText,
    fontSize: 15,
    fontWeight: '700',
  },
  formGroup: {
    gap: 8,
  },
  label: {
    color: colors.softText,
    fontSize: 13,
    fontWeight: '800',
    textTransform: 'uppercase',
  },
  input: {
    minHeight: 48,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    paddingHorizontal: 13,
    color: colors.text,
    backgroundColor: colors.surface,
    fontSize: 16,
  },
  quickRow: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    gap: 8,
  },
  chip: {
    minHeight: 34,
    justifyContent: 'center',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    paddingHorizontal: 12,
    backgroundColor: colors.panel,
  },
  chipText: {
    color: colors.text,
    fontWeight: '700',
  },
  header: {
    paddingHorizontal: 14,
    paddingTop: 10,
    paddingBottom: 12,
    borderBottomWidth: 1,
    borderBottomColor: colors.border,
    backgroundColor: colors.surface,
  },
  headerTop: {
    minHeight: 48,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
  },
  headerSide: {
    minWidth: 44,
    alignItems: 'flex-end',
  },
  headerTextWrap: {
    flex: 1,
    minWidth: 0,
  },
  headerTitle: {
    color: colors.text,
    fontSize: 22,
    fontWeight: '900',
  },
  headerEyebrow: {
    marginTop: 2,
    color: colors.muted,
    fontSize: 12,
    fontWeight: '700',
  },
  headerActions: {
    flexDirection: 'row',
    gap: 8,
  },
  screenContent: {
    gap: 14,
    padding: 14,
    paddingBottom: 30,
  },
  list: {
    gap: 10,
  },
  projectCard: {
    minHeight: 76,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    padding: 12,
    backgroundColor: colors.surface,
  },
  deploymentCard: {
    minHeight: 74,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 12,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    padding: 12,
    backgroundColor: colors.surface,
  },
  cardIcon: {
    width: 38,
    height: 38,
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 8,
    backgroundColor: colors.panel,
  },
  cardBody: {
    flex: 1,
    minWidth: 0,
  },
  cardTitle: {
    color: colors.text,
    fontSize: 16,
    fontWeight: '900',
  },
  cardMeta: {
    marginTop: 3,
    color: colors.muted,
    fontSize: 13,
    fontWeight: '600',
  },
  segmented: {
    flexDirection: 'row',
    gap: 8,
    padding: 10,
    borderBottomWidth: 1,
    borderBottomColor: colors.border,
    backgroundColor: colors.surface,
  },
  segmentButton: {
    flex: 1,
    minWidth: 0,
    minHeight: 42,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    backgroundColor: colors.panel,
  },
  segmentButtonActive: {
    borderColor: colors.accent,
    backgroundColor: colors.accent,
  },
  segmentText: {
    color: colors.text,
    fontSize: 13,
    fontWeight: '900',
  },
  segmentTextActive: {
    color: colors.onAccent,
  },
  tabPager: {
    flex: 1,
    overflow: 'hidden',
    position: 'relative',
  },
  tabPage: {
    ...StyleSheet.absoluteFillObject,
    flex: 1,
  },
  tabPageActive: {
    opacity: 1,
    zIndex: 1,
  },
  tabPageHidden: {
    opacity: 0,
    zIndex: 0,
  },
  webView: {
    flex: 1,
    backgroundColor: colors.surface,
  },
  terminalPanel: {
    flex: 1,
    backgroundColor: colors.background,
  },
  terminalSessionBar: {
    minHeight: 54,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
    borderBottomWidth: 1,
    borderBottomColor: colors.border,
    paddingHorizontal: 10,
    paddingVertical: 8,
    backgroundColor: colors.surface,
  },
  terminalSessionList: {
    alignItems: 'center',
    gap: 8,
    paddingRight: 2,
  },
  terminalSessionItem: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 4,
  },
  terminalSessionChip: {
    maxWidth: 156,
    minHeight: 36,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 6,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    paddingHorizontal: 10,
    backgroundColor: colors.panel,
  },
  terminalSessionChipActive: {
    borderColor: colors.accent,
    backgroundColor: colors.accent,
  },
  terminalSessionText: {
    minWidth: 0,
    color: colors.text,
    fontSize: 13,
    fontWeight: '900',
  },
  terminalSessionTextActive: {
    color: colors.onAccent,
  },
  terminalSessionClose: {
    width: 30,
    height: 30,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    backgroundColor: colors.surface,
  },
  terminalNewButton: {
    width: 38,
    height: 38,
    alignItems: 'center',
    justifyContent: 'center',
    borderRadius: 8,
    backgroundColor: colors.accent,
  },
  terminalStack: {
    flex: 1,
    position: 'relative',
    backgroundColor: colors.background,
  },
  terminalFrame: {
    ...StyleSheet.absoluteFillObject,
    opacity: 0,
    zIndex: 0,
  },
  terminalFrameActive: {
    opacity: 1,
    zIndex: 1,
  },
  button: {
    minHeight: 48,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 8,
    borderWidth: 1,
    borderColor: colors.accent,
    borderRadius: 8,
    paddingHorizontal: 16,
    backgroundColor: colors.accent,
  },
  buttonCompact: {
    minHeight: 40,
    alignSelf: 'flex-start',
    paddingHorizontal: 12,
  },
  buttonSecondary: {
    borderColor: colors.border,
    backgroundColor: colors.surface,
  },
  buttonDisabled: {
    borderColor: colors.border,
    backgroundColor: colors.panel,
  },
  buttonText: {
    color: colors.onAccent,
    fontSize: 15,
    fontWeight: '900',
  },
  buttonSecondaryText: {
    color: colors.text,
  },
  buttonDisabledText: {
    color: colors.muted,
  },
  iconButton: {
    width: 42,
    height: 42,
    alignItems: 'center',
    justifyContent: 'center',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    backgroundColor: colors.surface,
  },
  pressed: {
    opacity: 0.72,
  },
  notice: {
    minHeight: 44,
    flexDirection: 'row',
    alignItems: 'flex-start',
    gap: 8,
    borderWidth: 1,
    borderRadius: 8,
    padding: 10,
  },
  noticeError: {
    borderColor: '#f0b8ae',
    backgroundColor: colors.dangerBg,
  },
  noticeSuccess: {
    borderColor: '#b6dfc6',
    backgroundColor: colors.successBg,
  },
  noticeText: {
    flex: 1,
    fontSize: 14,
    fontWeight: '700',
  },
  noticeErrorText: {
    color: colors.danger,
  },
  noticeSuccessText: {
    color: colors.success,
  },
  loadingBlock: {
    minHeight: 96,
    alignItems: 'center',
    justifyContent: 'center',
    gap: 10,
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    backgroundColor: colors.surface,
  },
  emptyPanel: {
    minHeight: 86,
    justifyContent: 'center',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    padding: 14,
    backgroundColor: colors.surface,
  },
  emptyTitle: {
    color: colors.text,
    fontSize: 16,
    fontWeight: '900',
  },
  emptyText: {
    color: colors.muted,
    fontSize: 14,
    fontWeight: '700',
  },
  diffToolbar: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    alignItems: 'center',
    gap: 8,
  },
  commitRow: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: 8,
  },
  commitInput: {
    flex: 1,
    minWidth: 0,
  },
  diffSection: {
    gap: 8,
  },
  sectionHeading: {
    minHeight: 38,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
  },
  sectionTitle: {
    color: colors.text,
    fontSize: 18,
    fontWeight: '900',
  },
  sectionCount: {
    color: colors.muted,
    fontSize: 12,
    fontWeight: '900',
    textTransform: 'uppercase',
  },
  fileCard: {
    overflow: 'hidden',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    backgroundColor: colors.surface,
  },
  fileSummary: {
    minHeight: 64,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
    padding: 10,
  },
  fileSummaryMain: {
    flex: 1,
    minWidth: 0,
    minHeight: 44,
    flexDirection: 'row',
    alignItems: 'center',
    gap: 10,
  },
  fileRowAction: {
    minHeight: 36,
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'center',
    gap: 6,
    borderRadius: 8,
    paddingHorizontal: 10,
    backgroundColor: colors.accent,
  },
  fileRowActionDisabled: {
    backgroundColor: colors.muted,
  },
  fileRowActionText: {
    color: colors.onAccent,
    fontSize: 12,
    fontWeight: '900',
  },
  statusBadge: {
    minWidth: 38,
    overflow: 'hidden',
    borderWidth: 1,
    borderColor: colors.border,
    borderRadius: 8,
    paddingVertical: 5,
    textAlign: 'center',
    color: colors.softText,
    fontFamily: Platform.select({
      ios: 'Menlo',
      android: 'monospace',
      default: 'monospace',
    }),
    fontWeight: '900',
    backgroundColor: colors.panel,
  },
  filePath: {
    color: colors.text,
    fontSize: 14,
    fontWeight: '900',
  },
  fileDetail: {
    borderTopWidth: 1,
    borderTopColor: colors.border,
    backgroundColor: colors.codeBg,
  },
  fileDetailEmptyText: {
    color: colors.muted,
    fontSize: 14,
    fontWeight: '700',
    padding: 12,
  },
  diffBlock: {
    overflow: 'hidden',
    backgroundColor: colors.codeBg,
  },
  diffScroller: {
    backgroundColor: colors.codeBg,
  },
  diffScrollerContent: {
    paddingBottom: 8,
    paddingRight: 16,
  },
  diffText: {
    minWidth: '100%',
    paddingHorizontal: 10,
    paddingVertical: 8,
    color: colors.codeText,
    fontFamily: Platform.select({
      ios: 'Menlo',
      android: 'monospace',
      default: 'monospace',
    }),
    fontSize: 12,
    lineHeight: 19,
  },
  diffLineFile: {
    color: colors.codeFile,
    fontWeight: '900',
  },
  diffLineHunk: {
    color: colors.codeHunkText,
    backgroundColor: colors.codeHunkBg,
  },
  diffLineAdd: {
    color: colors.codeAddText,
    backgroundColor: colors.codeAddBg,
  },
  diffLineRemove: {
    color: colors.codeRemoveText,
    backgroundColor: colors.codeRemoveBg,
  },
  tokenComment: {
    color: colors.tokenComment,
    fontStyle: 'italic',
  },
  tokenKeyword: {
    color: colors.tokenKeyword,
    fontWeight: '800',
  },
  tokenNumber: {
    color: colors.tokenNumber,
  },
  tokenProperty: {
    color: colors.tokenProperty,
  },
  tokenPunctuation: {
    color: colors.tokenPunctuation,
  },
  tokenString: {
    color: colors.tokenString,
  },
  tokenType: {
    color: colors.tokenType,
  },
  themeToggle: {
    marginLeft: 0,
  },
});
}
