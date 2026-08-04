import 'react-native-gesture-handler';

import { AppContent } from './src/AppContent';
import { ApiProvider } from './src/core/api/ApiContext';
import { SessionProvider } from './src/core/session/SessionContext';
import { ThemeProvider } from './src/theme';

export default function App() {
  return (
    <ThemeProvider>
      <SessionProvider>
        <ApiProvider>
          <AppContent />
        </ApiProvider>
      </SessionProvider>
    </ThemeProvider>
  );
}
