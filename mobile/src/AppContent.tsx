import { LoadingScreen, Shell } from './components/Shell';
import { useSession } from './core/session/SessionContext';
import { RootNavigator } from './application/RootNavigator';
import { ConnectScreen } from './screens/ConnectScreen';

export function AppContent() {
  const { booting, clearError, error, login, rememberedBaseUrl, session } =
    useSession();

  if (booting) {
    return (
      <Shell>
        <LoadingScreen />
      </Shell>
    );
  }

  if (!session) {
    return (
      <Shell>
        <ConnectScreen
          error={error}
          initialBaseUrl={rememberedBaseUrl}
          onClearError={clearError}
          onLogin={login}
        />
      </Shell>
    );
  }

  return (
    <Shell>
      <RootNavigator />
    </Shell>
  );
}
